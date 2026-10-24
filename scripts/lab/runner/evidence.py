"""Run publication and evidence verification; interrupted-unit recovery, no pruning (lab.md, 8)."""
import os
from . import canonical, judge
from .records import Refused, evidence_refusals, run_refusals


def read(lab, key_path):
    try:
        data = lab.read(key_path + '/evidence.jsonl')
    except FileNotFoundError:
        return []
    result, runs, attempts = [], set(), {}
    for line in data.splitlines(keepends=True):
        item = canonical.loads(line)
        problems = evidence_refusals(item)
        if problems:
            raise Refused(problems[0].member, problems[0].why)
        if item['run'] in runs or item['attempt'] != attempts.get(item['seed'], 0) + 1:
            raise Refused('evidence', 'duplicate run or nonadditive attempt')
        record = canonical.loads(lab.read(f"{key_path}/runs/{item['run']}/run.json"))
        problems = run_refusals(record)
        if problems:
            raise Refused(problems[0].member, problems[0].why)
        if 'rehearsal' in record:
            raise Refused('evidence', 'a rehearsal is never evidence')
        if record['key'] != key_path.rsplit('/', 1)[-1] or line_for(record) != item:
            raise Refused('evidence', 'run record and evidence differ')
        runs.add(item['run'])
        attempts[item['seed']] = item['attempt']
        result.append(item)
    judge.verdict(result)
    return result


def line_for(record):
    return dict(format='lab-evidence/1', **{k: record[k] for k in ('run', 'seed', 'attempt', 'outcome', 'ended')},
                signatures=sorted(set(f['signature'] for f in record['failures'])))


def derive_verdict(lab, key_path, previous):
    """Read the current class and retained bounds for a published verdict (lab.md, 8)."""
    from .cli import _runtime_cost
    key = canonical.loads(lab.read(key_path + '/key.json'))
    current = _runtime_cost(lab, key)
    bounds = {line['run']: canonical.loads(lab.read(
        f"{key_path}/runs/{line['run']}/run.json"))['bounds'] for line in previous}
    derived = judge.derive_verdict(key_path.rsplit('/', 1)[-1], previous, bounds, current=current)
    lab.replace_derived(key_path + '/verdict.json', derived)
    return derived


def publish(lab, key_path, record):
    if 'rehearsal' in record:
        raise Refused('run.rehearsal', 'a rehearsal is never evidence')
    problems = run_refusals(record)
    if problems:
        raise Refused(problems[0].member, problems[0].why)
    if key_path != f"keys/{record['experiment']}/{record['key']}":
        raise Refused('run.key', 'publication address differs from run identity')
    previous = read(lab, key_path)
    if not judge.seed_admitted(previous, record['seed']):
        raise Refused('seed', 'a fourth seed is forbidden')
    existing = [r for r in previous if r['run'] == record['run']]
    attempt = 1 + sum(r['seed'] == record['seed'] for r in previous)
    if not existing and record['attempt'] != attempt:
        raise Refused('attempt', 'run is not additive')
    lab.write_once(f"{key_path}/runs/{record['run']}/run.json", canonical.record_bytes(record))
    if not existing:
        lab.append_line(key_path + '/evidence.jsonl', line_for(record))
        previous.append(line_for(record))
    derive_verdict(lab, key_path, previous)
    # Publication failure leaves the worker's live marker intact. A repeated
    # publish verifies immutable bytes and reconciles without duplicating events.
    from .finding_store import sync
    sync(lab, record['ended'])


def _names(lab, path):
    try:
        fd = lab.directory(path)
    except FileNotFoundError:
        return []
    try:
        return sorted(os.listdir(fd))
    finally:
        os.close(fd)


def _finish(lab, path, record_name, writing, key_path=None):
    files = _names(lab, path)
    complete = record_name in files
    live = 'live.json' in files
    if complete and not live:
        return
    kind = 'triage' if record_name == 'triage.json' else 'rehearsal' if path.startswith('rehearsals/') else 'run'
    refusal = path + ': unfinished ' + kind + ' retained; owner recovery required'
    if not writing or not live:
        raise Refused('recovery', refusal)
    marker = canonical.loads(lab.read(path + '/live.json'))
    context = marker.get('context')
    if complete:
        context = canonical.loads(lab.read(path + '/' + record_name))
    if not complete and type(context) is not dict:
        raise Refused('recovery', refusal)
    from . import recovery
    from .host import manager, CapabilityError
    import subprocess
    from .loop import utc
    import shutil
    systemctl = marker.get('systemctl') or (shutil.which('systemctl', path=os.defpath) if complete else None)
    if type(context) is not dict or systemctl is None:
        raise Refused('recovery', refusal)
    try:
        answer = manager(systemctl, ['show', '--property=ActiveState,LoadState', marker['unit']],
                         context['bounds']['command_seconds'])
        ended = recovery.unit_ended(answer)
    except (OSError, subprocess.SubprocessError, CapabilityError, ValueError, KeyError) as error:
        raise Refused('recovery', path + ': manager unavailable: ' + str(error)) from error
    if not ended:
        raise Refused('recovery', path + ': run in progress: ' + marker['unit'])
    if complete:
        record = canonical.loads(lab.read(path + '/' + record_name))
    else:
        try:
            data = lab.read(path + '/events.jsonl')
        except FileNotFoundError:
            data = b''
        from . import held
        taken = []
        for name in marker.get('held', []):
            handoff = held.taken_record(lab, name)
            if handoff is not None and handoff['run'] == context['run']:
                taken.append(name)
        try:
            driver_data = lab.read(path + '/driver/events.jsonl') if record_name == 'run.json' else None
        except FileNotFoundError:
            driver_data = None
        record, event = recovery.interrupted(context, data, utc(), taken, driver_data)
        # Preserve a cut fragment byte for byte, separating the correction line.
        lab.append_line(path + '/events.jsonl', event, prefix=b'\n' if data and not data.endswith(b'\n') else b'')
        from .records import triage_refusals
        problems = (run_refusals if record_name == 'run.json' else triage_refusals)(record)
        if problems:
            raise Refused(problems[0].member, problems[0].why)
        lab.write_once(path + '/' + record_name, canonical.record_bytes(record))
    if record_name == 'run.json' and 'rehearsal' not in record:
        publish(lab, key_path, record)
    elif record_name == 'triage.json' and 'rehearsal' not in record:
        from .finding_store import sync
        sync(lab, record['ended'])
    lab.remove_live(path + '/live.json')


def require_recovered(lab, *, writing=False):
    """Writers sweep ended units; readers refuse unswept metadata (lab.md, 3.2)."""
    for experiment in _names(lab, 'rehearsals'):
        if experiment == 'triage':
            for run in _names(lab, 'rehearsals/triage'):
                for n in _names(lab, 'rehearsals/triage/' + run):
                    _finish(lab, f'rehearsals/triage/{run}/{n}', 'triage.json', writing)
        else:
            for run in _names(lab, 'rehearsals/' + experiment):
                _finish(lab, f'rehearsals/{experiment}/{run}', 'run.json', writing)
    for name in _names(lab, 'keys'):
        for key in _names(lab, 'keys/' + name):
            key_path = f'keys/{name}/{key}'
            for run in _names(lab, key_path + '/runs'):
                path = key_path + '/runs/' + run
                _finish(lab, path, 'run.json', writing, key_path)
                published = {r['run'] for r in read(lab, key_path)}
                if run not in published:
                    raise Refused('recovery', path + ': unpublished record; owner recovery required')
                for n in _names(lab, path + '/triage'):
                    _finish(lab, path + '/triage/' + n, 'triage.json', writing)
