"""Shapes consumed by the first lab slice (lab.md, 2)."""
from dataclasses import dataclass
import re

NAME = re.compile(r"[a-z][a-z0-9-]*\Z")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
SAFE = (1 << 53) - 1
COST_FIELDS = ("ready_seconds", "command_seconds", "stop_seconds", "driver_seconds",
               "driver_actions", "driver_input_tokens")
FORMATS = {"lab-experiment/1", "lab-tree/1", "lab-subject/1", "lab-key/1",
           "lab-settings/1"}


@dataclass(frozen=True)
class Refusal:
    member: str
    why: str

    def __str__(self):
        return f"{self.member}: {self.why}"


class Refused(ValueError):
    def __init__(self, member, why):
        self.refusal = Refusal(member, why)
        super().__init__(str(self.refusal))


def shape(value, required, optional, member, refusals):
    if type(value) is not dict:
        refusals.append(Refusal(member, "expected an object"))
        return {}
    bad_keys = [key for key in value if type(key) is not str or not key.isascii()]
    if bad_keys:
        refusals.append(Refusal(member, "object keys must be ASCII strings"))
        value = {key: item for key, item in value.items() if type(key) is str and key.isascii()}
    for field in sorted(set(required) - value.keys()):
        refusals.append(Refusal(f"{member}.{field}", "missing member"))
    for field in sorted(value.keys() - set(required) - set(optional)):
        refusals.append(Refusal(f"{member}.{field}", "unknown member"))
    return value


def positive(value):
    return type(value) is int and 0 < value <= SAFE


def digest_value(value):
    return type(value) is str and DIGEST.fullmatch(value) is not None


def relative(path):
    return (type(path) is str and bool(path) and not path.startswith("/")
            and "\x00" not in path
            and all(p not in ("", ".", "..") for p in path.split("/")))


def subject_refusals(value):
    out = []
    obj = shape(value, ("format", "commit", "artifacts", "source"), ("about",), "subject", out)
    if obj.get("format") != "lab-subject/1":
        out.append(Refusal("subject.format", "expected lab-subject/1"))
    if type(obj.get("commit")) is not str or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", obj["commit"]):
        out.append(Refusal("subject.commit", "expected a full commit object id"))
    artifacts = obj.get("artifacts")
    if type(artifacts) is not dict:
        out.append(Refusal("subject.artifacts", "expected named artifacts"))
    else:
        for name, item in artifacts.items():
            m = f"subject.artifacts.{name}"
            if type(name) is not str or not NAME.fullmatch(name):
                out.append(Refusal(m, "invalid artifact name"))
            a = shape(item, ("sha256", "bytes", "exec"), (), m, out)
            if not digest_value(a.get("sha256")):
                out.append(Refusal(m + ".sha256", "expected a digest"))
            if type(a.get("bytes")) is not int or not 0 <= a["bytes"] <= SAFE:
                out.append(Refusal(m + ".bytes", "expected a safe nonnegative integer"))
            if type(a.get("exec")) is not bool:
                out.append(Refusal(m + ".exec", "expected a boolean"))
    if obj.get("source") is not None and not digest_value(obj.get("source")):
        out.append(Refusal("subject.source", "expected a tree digest or null"))
    return out


def lab_refusals(value):
    out = []
    lab = shape(value, ("runner", "python", "programs", "driver", "cost"), (), "lab", out)
    if not digest_value(lab.get("runner")):
        out.append(Refusal("lab.runner", "expected a tree digest"))
    if type(lab.get("python")) is not str or not lab["python"]:
        out.append(Refusal("lab.python", "expected interpreter version"))
    programs = lab.get("programs")
    paths = []
    if type(programs) is not list:
        out.append(Refusal("lab.programs", "expected a list"))
    else:
        for i, item in enumerate(programs):
            m = f"lab.programs.{i}"
            p = shape(item, ("path", "sha256"), (), m, out)
            path = p.get("path")
            if type(path) is not str or not path.startswith("/") or "\x00" in path:
                out.append(Refusal(m + ".path", "expected an absolute path"))
            else:
                paths.append(path)
            if not digest_value(p.get("sha256")):
                out.append(Refusal(m + ".sha256", "expected a digest"))
        if paths != sorted(set(paths)):
            out.append(Refusal("lab.programs", "paths must be unique and sorted"))
    driver = lab.get("driver")
    if driver is not None:
        d = shape(driver, ("program", "model", "effort", "config"), (), "lab.driver", out)
        p = shape(d.get("program"), ("path", "sha256", "version"), (), "lab.driver.program", out)
        if type(p.get("path")) is not str or not p["path"].startswith("/"):
            out.append(Refusal("lab.driver.program.path", "expected an absolute path"))
        if not digest_value(p.get("sha256")):
            out.append(Refusal("lab.driver.program.sha256", "expected a digest"))
        for field in ("model", "effort"):
            if type(d.get(field)) is not str or not d[field]:
                out.append(Refusal("lab.driver." + field, "expected nonempty text"))
        if type(p.get("version")) is not str or not p["version"]:
            out.append(Refusal("lab.driver.program.version", "expected version"))
        if type(d.get("config")) is not dict:
            out.append(Refusal("lab.driver.config", "expected configuration object"))
    cost = shape(lab.get("cost"), ("name",) + COST_FIELDS, (), "lab.cost", out)
    if type(cost.get("name")) is not str or not NAME.fullmatch(cost["name"]):
        out.append(Refusal("lab.cost.name", "expected a cost class name"))
    for field in COST_FIELDS:
        if not positive(cost.get(field)):
            out.append(Refusal("lab.cost." + field, "expected a positive safe integer"))
    return out
