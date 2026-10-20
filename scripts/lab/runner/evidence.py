"""Run publication and evidence verification; no recovery or pruning (lab.md, 8)."""
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


def publish(lab, key_path, record):
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
    lab.replace_derived(key_path + '/verdict.json', judge.verdict_record(record['key'], previous))
    # Publication failure leaves the worker's live marker intact. A repeated
    # publish verifies immutable bytes and reconciles without duplicating events.
    from .finding_store import sync
    sync(lab, record['ended'])


def require_recovered(lab):
    """Inspect run metadata only; retain unfinished runs for owner recovery."""
    try:
        root = lab.directory('keys')
    except FileNotFoundError:
        return
    try:
        experiments = os.listdir(root)
    finally:
        os.close(root)
    for name in sorted(experiments):
        parent = lab.directory('keys/' + name)
        try:
            keys = os.listdir(parent)
        finally:
            os.close(parent)
        for key in sorted(keys):
            published = {r['run'] for r in read(lab, 'keys/' + name + '/' + key)}
            prefix = 'keys/' + name + '/' + key + '/runs'
            try:
                runs = lab.directory(prefix)
            except FileNotFoundError:
                continue
            try:
                names = os.listdir(runs)
            finally:
                os.close(runs)
            for run in sorted(names):
                path = prefix + '/' + run
                directory = lab.directory(path)
                try:
                    files = os.listdir(directory)
                finally:
                    os.close(directory)
                if 'live.json' in files or 'run.json' not in files or run not in published:
                    raise Refused('recovery', path + ': unfinished run retained; owner recovery required')
