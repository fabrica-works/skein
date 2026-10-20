"""Finding decisions over supplied records and verified carriage facts (lab.md, 9).

A carriage fact is keyed by (digest of fix evidence, key) and is True, False,
or absent (unavailable). The host must establish it independently. No event's
claim, cached verdict, hash ordering or provenance supplies such a fact.
"""
from . import canonical, judge
from .records import FINDING_ID, Refused, digest_value, finding_refusals, fix_refusals


def event(name, t, by, **fields):
    value = dict(format='lab-finding/1', event=name, t=t, by=by, **fields)
    problems = finding_refusals(value)
    if problems:
        raise Refused(problems[0].member, problems[0].why)
    return value


def _owner(value):
    if value['by'] in ('runner', 'triager'):
        raise Refused('finding.by', 'owner action requires an owner, not runner or triager')


def _records(evidence, key):
    if not digest_value(key) or key not in evidence:
        raise Refused('finding.key', 'verified evidence unavailable')
    return evidence[key]


def _signature(run, signature):
    return any(f['signature'] == signature for f in run['failures'])


def _failure(value, signature, evidence):
    runs = [r for r in _records(evidence, value['key']) if r['run'] == value['run']]
    if len(runs) != 1 or runs[0]['outcome'] != 'fail' or runs[0]['seed'] != value['seed']:
        raise Refused('finding.run', 'failure must reference one verified judged failing run and its seed')
    failures = [f for f in runs[0]['failures'] if f['id'] == value['failure'] and f['signature'] == signature]
    if len(failures) != 1:
        raise Refused('finding.failure', 'failure identity or signature differs')
    return failures[0]


def occurrence(value):
    return tuple(value[k] for k in ('key', 'run', 'failure'))


def reference(run, failure):
    return dict(key=run['key'], run=run['run'], seed=run['seed'], failure=failure['id'])


def _lines(runs):
    return [dict(format='lab-evidence/1', **{k: r[k] for k in ('run', 'seed', 'attempt', 'outcome', 'ended')},
                 signatures=sorted({f['signature'] for f in r['failures']})) for r in runs]


def snapshot(key, runs):
    return judge.verdict_record(key, _lines(runs))


def _snapshot_runs(value, evidence):
    """Verify a historical verdict against a prefix of retained evidence."""
    if type(value) is not dict or type(value.get('seeds')) is not list:
        raise Refused('finding.verdict', 'invalid verdict snapshot')
    key = value.get('key')
    runs = _records(evidence, key)
    try:
        ids = [r for seed in value['seeds'] for field in ('judged', 'not_judged') for r in seed[field]]
        if len(set(ids)) != len(ids):
            raise Refused('finding.verdict', 'duplicate snapshot run')
        prefix = runs[:len(ids)]
        if {r['run'] for r in prefix} != set(ids) or snapshot(key, prefix) != value:
            raise Refused('finding.verdict', 'snapshot is not a verified evidence prefix')
    except (KeyError, TypeError) as error:
        raise Refused('finding.verdict', 'invalid verdict snapshot') from error
    return prefix


def reproduction(state, evidence):
    """Every attempt after the first signature failure of each key/seed counts."""
    reruns = []
    for key in sorted({v['key'] for v in state['occurrences']}):
        seen = set()
        for run in _records(evidence, key):
            if run['seed'] in seen:
                reruns.append(dict(key=key, run=run['run'], seed=run['seed'], outcome=run['outcome'],
                                   reproduced=_signature(run, state['signature'])))
            if run['outcome'] == 'fail' and _signature(run, state['signature']):
                seen.add(run['seed'])
    return dict(numerator=sum(r['reproduced'] for r in reruns), denominator=len(reruns), reruns=reruns)


def filing(state, evidence):
    first = state['occurrences'][0]
    runs = _records(evidence, first['key'])
    verdict = snapshot(first['key'], runs)
    if verdict['verdict'] not in ('fail', 'flaky'):
        raise Refused('finding.file', 'fail or flaky from exactly three judged seeds required')
    start = next(i for i, r in enumerate(runs) if r['run'] == first['run'])
    reruns = [dict(run=r['run'], outcome=r['outcome'], reproduced=_signature(r, state['signature']))
              for r in runs[start + 1:] if r['seed'] == first['seed']]
    if not reruns:
        raise Refused('finding.file', 'first failing seed must be rerun, including its result')
    return dict(verdict=verdict, flaky=any(not _signature(r, state['signature']) for r in runs
                                         if r['outcome'] in ('pass', 'fail')), reruns=reruns)


def carries(state, key, carried):
    answer = carried.get((canonical.digest(state['fix']), key))
    if type(answer) is not bool:
        raise Refused('finding.fix', 'independently verified fix carriage unavailable for key ' + key)
    return answer


def fold(events, evidence, carried=None):
    carried = {} if carried is None else carried
    state = None
    for value in events:
        problems = finding_refusals(value)
        if problems:
            raise Refused(problems[0].member, problems[0].why)
        name = value['event']
        if state is None:
            if name != 'candidate':
                raise Refused('finding.event', 'history must start with candidate')
            failure = _failure(value, value['signature'], evidence)
            if any(value[k] != failure[k] for k in ('kind', 'summary')):
                raise Refused('finding.candidate', 'candidate differs from failure')
            if failure['kind'] == 'use' and value['by'] != 'triager':
                raise Refused('finding.candidate', 'use failure requires triager proposal')
            if value['by'] not in ('runner', 'triager'):
                raise Refused('finding.by', 'candidate requires runner or triager')
            state = dict(signature=value['signature'], kind=value['kind'], summary=value['summary'],
                         title=value['summary'], state='candidate', **{'class': None}, proposed=None,
                         owner=None, fix=None, recheck=None, occurrences=[value])
            continue
        phase = state['state']
        if name in ('candidate',):
            raise Refused('finding.event', 'candidate may only start history')
        if name in ('occurred', 'reopened'):
            _failure(value, state['signature'], evidence)
            repeated = occurrence(value) in {occurrence(v) for v in state['occurrences']}
            owner_reopening = name == 'reopened' and phase == 'closed' and state['class'] == 'lab'
            if repeated and not owner_reopening:
                raise Refused('finding.run', 'duplicate failure occurrence')
            if name == 'reopened':
                if phase != 'closed':
                    raise Refused('finding.reopened', 'only closed finding can reopen')
                _require_carried(state, value, carried)
                state.update(state='open', owner=None, fix=None, recheck=None)
            elif phase == 'closed' and state['class'] != 'lab' and carries(state, value['key'], carried):
                raise Refused('finding.occurred', 'recurrence carrying fix must reopen')
            elif phase == 'rechecked':
                state.update(state='fixed', recheck=None)
            if not repeated:
                state['occurrences'].append(value)
        elif name in ('proposed', 'classed'):
            if phase not in ('candidate', 'open', 'taken'):
                raise Refused('finding.class', 'class cannot change after fix')
            if name == 'proposed':
                if value['by'] != 'triager':
                    raise Refused('finding.by', 'only triager proposes')
                state.update(proposed=value['class'], title=value['title'])
            else:
                _owner(value)
                state['class'] = value['class']
        elif name == 'filed':
            _owner(value)
            if phase != 'candidate' or not (state['class'] or state['proposed']):
                raise Refused('finding.file', 'candidate and proposed or confirmed class required')
            prefix = _snapshot_runs(value['verdict'], evidence)
            first_key = state['occurrences'][0]['key']
            expected = filing(state, {**evidence, first_key: prefix})
            if any(value[k] != expected[k] for k in expected) or value['verdict']['key'] != first_key:
                raise Refused('finding.file', 'filing evidence differs')
            state.update(state='open', flaky=value['flaky'])
        elif name == 'taken':
            _owner(value)
            if phase != 'open':
                raise Refused('finding.take', 'open finding required')
            state.update(state='taken', owner=value['by'])
        elif name == 'fixed':
            _owner(value)
            if phase != 'taken' or state['class'] is None:
                raise Refused('finding.fix', 'taken finding and confirmed class required')
            problems = fix_refusals(value['fix'], state['class'])
            if problems:
                raise Refused(problems[0].member, problems[0].why)
            state.update(state='fixed', fix=value['fix'], recheck=None)
        elif name == 'rechecked':
            _owner(value)
            if phase not in ('fixed', 'rechecked'):
                raise Refused('finding.recheck', 'fixed finding required')
            prefix = _snapshot_runs(value['verdict'], evidence)
            if (value['key'] != value['verdict']['key'] or value['key'] in {v['key'] for v in state['occurrences']}
                    or value['verdict']['verdict'] != 'pass'
                    or any(_signature(r, state['signature']) for r in prefix if r['outcome'] in ('pass', 'fail'))):
                raise Refused('finding.recheck', 'new key, exactly three passing seeds and no judged signature required')
            _require_carried(state, value, carried)
            state.update(state='rechecked', recheck=value)
        elif name == 'closed':
            _owner(value)
            if phase != 'rechecked':
                raise Refused('finding.close', 'verified recheck required')
            state['state'] = 'closed'
    if state is None:
        raise Refused('finding', 'empty history')
    return state


def _require_carried(state, value, carried):
    if state['class'] == 'lab':
        _owner(value)
        if not value.get('judgment'):
            raise Refused('finding.judgment', 'lab fix requires explicit owner judgment')
    if not carries(state, value['key'], carried):
        raise Refused('finding.fix', 'key does not carry fix')


def reconcile(histories, runs, evidence, t, carried=None):
    """Return additive (id, event) decisions; use failures await triage."""
    _ids(histories)
    states = {i: fold(h, evidence, carried) for i, h in histories.items()}
    signatures = [s['signature'] for s in states.values()]
    if len(set(signatures)) != len(signatures):
        raise Refused('findings', 'multiple findings have the same signature')
    additions = []
    for run in runs:
        if run['outcome'] not in ('pass', 'fail'):
            continue
        for failure in run['failures']:
            if failure['kind'] == 'use':
                continue
            matches = [i for i, s in states.items() if s['signature'] == failure['signature']]
            fields = reference(run, failure)
            if not matches:
                identity = f'F-{len(states) + 1:04d}'
                value = event('candidate', t if t is not None else run['ended'], 'runner', signature=failure['signature'], kind=failure['kind'],
                              summary=failure['summary'], **fields)
                histories = {**histories, identity: [value]}
            else:
                identity = matches[0]
                state = states[identity]
                if occurrence(fields) in {occurrence(v) for v in state['occurrences']}:
                    continue
                name = 'occurred'
                if state['state'] == 'closed' and state['class'] != 'lab' and carries(state, run['key'], carried or {}):
                    name = 'reopened'
                value = event(name, t if t is not None else run['ended'], 'runner', **fields)
                histories = {**histories, identity: histories[identity] + [value]}
            states[identity] = fold(histories[identity], evidence, carried)
            additions.append((identity, value))
    return additions


def index(histories, evidence, carried=None):
    _ids(histories)
    result = []
    for identity in sorted(histories, key=lambda i: int(i[2:])):
        state = fold(histories[identity], evidence, carried)
        result.append(dict(id=identity, signature=state['signature'], state=state['state'],
                           title=state['title'], **{'class': state['class']}, proposed=state['proposed'],
                           runs=list(dict.fromkeys(v['run'] for v in state['occurrences'])),
                           reproduction=reproduction(state, evidence)))
    if len({v['signature'] for v in result}) != len(result):
        raise Refused('findings', 'multiple findings have the same signature')
    return dict(format='lab-finding-index/1', findings=result)


def _ids(histories):
    if (any(type(i) is not str or not FINDING_ID.fullmatch(i) for i in histories)
            or sorted(int(i[2:]) for i in histories) != list(range(1, len(histories) + 1))):
        raise Refused('findings.id', 'ids must be contiguous from F-0001')
