"""Shapes consumed by the first lab slice (lab.md, 2)."""
from dataclasses import dataclass
import re

NAME = re.compile(r"[a-z][a-z0-9-]*\Z")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
SAFE = (1 << 53) - 1
COST_FIELDS = ("ready_seconds", "command_seconds", "stop_seconds", "driver_seconds",
               "driver_actions", "driver_input_tokens")
FORMATS = {"lab-experiment/1", "lab-tree/1", "lab-subject/1", "lab-key/1",
           "lab-settings/1", "lab-run/1", "lab-evidence/1", "lab-verdict/1",
           "lab-event/1", "lab-live/1", "lab-finding/1", "lab-finding-index/1"}


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
    lab = shape(value, ("runner", "python", "programs", "driver", "cost"), ("browser",), "lab", out)
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
    if "browser" in lab:
        browser = shape(lab["browser"], ("chromium", "fonts"), (), "lab.browser", out)
        out.extend(chromium_refusals(browser.get("chromium"), "lab.browser.chromium"))
        if not digest_value(browser.get("fonts")):
            out.append(Refusal("lab.browser.fonts", "expected a tree digest"))
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


RUN_ID = re.compile(r'[0-9]{8}T[0-9]{9}Z-[0-9a-f]{6}\Z')
OUTCOMES = ('pass', 'fail', 'incomplete', 'error')


def evidence_refusals(value):
    from .canonical import u64
    out = []
    obj = shape(value, ('format', 'run', 'seed', 'attempt', 'outcome', 'signatures', 'ended'), (), 'evidence', out)
    if obj.get('format') != 'lab-evidence/1':
        out.append(Refusal('evidence.format', 'expected lab-evidence/1'))
    if type(obj.get('run')) is not str or not RUN_ID.fullmatch(obj['run']):
        out.append(Refusal('evidence.run', 'invalid run id'))
    try:
        u64(obj.get('seed'))
    except Refused as error:
        out.append(error.refusal)
    if not positive(obj.get('attempt')):
        out.append(Refusal('evidence.attempt', 'expected positive attempt'))
    if obj.get('outcome') not in OUTCOMES:
        out.append(Refusal('evidence.outcome', 'unknown outcome'))
    if type(obj.get('signatures')) is not list or any(not signature_value(s) for s in obj.get('signatures', [])):
        out.append(Refusal('evidence.signatures', 'expected signatures'))
    if type(obj.get('ended')) is not str or not re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z', obj['ended']):
        out.append(Refusal('evidence.ended', 'expected UTC milliseconds'))
    return out


def run_refusals(value):
    fields = ('run', 'key', 'experiment', 'seed', 'attempt', 'campaign', 'started', 'ended', 'outcome',
              'schedule', 'processes', 'commands', 'driver', 'failures', 'undecided', 'errors', 'bounds', 'usage')
    out = []
    obj = shape(value, ('format',) + fields, ('about',), 'run', out)
    if obj.get('format') != 'lab-run/1':
        out.append(Refusal('run.format', 'expected lab-run/1'))
    if not digest_value(obj.get('key')):
        out.append(Refusal('run.key', 'expected key digest'))
    for field in ('failures', 'undecided', 'errors', 'commands', 'bounds'):
        if type(obj.get(field)) is not list:
            out.append(Refusal('run.' + field, 'expected list'))
    if type(obj.get('processes')) is not dict:
        out.append(Refusal('run.processes', 'expected named processes'))
    for i, command in enumerate(obj.get('commands', []) if type(obj.get('commands')) is list else []):
        if type(command) is dict and 'awaited' in command:
            m = f'run.commands.{i}.awaited'
            if command.get('group') != 'actions':
                out.append(Refusal(m, 'only seed actions await a line'))
            if command['awaited'] is not None:
                awaited = shape(command['awaited'], ('ms', 'captures'), (), m, out)
                ms = awaited.get('ms')
                if type(ms) is not int or not 0 <= ms <= SAFE:
                    out.append(Refusal(m + '.ms', 'expected nonnegative safe milliseconds'))
                captures = awaited.get('captures')
                if (type(captures) is not dict or len(captures) != 1
                        or any(type(k) is not str or not NAME.fullmatch(k) or type(v) is not str
                               for k, v in captures.items())):
                    out.append(Refusal(m + '.captures', 'expected one named text capture'))
    for i, failure in enumerate(obj.get('failures', []) if type(obj.get('failures')) is list else []):
        m = f'run.failures.{i}'
        f = shape(failure, ('id', 'signature', 'kind', 'summary', 'evidence'), (), m, out)
        for field in ('id', 'kind', 'summary'):
            if type(f.get(field)) is not str:
                out.append(Refusal(m + '.' + field, 'expected text'))
        if not signature_value(f.get('signature')):
            out.append(Refusal(m + '.signature', 'expected printable ASCII signature segments'))
        if type(f.get('evidence')) is not list or any(not relative(p) for p in f.get('evidence', [])):
            out.append(Refusal(m + '.evidence', 'expected relative evidence paths'))
    failures = obj.get('failures')
    if type(failures) is list and not out:
        ids = [f['id'] for f in failures]
        if len(set(ids)) != len(ids) or any(not text_value(i) for i in ids):
            out.append(Refusal('run.failures.id', 'failure ids must be nonempty and unique'))
    for field, members in (('errors', ('what', 'detail')), ('undecided', ('check', 'reason'))):
        if type(obj.get(field)) is list:
            for i, item in enumerate(obj[field]):
                m = f'run.{field}.{i}'
                value = shape(item, members, (), m, out)
                if any(type(value.get(k)) is not str for k in members):
                    out.append(Refusal(m, 'expected text members'))
    if not out:
        evidence = {k: obj[k] for k in ('run', 'seed', 'attempt', 'outcome', 'ended')}
        evidence.update(format='lab-evidence/1', signatures=[f['signature'] for f in obj['failures']])
        out.extend(evidence_refusals(evidence))
    return out


def signature_value(text):
    return type(text) is str and bool(text) and all(33 <= ord(c) <= 126 for c in text)


FINDING_ID = re.compile(r'F-(?:[0-9]{4}|[1-9][0-9]{4,})\Z')
CLASSES = ('world', 'fake', 'below', 'tools', 'experiment', 'lab', 'look')
FINDING_FIELDS = {
    'candidate': ('signature', 'kind', 'summary', 'run', 'key', 'seed', 'failure'),
    'occurred': ('run', 'key', 'seed', 'failure'),
    'proposed': ('class', 'title', 'reason'),
    'filed': ('verdict', 'flaky', 'reruns'),
    'classed': ('class',), 'taken': (), 'fixed': ('fix',),
    'rechecked': ('key', 'verdict'), 'closed': (),
    'reopened': ('run', 'key', 'seed', 'failure'),
}


def utc_value(value):
    """Validate a supplied UTC calendar value without consulting a clock."""
    if type(value) is not str:
        return False
    m = re.fullmatch(r'([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})\.([0-9]{3})Z', value)
    if m is None:
        return False
    year, month, day, hour, minute, second, _ms = map(int, m.groups())
    leap = year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
    days = (31, 29 if leap else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
    return year > 0 and 1 <= month <= 12 and 1 <= day <= days[month - 1] and hour < 24 and minute < 60 and second < 60


def finding_refusals(value):
    from .canonical import u64
    out = []
    event = value.get('event') if type(value) is dict else None
    fields = FINDING_FIELDS.get(event, ()) if type(event) is str else ()
    obj = shape(value, ('format', 'event', 't', 'by') + fields, ('judgment',), 'finding', out)
    if obj.get('format') != 'lab-finding/1':
        out.append(Refusal('finding.format', 'expected lab-finding/1'))
    if type(event) is not str or event not in FINDING_FIELDS:
        out.append(Refusal('finding.event', 'unknown event'))
    if not utc_value(obj.get('t')):
        out.append(Refusal('finding.t', 'expected UTC calendar time with milliseconds'))
    if not text_value(obj.get('by')):
        out.append(Refusal('finding.by', 'expected actor'))
    if 'judgment' in obj and (event not in ('rechecked', 'reopened') or not text_value(obj['judgment'])):
        out.append(Refusal('finding.judgment', 'owner judgment belongs to recheck or reopening'))
    for field in ('kind', 'summary', 'failure', 'title', 'reason'):
        valid = type(obj.get(field)) is str if field == 'summary' else text_value(obj.get(field))
        if field in fields and not valid:
            out.append(Refusal('finding.' + field, 'expected nonempty text'))
    if 'signature' in fields and not signature_value(obj.get('signature')):
        out.append(Refusal('finding.signature', 'expected printable ASCII signature'))
    if 'class' in fields and obj.get('class') not in CLASSES:
        out.append(Refusal('finding.class', 'unknown class'))
    if 'key' in fields and not digest_value(obj.get('key')):
        out.append(Refusal('finding.key', 'expected key digest'))
    if 'run' in fields and (type(obj.get('run')) is not str or not RUN_ID.fullmatch(obj['run'])):
        out.append(Refusal('finding.run', 'invalid run id'))
    if 'seed' in fields:
        try:
            u64(obj.get('seed'))
        except Refused as error:
            out.append(error.refusal)
    if event == 'filed':
        if type(obj.get('flaky')) is not bool or type(obj.get('reruns')) is not list:
            out.append(Refusal('finding.filed', 'expected flaky boolean and rerun results'))
    if event in ('filed', 'rechecked') and type(obj.get('verdict')) is not dict:
        out.append(Refusal('finding.verdict', 'expected verdict snapshot'))
    if event == 'fixed' and type(obj.get('fix')) is not dict:
        out.append(Refusal('finding.fix', 'expected fix evidence'))
    return out


def text_value(value):
    return type(value) is str and bool(value.strip()) and '\x00' not in value


def commit_value(value):
    return type(value) is str and re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', value) is not None


def fix_refusals(value, finding_class):
    out = []
    if finding_class not in CLASSES:
        return [Refusal('fix.class', 'confirmed class required')]
    fields = (('experiment',) if finding_class == 'experiment' else
              ('runner_commit', 'tests') if finding_class == 'lab' else
              ('repository', 'commit', 'expectations', 'product_commit') if finding_class == 'look' else
              ('repository', 'commit', 'tests', 'product_commit'))
    obj = shape(value, fields, (), 'fix', out)
    for field in fields:
        item = obj.get(field)
        if field == 'experiment':
            valid = digest_value(item)
        elif field in ('commit', 'product_commit', 'runner_commit'):
            valid = commit_value(item)
        elif field in ('tests', 'expectations'):
            valid = type(item) is list and bool(item) and all(text_value(t) for t in item) and len(set(item)) == len(item)
        else:
            valid = text_value(item) and item.startswith('/')
        if not valid:
            out.append(Refusal('fix.' + field, 'missing or invalid fix evidence'))
    return out


def chromium_refusals(value, member):
    out = []
    obj = shape(value, ('path', 'sha256', 'version'), (), member, out)
    path = obj.get('path')
    if type(path) is not str or not path.startswith('/') or '\x00' in path:
        out.append(Refusal(member + '.path', 'expected an absolute path'))
    if not digest_value(obj.get('sha256')):
        out.append(Refusal(member + '.sha256', 'expected a digest'))
    if not text_value(obj.get('version')):
        out.append(Refusal(member + '.version', 'expected nonempty text'))
    return out
