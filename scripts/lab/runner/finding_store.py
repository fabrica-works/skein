"""Finding files, retained evidence and owner commands (lab.md, 9).

Callers hold Lab.lock throughout. This adapter starts no processes. Product
ancestry and runner-commit proofs are refused until their host verifier exists.
"""
import os
from . import canonical, evidence, findings, store
from .records import (FINDING_ID, NAME, Refused, digest_value, finding_refusals, fix_refusals, lab_refusals, shape)


def _names(lab, path):
    try:
        fd = lab.directory(path)
    except FileNotFoundError:
        return []
    try:
        return sorted(os.listdir(fd))
    finally:
        os.close(fd)


def sources(lab):
    result, paths = {}, {}
    for name in _names(lab, 'keys'):
        if not NAME.fullmatch(name):
            raise Refused('keys', 'invalid experiment directory')
        for key in _names(lab, 'keys/' + name):
            if not digest_value(key) or key in result:
                raise Refused('keys', 'invalid or duplicate key directory')
            path = 'keys/' + name + '/' + key
            lines = evidence.read(lab, path)
            result[key] = [canonical.loads(lab.read(f"{path}/runs/{line['run']}/run.json")) for line in lines]
            for run in result[key]:
                for failure in run['failures']:
                    for file in failure['evidence']:
                        if file.split('/')[0] in ('scratch', 'dirs'):
                            raise Refused('finding.evidence', 'scratch is not retained evidence')
                        lab.read(f"{path}/runs/{run['run']}/{file}")
            paths[key] = path
    return result, paths


def histories(lab):
    result = {}
    for name in _names(lab, 'findings'):
        if name == 'index.json':
            continue  # Derived bytes are never evidence, even when corrupt.
        identity = name.removesuffix('.jsonl')
        if name != identity + '.jsonl' or not FINDING_ID.fullmatch(identity) or int(identity[2:]) == 0:
            raise Refused('findings', 'unexpected finding file ' + name)
        result[identity] = [canonical.loads(line) for line in lab.read('findings/' + name).splitlines(keepends=True)]
    if sorted(int(i[2:]) for i in result) != list(range(1, len(result) + 1)):
        raise Refused('findings', 'finding ids must be contiguous from F-0001')
    return result


def verified_key(lab, key, paths):
    if key not in paths:
        raise Refused('recheck.key', 'retained key unavailable')
    value = canonical.loads(lab.read(paths[key] + '/key.json'))
    problems = []
    shape(value, ('format', 'experiment', 'subject', 'lab'), ('about',), 'key', problems)
    problems.extend(lab_refusals(value.get('lab')))
    if (problems or value.get('format') != 'lab-key/1' or canonical.digest(value) != key
            or not digest_value(value.get('experiment')) or not digest_value(value.get('subject'))):
        raise Refused('key', 'invalid key or identity differs')
    verified_experiment(lab, value['experiment'])
    # Reuse the input verifier, including source/artifact bytes and frozen modes.
    from .cli import _subject
    _subject(lab, value['subject'])
    return value


def verified_experiment(lab, identity):
    exp_path = 'store/experiments/' + identity
    fd = lab.directory(exp_path)
    try:
        lab._verify_frozen(fd)
    finally:
        os.close(fd)
    files = store.snapshot(lab.root / exp_path)
    if canonical.tree_digest(store.entries(files)) != identity:
        raise Refused('key.experiment', 'retained experiment differs from key')


def carriage(lab, history, records, paths, *, extra_key=None):
    """Only corrected-experiment equality is currently host-verifiable."""
    facts = {}
    fix, finding_class, closed = None, None, False
    for value in history:
        if value['event'] == 'classed':
            finding_class = value['class']
        elif value['event'] == 'fixed':
            fix = value['fix']
            problems = fix_refusals(fix, finding_class)
            if problems:
                raise Refused(problems[0].member, problems[0].why)
            if finding_class == 'experiment':
                verified_experiment(lab, fix['experiment'])
                origin = verified_key(lab, history[0].get('key'), paths)
                if origin['experiment'] == fix['experiment']:
                    raise Refused('fix.experiment', 'corrected experiment must differ from original')
        elif value['event'] in ('rechecked', 'reopened', 'occurred') and fix is not None:
            if value['event'] != 'occurred' or (closed and finding_class != 'lab'):
                _carriage_fact(lab, finding_class, fix, value['key'], paths, facts)
            if value['event'] == 'reopened':
                closed = False
        elif value['event'] == 'closed':
            closed = True
    if fix is not None:
        keys = [extra_key] if extra_key is not None else []
        # New failures after closure need proof before automatic reopening.
        if closed:
            signature = history[0]['signature']
            keys.extend(key for key, runs in records.items() if any(
                r['outcome'] == 'fail' and any(f['signature'] == signature for f in r['failures']) for r in runs))
        for key in dict.fromkeys(keys):
            if finding_class != 'lab' or extra_key is not None:
                _carriage_fact(lab, finding_class, fix, key, paths, facts)
    return facts


def _carriage_fact(lab, finding_class, fix, key, paths, facts):
    if finding_class != 'experiment':
        why = ('lab recheck needs a verified mapping from key.lab.runner to runner_commit and explicit owner judgment'
               if finding_class == 'lab' else
               'product recheck/recurrence needs an independently verified product repository binding, subject commit and Git ancestry from product_commit; provenance is not proof')
        raise Refused('finding.proof', why)
    value = verified_key(lab, key, paths)
    facts[(canonical.digest(fix), key)] = value['experiment'] == fix['experiment']


def load(lab):
    records, paths = sources(lab)
    history = histories(lab)
    facts = {}
    for values in history.values():
        # Shape validation precedes field access in host proof collection.
        for value in values:
            problems = finding_refusals(value)
            if problems:
                raise Refused(problems[0].member, problems[0].why)
        facts.update(carriage(lab, values, records, paths))
    findings.index(history, records, facts)
    return history, records, paths, facts


def sync(lab, t):
    """Reconcile published runs idempotently; partial/corrupt appends refuse."""
    history, records, _paths, facts = load(lab)
    runs = [run for key in sorted(records) for run in records[key]]
    additions = findings.reconcile(history, runs, records, t, facts)
    for identity, value in additions:
        lab.append_line('findings/' + identity + '.jsonl', value)
        history.setdefault(identity, []).append(value)
    os.close(lab.directory('findings', create=True))
    derived = findings.index(history, records, facts)
    lab.replace_derived('findings/index.json', derived)
    return derived


def act(lab, identity, action, t, by, *, finding_class=None, fix=None, recheck=None):
    if type(identity) is not str or not FINDING_ID.fullmatch(identity):
        raise Refused('finding.id', 'expected F-0001 or later')
    sync(lab, t)
    history, records, paths, facts = load(lab)
    if identity not in history:
        raise Refused('finding.id', 'finding unavailable')
    values = history[identity]
    state = findings.fold(values, records, facts)
    if action == 'file':
        additions = [findings.event('filed', t, by, **findings.filing(state, records))]
    elif action == 'class':
        additions = [findings.event('classed', t, by, **{'class': finding_class})]
    elif action == 'take':
        additions = [findings.event('taken', t, by)]
    elif action == 'fix':
        additions = [findings.event('fixed', t, by, fix=fix)]
    elif action == 'close':
        if not digest_value(recheck):
            raise Refused('recheck.key', 'expected key digest')
        verified_key(lab, recheck, paths)
        if (state['state'] == 'rechecked' and state['recheck']['key'] == recheck
                and state['recheck']['verdict'] == findings.snapshot(recheck, records[recheck])):
            additions = [findings.event('closed', t, by)]
        else:
            if state['fix'] is None:
                raise Refused('finding.close', 'fixed finding required')
            facts.update(carriage(lab, values, records, paths, extra_key=recheck))
            additions = [findings.event('rechecked', t, by, key=recheck, verdict=findings.snapshot(recheck, records[recheck])),
                         findings.event('closed', t, by)]
    else:
        raise Refused('finding.action', 'unsupported action')
    # Decide and verify the entire command before the first append.
    findings.fold(values + additions, records, facts)
    if action == 'fix':
        carriage(lab, values + additions, records, paths)
    for value in additions:
        lab.append_line('findings/' + identity + '.jsonl', value)
    return sync(lab, t)


def append_triage(lab, additions):
    """The triage planner verifies the entire batch through the ordinary fold first."""
    history, records, _paths, facts = load(lab)
    for identity, value in additions:
        history.setdefault(identity, []).append(value)
    derived = findings.index(history, records, facts)
    for identity, value in additions:
        lab.append_line('findings/' + identity + '.jsonl', value)
    lab.replace_derived('findings/index.json', derived)
    return additions
