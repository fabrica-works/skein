"""The subject -> validate -> key command path (lab.md, 10.2)."""
import argparse
from datetime import datetime, timezone
import errno
import json
import os
from pathlib import Path
import re
import shutil
import secrets
import subprocess
import stat
import sys
from . import brake, canonical, codex, evidence, experiment, finding_store, held, judge, store
from .records import CLASSES, Refusal, Refused, cost_refusals, digest_value, relative, subject_refusals


def _program(path):
    if type(path) is not str or not path.startswith("/"):
        raise Refused("program", "expected an absolute executable path")
    resolved = os.path.realpath(path)
    data, executable = store.read_regular(resolved, allow_hardlinks=True)
    if not executable:
        raise Refused(path, "program has no owner execute bit")
    return {"path": resolved, "sha256": store.file_digest(data), "regular": True, "exec": True}


def _runner_root():
    return Path(__file__).resolve().parent.parent


def _browser_facts(browser):
    """Read only an explicitly requested browser's host identities."""
    known, facts, problems = {}, {}, []
    problems.extend(experiment.browser_settings_refusals(browser))
    if problems:
        return known, facts, problems
    known.update({'browser.program': browser['program'], 'browser.chromium': browser['chromium']['path'],
                  'browser.fonts': browser['fonts']['path']})
    for field, path in (('program', browser['program']), ('chromium', browser['chromium']['path'])):
        try:
            facts[field] = _program(path)
        except (OSError, Refused) as error:
            problems.append(Refusal('settings.browser.' + field, str(error)))
    try:
        facts['fonts'] = canonical.tree_digest(store.walk(browser['fonts']['path']))
    except (OSError, Refused) as error:
        problems.append(Refusal('settings.browser.fonts', str(error)))
    return known, facts, problems


def _program_facts(settings, used):
    """Inspect only the named programs the experiment's templates request."""
    known, facts, problems = {}, {}, []
    programs = settings.get('programs', {})
    if type(programs) is not dict:
        return known, facts, problems
    for name in sorted(used):
        configured = programs.get(name)
        if type(configured) is not dict:
            continue
        member = 'settings.programs.' + name
        if experiment.programs_settings_refusals({name: configured}):
            continue
        known['program.' + name] = configured['path']
        try:
            facts[configured['path']] = _program(configured['path'])
        except (OSError, Refused) as error:
            problems.append(Refusal(member + '.path', str(error)))
    return known, facts, problems


def _runtime_cost(lab, key):
    settings = canonical.loads(lab.read('settings.json'))
    name = key['lab']['cost']['name']
    costs = settings.get('costs')
    selected = costs.get(name) if type(costs) is dict else None
    problems = cost_refusals(selected, 'settings.costs.' + name)
    if problems:
        raise Refused(problems[0].member, '; '.join(str(p) for p in problems))
    return {'name': name, **selected}


def _superseded(lab, key_path, previous, current):
    result = set()
    for line in previous:
        if (line['outcome'] != 'fail' or not line['signatures']
                or not all(signature.startswith('bound/') for signature in line['signatures'])):
            continue
        record = canonical.loads(lab.read(f"{key_path}/runs/{line['run']}/run.json"))
        if judge.superseded(line['signatures'], record['bounds'], current):
            result.add(line['run'])
    return frozenset(result)


def _subject(lab, identity):
    if not digest_value(identity):
        raise Refused("subject", "expected sha256: followed by 64 lowercase hex digits")
    root = f"store/subjects/{identity}"
    fd = lab.directory(root)
    try:
        lab._verify_frozen(fd)
    finally:
        os.close(fd)
    files = store.snapshot(lab.root / root)
    by_path = {f["path"]: f for f in files}
    if "pruned.json" in by_path:
        raise Refused("subject", "subject is pruned")
    if "subject.json" not in by_path:
        raise Refused("subject", "missing subject.json")
    subject = canonical.loads(by_path["subject.json"]["data"])
    problems = subject_refusals(subject)
    if problems:
        raise Refused(problems[0].member, problems[0].why)
    if canonical.digest(subject) != identity:
        raise Refused("subject", "stored record does not match its identity")
    wanted = {"subject.json"}
    for name, artifact in subject["artifacts"].items():
        path = "artifacts/" + name
        wanted.add(path)
        file = by_path.get(path)
        if file is None or {"sha256": store.file_digest(file["data"]), "bytes": len(file["data"]), "exec": file["exec"]} != artifact:
            raise Refused("subject.artifacts." + name, "artifact missing or content identity differs")
    source = [{**f, "path": f["path"][7:]} for f in files if f["path"].startswith("source/")]
    wanted.update("source/" + f["path"] for f in source)
    if subject["source"] is None:
        if source:
            raise Refused("subject.source", "unexpected source files")
    else:
        source_fd = lab.directory(root + "/source")
        os.close(source_fd)
        if canonical.tree_digest(store.entries(source)) != subject["source"]:
            raise Refused("subject.source", "source identity differs")
    if set(by_path) != wanted:
        raise Refused("subject", "unexpected stored input files")
    return subject, lab.root / root


def _machine_facts(lab):
    floor = 40
    try:
        data, _ = store.read_regular(lab.root.parent / "machine.env")
        for line in data.decode("utf-8", "strict").splitlines():
            if re.fullmatch(r"PLAN_MIN_DISK_GB=[0-9]+", line):
                floor = int(line.split("=", 1)[1])
    except FileNotFoundError:
        pass
    except UnicodeError as error:
        raise Refused("machine.env", "expected UTF-8 machine settings") from error
    cgroup = Path("/sys/fs/cgroup/cgroup.controllers")
    bus = Path(f"/run/user/{os.getuid()}/bus")
    manager = bus.exists() and stat.S_ISSOCK(bus.stat().st_mode)
    namespaces = Path("/proc/sys/user/max_user_namespaces")
    try:
        userns = int(namespaces.read_text().strip()) > 0
    except (OSError, ValueError):
        userns = False
    return {"private": True, "lock_available": True,
            "disk_ok": shutil.disk_usage(lab.root).free >= floor * 1024 ** 3,
            "containment": cgroup.is_file() and manager and userns
            and all(shutil.which(name, path=os.defpath) is not None for name in ("systemd-run", "systemctl")),
            "bubblewrap": shutil.which("bwrap", path=os.defpath) is not None}


def _experiment_input(directory):
    """Read the complete experiment before any immutable publication."""
    files = store.snapshot(directory)
    by_path = {f["path"]: f for f in files}
    if "experiment.json" not in by_path:
        raise Refused("experiment.json", "missing experiment record")
    exp = canonical.loads(by_path["experiment.json"]["data"])
    return exp, files


def validate_command(lab, directory, subject_id, *, inputs=None):
    exp, files = _experiment_input(directory) if inputs is None else inputs
    out = []
    try:
        settings = canonical.loads(lab.read("settings.json"))
    except (OSError, Refused) as error:
        out.append(Refusal("settings", str(error)))
        settings = {}
    try:
        subject, subject_path = _subject(lab, subject_id)
        subject_available = True
    except (OSError, Refused) as error:
        out.append(Refusal("subject", str(error)))
        subject, subject_path, subject_available = {}, lab.root / "store/subjects/missing", False
    exp_id = canonical.tree_digest(store.entries(files))
    exp_path = lab.root / "store/experiments" / exp_id
    known = {"experiment": str(exp_path), "source": str(subject_path / "source"),
             "runner": str(_runner_root())}
    known.update({"artifact." + name: str(subject_path / "artifacts" / name)
                  for name in subject.get("artifacts", {})})
    known.update(experiment.port_values(settings.get('ports', {}) if type(settings.get('ports', {})) is dict else {}))
    texts = {}
    executables = {}
    for file in files:
        try:
            texts[file["path"]] = file["data"].decode("utf-8", "strict")
        except UnicodeError:
            texts[file["path"]] = None
        if file["exec"]:
            executables[str(exp_path / file["path"])] = {"regular": True, "exec": True, "sha256": store.file_digest(file["data"])}
    for name, item in subject.get("artifacts", {}).items():
        executables[known["artifact." + name]] = {"regular": True, "exec": item["exec"], "sha256": item["sha256"]}
    system = {}
    used_programs = experiment.uses_programs(exp, texts)
    program_known, program_facts, problems = _program_facts(settings, used_programs)
    known.update(program_known)
    executables.update(program_facts)
    out.extend(problems)
    browser_facts = {}
    browser_used = experiment.uses_browser(exp, texts)
    if browser_used:
        browser_known, browser_facts, problems = _browser_facts(settings.get('browser'))
        known.update(browser_known)
        out.extend(problems)
        if 'program' in browser_facts:
            metadata = browser_facts['program']
            executables[known['browser.program']] = metadata
            system[metadata['path']] = {k: metadata[k] for k in ('path', 'sha256')}
    # Invalid or run-specific templates are left to the pure validator.
    for member, text, _, _, executable in experiment.template_uses(exp, texts):
        if not executable:
            continue
        try:
            path = experiment.render(text, known)
            if path in executables:
                continue
            if path.startswith(str(exp_path) + "/") or path.startswith(str(subject_path) + "/"):
                continue
            if path.startswith(known['runner'] + '/'):
                suffix = path[len(known['runner']) + 1:]
                if relative(suffix) and suffix.split('/')[0] != 'tests':
                    data, executable_bit = store.read_regular(path)
                    executables[path] = {'regular': True, 'exec': executable_bit, 'sha256': store.file_digest(data)}
                continue
            metadata = _program(path)
            executables[path] = metadata
            system[metadata["path"]] = {k: metadata[k] for k in ("path", "sha256")}
        except (OSError, Refused):
            continue
    helpers = [os.path.realpath(sys.executable)]
    for name in ("git", "systemd-run", "systemctl", "bwrap"):
        path = shutil.which(name, path=os.defpath)
        if path:
            helpers.append(path)
    for path in helpers:
        try:
            metadata = _program(path)
            system[metadata["path"]] = {k: metadata[k] for k in ("path", "sha256")}
        except (OSError, Refused) as error:
            out.append(Refusal("lab.programs", str(error)))
    for name in sorted(used_programs):
        path = program_known.get('program.' + name)
        if path in program_facts:
            metadata = program_facts[path]
            system[metadata['path']] = {k: metadata[k] for k in ('path', 'sha256')}
            system[metadata['path']]['version'] = settings['programs'][name]['version']
    driver = settings.get("driver") if exp.get("driver") is not None else None
    driver_available = False
    if type(driver) is dict and type(driver.get("program")) is dict:
        try:
            metadata = _program(driver["program"].get("path"))
            if metadata["sha256"] != driver["program"].get("sha256") or metadata["path"] != driver["program"].get("path"):
                out.append(Refusal("settings.driver.program", "host identity differs from configured program"))
            else:
                driver_available = True
        except (OSError, Refused) as error:
            out.append(Refusal("settings.driver.program", str(error)))
    facts = {**_machine_facts(lab), "subject_available": subject_available,
             "source_available": subject_available and subject.get("source") is not None,
             "artifacts_available": list(subject.get("artifacts", {})), "experiment_files": texts,
             "executables": executables, "known": known, "driver_host": driver_available,
             "browser": browser_facts, 'held': held.availability(lab, exp.get('held', []))}
    facts['artifact_bytes'] = {}
    for i, process in enumerate(exp.get('processes', []) if type(exp.get('processes')) is list else []):
        if type(process) is not dict or type(process.get('files', [])) is not list:
            continue
        for j, entry in enumerate(process.get('files', [])):
            name = entry.get('tree') if type(entry) is dict else None
            if type(name) is str and name in subject.get('artifacts', {}) and name not in facts['artifact_bytes']:
                try:
                    facts['artifact_bytes'][name] = lab.read(f'store/subjects/{subject_id}/artifacts/{name}')
                except (OSError, Refused) as error:
                    out.append(Refusal(f'processes.{i}.files.{j}.tree', str(error)))
    out.extend(experiment.validate(exp, subject, settings, facts))
    if out:
        return None, out
    if driver is not None:
        driver = {**driver, 'config': codex.configuration(os.path.realpath(sys.executable), str(_runner_root()),
                  driver['program']['path'], [t['name'] for t in exp['driver']['tools']],
                  driver['model'], driver['effort'], subject['source'] is not None,
                  json.loads((_runner_root() / 'schemas/report.json').read_text()))}
    lab_inputs = {"runner": _runner_digest(),
                  "python": sys.version.split()[0], "programs": sorted(system.values(), key=lambda p: p["path"]),
                  "driver": driver, "cost": {"name": exp["cost"]}}
    if browser_used:
        lab_inputs['browser'] = {'chromium': settings['browser']['chromium'], 'fonts': browser_facts['fonts']}
    value = experiment.key(exp_id, subject, lab_inputs)
    identity = canonical.digest(value)
    lab.capture("experiments", exp_id, files)
    lab.write_once(f"keys/{exp['name']}/{identity}/key.json", canonical.record_bytes(value))
    return identity, []


def runtime_command(lab, lock_fd, args):
    from .transport import launch
    seed_problems = []
    if args.command == 'run' and args.seed is not None:
        try:
            canonical.u64(args.seed)
        except Refused as error:
            seed_problems.append(error.refusal)
    if args.campaign is not None:
        seed_problems.append(Refusal('campaign', 'campaign integration is unsupported by this runtime'))
    inputs = _experiment_input(args.experiment)
    exp, _files = inputs
    identity, problems = validate_command(lab, args.experiment, args.subject, inputs=inputs)
    problems.extend(seed_problems)
    if problems:
        for problem in problems:
            print(str(problem), file=sys.stderr)
        return 2
    key_path = f"keys/{exp['name']}/{identity}"
    previous = evidence.read(lab, key_path)
    if args.command == 'run':
        seed = args.seed
        if seed is None:
            if len({e['seed'] for e in previous}) == 3:
                raise Refused('seed', 'three seeds already exist; supply one explicitly for an additive rerun')
            seed = _new_seed(previous)
        if not judge.seed_admitted(previous, seed):
            raise Refused('seed', 'a fourth seed is forbidden')
        admitted, refusal = brake.admit(lab, exp, _runtime_cost(lab, canonical.loads(lab.read(key_path + '/key.json'))), str(_runner_root()))
        if refusal is not None:
            print('brake: ' + refusal, file=sys.stderr)
            return 2
        record, status = launch(lab, lock_fd, key_path, seed, None, brake_event=admitted)
        print(record['run'])
        for error in record['errors']:
            print(error['what'] + ': ' + error['detail'], file=sys.stderr)
        return status
    while True:
        key = canonical.loads(lab.read(key_path + '/key.json'))
        superseded = _superseded(lab, key_path, previous, _runtime_cost(lab, key))
        decision = judge.next_seed(previous, superseded)
        derived = judge.verdict_record(identity, previous, superseded)
        lab.replace_derived(key_path + '/verdict.json', derived)
        if decision['action'] == 'done':
            print(derived['verdict'])
            return 0 if derived['verdict'] == 'pass' else 1
        if decision['action'] == 'blocked':
            print('seed ' + decision['seed'] + ' needs an explicit additive run', file=sys.stderr)
            print(derived['verdict'])
            return 3
        for name, status in held.availability(lab, exp.get('held', [])).items():
            if status != 'kept':
                print('held.' + name + ': ' + status, file=sys.stderr)
                print(derived['verdict'])
                return 2
        admitted, refusal = brake.admit(lab, exp, _runtime_cost(lab, key), str(_runner_root()))
        if refusal is not None:
            print(derived['verdict'])
            print('brake: ' + refusal, file=sys.stderr)
            return 2
        record, status = launch(lab, lock_fd, key_path, _new_seed(previous), None, brake_event=admitted)
        previous = evidence.read(lab, key_path)
        if record['outcome'] not in ('pass', 'fail') or status >= 128:
            print('not enough evidence')
            print('seed ' + record['seed'] + ' needs an explicit additive run', file=sys.stderr)
            return status if status in (3, 4) or status >= 128 else 4


def _new_seed(previous):
    existing = {e['seed'] for e in previous}
    seed = canonical.u64_text(secrets.randbits(64))
    while seed in existing:
        seed = canonical.u64_text(secrets.randbits(64))
    return seed


def _runner_digest():
    return canonical.tree_digest(store.walk(_runner_root(), exclude=("tests",)))


def _artifact(text):
    if "=" not in text:
        raise argparse.ArgumentTypeError("artifact must be NAME=PATH")
    name, path = text.split("=", 1)
    if not name or not path:
        raise argparse.ArgumentTypeError("artifact must be NAME=PATH")
    return name, path


def main(argv=None, *, lab_root=None, finding_time=None):
    parser = argparse.ArgumentParser(prog="lab", description="Capture, validate, run and judge lab experiments.")
    commands = parser.add_subparsers(dest="command", required=True)
    tool = commands.add_parser("tool", help="serve the run’s tools over stdio MCP")
    tool.add_argument("run")
    subject = commands.add_parser("subject", help="capture a commit tree and named artifacts")
    subject.add_argument("--repository", required=True)
    subject.add_argument("--commit", required=True)
    subject.add_argument("--exclude", action="append", default=[])
    subject.add_argument("--artifact", action="append", type=_artifact, required=True)
    validate = commands.add_parser("validate", help="validate all experiment fields and print the key's digest")
    validate.add_argument("experiment")
    validate.add_argument("--subject", required=True)
    held_command = commands.add_parser('held', help='list held files or move one into the lab')
    held_actions = held_command.add_subparsers(dest='held_action')
    put = held_actions.add_parser('put')
    put.add_argument('name')
    put.add_argument('file')
    finding = commands.add_parser('finding', help='act on retained finding evidence')
    finding.add_argument('id')
    actions = finding.add_subparsers(dest='action', required=True)
    for action in ('file', 'class', 'take', 'fix', 'close'):
        command = actions.add_parser(action)
        command.add_argument('--by', default='uid:' + str(os.getuid()), required=action == 'take')
        if action == 'class':
            command.add_argument('finding_class', choices=CLASSES)
        elif action == 'fix':
            command.add_argument('--repository')
            command.add_argument('--commit')
            command.add_argument('--test', action='append')
            command.add_argument('--expectation', action='append')
            command.add_argument('--product-commit')
            command.add_argument('--experiment', dest='corrected_experiment')
            command.add_argument('--runner-commit')
        elif action == 'close':
            command.add_argument('--recheck', required=True)
    for name in ('run', 'judge'):
        runtime = commands.add_parser(name, help='run an experiment' if name == 'run' else 'judge exactly three seeds')
        runtime.add_argument('experiment')
        runtime.add_argument('--subject', required=True)
        runtime.add_argument('--campaign')
        if name == 'run':
            runtime.add_argument('--seed')
    args = parser.parse_args(argv)
    if sys.version_info < (3, 11):
        parser.error("Python 3.11 or later is required")
    previous_umask = os.umask(0o077)
    root = lab_root if lab_root is not None else Path.home() / "src/rust/tmp/lab"
    try:
        if args.command == 'tool':
            from .tool import main as tool_main
            return tool_main(root, args.run)
        with store.open_lab(root) as lab, lab.lock() as lock_fd:
            evidence.require_recovered(lab)
            now = finding_time
            if args.command == 'finding' and now is None:
                now = datetime.now(timezone.utc).isoformat(timespec='milliseconds').replace('+00:00', 'Z')
            finding_store.sync(lab, now)
            if args.command == 'held':
                if args.held_action == 'put':
                    held.put(lab, args.name, args.file)
                    print(args.name + ' kept')
                else:
                    for line in held.listing(lab):
                        print(line)
                return 0
            if args.command == 'finding':
                fix = None
                if args.action == 'fix':
                    fields = {'repository': args.repository, 'commit': args.commit, 'tests': args.test,
                              'expectations': args.expectation,
                              'product_commit': args.product_commit, 'experiment': args.corrected_experiment,
                              'runner_commit': args.runner_commit}
                    fix = {k: v for k, v in fields.items() if v is not None}
                derived = finding_store.act(lab, args.id, args.action, now, args.by,
                                            finding_class=getattr(args, 'finding_class', None), fix=fix,
                                            recheck=getattr(args, 'recheck', None))
                state = next(f['state'] for f in derived['findings'] if f['id'] == args.id)
                print(args.id + ' ' + state)
                return 0
            if args.command in ('run', 'judge'):
                return runtime_command(lab, lock_fd, args)
            if args.command == "subject":
                identity = store.capture_subject(lab, args.repository, args.commit, args.exclude, args.artifact)
                problems = []
            else:
                identity, problems = validate_command(lab, args.experiment, args.subject)
        if problems:
            for problem in problems:
                print(str(problem), file=sys.stderr)
            return 2
        print(identity)
        return 0
    except (Refused, FileNotFoundError, PermissionError, NotADirectoryError) as error:
        print(str(error), file=sys.stderr)
        return 2
    except (RuntimeError, subprocess.SubprocessError) as error:
        print(f"lab: {error}", file=sys.stderr)
        return 4
    except OSError as error:
        print(f"lab: {error}", file=sys.stderr)
        return 2 if error.errno == errno.ELOOP else 4
    finally:
        os.umask(previous_umask)
