"""The subject -> validate -> key command path (lab.md, 10.2)."""
import argparse
import errno
import os
from pathlib import Path
import re
import shutil
import stat
import sys
from . import canonical, experiment, store
from .records import Refusal, Refused, digest_value, subject_refusals


def _program(path):
    if type(path) is not str or not path.startswith("/"):
        raise Refused("program", "expected an absolute executable path")
    resolved = os.path.realpath(path)
    data, executable = store.read_regular(resolved, allow_hardlinks=True)
    if not executable:
        raise Refused(path, "program has no owner execute bit")
    return {"path": resolved, "sha256": store.file_digest(data), "regular": True, "exec": True}


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
            "containment": cgroup.is_file() and manager and userns and shutil.which("systemd-run", path=os.defpath) is not None,
            "bubblewrap": shutil.which("bwrap", path=os.defpath) is not None}


def _experiment_input(directory):
    """Read the complete experiment before any immutable publication."""
    files = store.snapshot(directory)
    by_path = {f["path"]: f for f in files}
    if "experiment.json" not in by_path:
        raise Refused("experiment.json", "missing experiment record")
    exp = canonical.loads(by_path["experiment.json"]["data"])
    return exp, files


def validate_command(lab, directory, subject_id):
    exp, files = _experiment_input(directory)
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
    known = {"experiment": str(exp_path), "source": str(subject_path / "source")}
    known.update({"artifact." + name: str(subject_path / "artifacts" / name)
                  for name in subject.get("artifacts", {})})
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
            metadata = _program(path)
            executables[path] = metadata
            system[metadata["path"]] = {k: metadata[k] for k in ("path", "sha256")}
        except (OSError, Refused):
            continue
    helpers = [os.path.realpath(sys.executable)]
    for name in ("git", "systemd-run", "bwrap"):
        path = shutil.which(name, path=os.defpath)
        if path:
            helpers.append(path)
    for path in helpers:
        try:
            metadata = _program(path)
            system[metadata["path"]] = {k: metadata[k] for k in ("path", "sha256")}
        except (OSError, Refused) as error:
            out.append(Refusal("lab.programs", str(error)))
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
             "executables": executables, "known": known, "driver_host": driver_available}
    out.extend(experiment.validate(exp, subject, settings, facts))
    if out:
        return None, out
    lab_inputs = {"runner": _runner_digest(),
                  "python": sys.version.split()[0], "programs": sorted(system.values(), key=lambda p: p["path"]),
                  "driver": driver, "cost": {"name": exp["cost"], **settings["costs"][exp["cost"]]}}
    value = experiment.key(exp_id, subject, lab_inputs)
    identity = canonical.digest(value)
    lab.capture("experiments", exp_id, files)
    lab.write_once(f"keys/{exp['name']}/{identity}/key.json", canonical.record_bytes(value))
    return identity, []


def _runner_digest():
    runner_root = Path(__file__).resolve().parent.parent
    return canonical.tree_digest(store.walk(runner_root, exclude=("tests",)))


def _artifact(text):
    if "=" not in text:
        raise argparse.ArgumentTypeError("artifact must be NAME=PATH")
    name, path = text.split("=", 1)
    if not name or not path:
        raise argparse.ArgumentTypeError("artifact must be NAME=PATH")
    return name, path


def main(argv=None, *, lab_root=None):
    parser = argparse.ArgumentParser(prog="lab", description="Capture subjects and validate immutable lab inputs.")
    commands = parser.add_subparsers(dest="command", required=True)
    subject = commands.add_parser("subject", help="capture a commit tree and named artifacts")
    subject.add_argument("--repository", required=True)
    subject.add_argument("--commit", required=True)
    subject.add_argument("--exclude", action="append", default=[])
    subject.add_argument("--artifact", action="append", type=_artifact, required=True)
    validate = commands.add_parser("validate", help="validate all experiment fields and print the key's digest")
    validate.add_argument("experiment")
    validate.add_argument("--subject", required=True)
    args = parser.parse_args(argv)
    if sys.version_info < (3, 11):
        parser.error("Python 3.11 or later is required")
    previous_umask = os.umask(0o077)
    root = lab_root if lab_root is not None else Path.home() / "src/rust/tmp/lab"
    try:
        with store.open_lab(root) as lab, lab.lock():
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
    except OSError as error:
        print(f"lab: {error}", file=sys.stderr)
        return 2 if error.errno == errno.ELOOP else 4
    finally:
        os.umask(previous_umask)
