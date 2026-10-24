"""Experiment decisions over records and supplied facts; no filesystem access."""
import re
from . import canonical, tree
from .records import (NAME, Refusal, Refused, chromium_refusals, cost_refusals, digest_value, lab_refusals,
                      positive, relative, shape, signature_value, subject_refusals)


def _parts(text):
    if type(text) is not str or "\x00" in text:
        raise Refused("template", "expected text without NUL")
    parts = []
    literal = []
    i = 0
    while i < len(text):
        char = text[i]
        if char in "{}" and text[i:i + 2] == char * 2:
            literal.append(char)
            i += 2
        elif char == "{":
            end = text.find("}", i + 1)
            if end < 0 or not re.fullmatch(r"[a-z][a-z0-9.-]*", text[i + 1:end]):
                raise Refused("template", "malformed placeholder")
            if literal:
                parts.append((False, "".join(literal)))
                literal = []
            parts.append((True, text[i + 1:end]))
            i = end + 1
        elif char == "}":
            raise Refused("template", "unescaped closing brace")
        else:
            literal.append(char)
            i += 1
    if literal:
        parts.append((False, "".join(literal)))
    return parts


def render(text, known):
    result = []
    for placeholder, value in _parts(text):
        if placeholder:
            if value not in known or type(known[value]) is not str:
                raise Refused(value, "template value is not known")
            result.append(known[value])
        else:
            result.append(value)
    return "".join(result)


def _list(value, member, out):
    if type(value) is not list:
        out.append(Refusal(member, "expected a list"))
        return []
    return value


def _named(items, member, out):
    result = {}
    for i, item in enumerate(items):
        m = f"{member}.{i}"
        if type(item) is not dict:
            out.append(Refusal(m, "expected an object"))
            continue
        name = item.get("name")
        if type(name) is not str or not NAME.fullmatch(name):
            out.append(Refusal(m + ".name", "invalid name"))
        elif name in result:
            out.append(Refusal(m + ".name", "duplicate name"))
        else:
            result[name] = item
    return result


def template_uses(experiment, files):
    """Yield (member, text, phase, process, executable) for every template use.

    File contents remain templates until their process's predecessors are ready.
    Malformed structures are diagnosed separately by validate.
    """
    if type(experiment) is not dict:
        return
    groups = [(field, experiment.get(field), field) for field in ("processes", "actions", "collect", "checks", "release")]
    if experiment.get("meter") is not None:
        groups.append(("meter", [experiment["meter"]], "meter"))
    driver = experiment.get("driver")
    if type(driver) is dict:
        groups.append(("driver.tools", driver.get("tools"), "tools"))
    for group, items, phase in groups:
        if type(items) is not list:
            continue
        for i, item in enumerate(items):
            if type(item) is not dict:
                continue
            m = f"{group}.{i}"
            process = item.get("name") if phase == "processes" and type(item.get("name")) is str else None
            argv = item.get("argv")
            if type(argv) is list:
                for j, text in enumerate(argv):
                    yield f"{m}.argv.{j}", text, phase, process, j == 0
            env = item.get("env")
            if type(env) is dict:
                for name, text in env.items():
                    yield f"{m}.env.{name}", text, phase, process, False
            if "stdin" in item:
                yield m + ".stdin", item["stdin"], phase, process, False
            if phase == "tools":
                for field in ("reads", "writes"):
                    paths = item.get(field)
                    if type(paths) is list:
                        for j, text in enumerate(paths):
                            yield f"{m}.{field}.{j}", text, phase, process, False
            entries = item.get("files")
            if phase == "processes" and type(entries) is list:
                for j, entry in enumerate(entries):
                    if type(entry) is dict and type(entry.get("from")) is str and entry["from"] in files:
                        yield f"{m}.files.{j}.from", files[entry["from"]], phase, process, False


def uses_browser(experiment, files):
    for _member, text, _phase, _process, _executable in template_uses(experiment, files):
        try:
            if any(placeholder and name.startswith('browser.') for placeholder, name in _parts(text)):
                return True
        except Refused:
            continue
    return False


def uses_programs(experiment, files):
    names = set()
    for _member, text, _phase, _process, _executable in template_uses(experiment, files):
        try:
            for placeholder, name in _parts(text):
                bits = name.split('.')
                if placeholder and len(bits) == 2 and bits[0] == 'program' and NAME.fullmatch(bits[1]):
                    names.add(bits[1])
        except Refused:
            continue
    return names


def programs_settings_refusals(value):
    if type(value) is not dict:
        return [Refusal('settings.programs', 'expected an object')]
    out = []
    for name, program in value.items():
        member = 'settings.programs.' + str(name)
        if type(name) is not str or not NAME.fullmatch(name):
            out.append(Refusal(member, 'invalid name'))
        out.extend(chromium_refusals(program, member))
        path = program.get('path') if type(program) is dict else None
        if type(path) is str and path.startswith('/') and not relative(path[1:]):
            out.append(Refusal(member + '.path', 'expected a resolved absolute path'))
    return out


def port_values(ports):
    return {'port.' + name: str(port) for name, port in ports.items()
            if type(name) is str and NAME.fullmatch(name) and type(port) is int and 1 <= port <= 65535}


def ports_settings_refusals(value):
    if type(value) is not dict:
        return [Refusal('settings.ports', 'expected an object')]
    out = []
    for name, port in value.items():
        member = 'settings.ports.' + str(name)
        if type(name) is not str or not NAME.fullmatch(name):
            out.append(Refusal(member, 'invalid name'))
        if type(port) is not int or not 1 <= port <= 65535:
            out.append(Refusal(member, 'expected integer 1..65535'))
    return out


def brake_settings_refusals(value, runner):
    out = []
    obj = shape(value, ('argv', 'env'), (), 'settings.brake', out)
    argv = obj.get('argv')
    if type(argv) is not list or not argv:
        out.append(Refusal('settings.brake.argv', 'expected non-empty argv'))
        argv = []
    env = obj.get('env')
    if type(env) is not dict:
        out.append(Refusal('settings.brake.env', 'expected complete environment object'))
        env = {}
    for name in env:
        if type(name) is not str or not name or '=' in name or '\0' in name:
            out.append(Refusal('settings.brake.env', 'invalid environment name'))
    for member, text in [(f'settings.brake.argv.{i}', v) for i, v in enumerate(argv)] + [
            ('settings.brake.env.' + str(k), v) for k, v in env.items()]:
        try:
            if member.startswith('settings.brake.argv.') and (type(text) is not str or not text):
                raise Refused(member, 'expected non-empty string')
            rendered = render(text, {'runner': runner})
            if member == 'settings.brake.argv.0' and not rendered.startswith('/'):
                raise Refused(member, 'expected an absolute executable path')
        except Refused as error:
            out.append(Refusal(member, error.refusal.why))
    return out


def browser_settings_refusals(value):
    out = []
    obj = shape(value, ('program', 'chromium', 'fonts'), (), 'settings.browser', out)
    program = obj.get('program')
    if type(program) is not str or not program.startswith('/') or '\x00' in program:
        out.append(Refusal('settings.browser.program', 'expected an absolute path'))
    out.extend(chromium_refusals(obj.get('chromium'), 'settings.browser.chromium'))
    fonts = shape(obj.get('fonts'), ('path', 'tree'), (), 'settings.browser.fonts', out)
    path = fonts.get('path')
    if type(path) is not str or not path.startswith('/') or '\x00' in path:
        out.append(Refusal('settings.browser.fonts.path', 'expected an absolute path'))
    if not digest_value(fonts.get('tree')):
        out.append(Refusal('settings.browser.fonts.tree', 'expected a tree digest'))
    return out


def validate(experiment, subject, settings, facts):
    out = []
    exp = shape(experiment, ("format", "name", "cost", "dirs", "processes", "actions", "driver",
                             "faults", "planted", "collect", "checks"), ("about", "held", "release", "meter"), "experiment", out)
    if exp.get("format") != "lab-experiment/1":
        out.append(Refusal("experiment.format", "expected lab-experiment/1"))
    if type(exp.get("name")) is not str or not NAME.fullmatch(exp["name"]):
        out.append(Refusal("experiment.name", "invalid name"))
    out.extend(subject_refusals(subject))
    subject = subject if type(subject) is dict else {}
    artifacts = subject.get("artifacts")
    artifacts = artifacts if type(artifacts) is dict else {}
    settings = shape(settings, ("format", "costs", "driver", "triager"), ("about", "browser", "programs", "codex_home", "brake", "ports"), "settings", out)
    if settings.get("format") != "lab-settings/1":
        out.append(Refusal("settings.format", "expected lab-settings/1"))
    if 'codex_home' in settings and (type(settings['codex_home']) is not str
            or not settings['codex_home'].startswith('/') or '\0' in settings['codex_home']):
        out.append(Refusal('settings.codex_home', 'expected an absolute directory path'))
    if settings.get('triager') is not None:
        out.append(Refusal('settings.triager', 'unsupported by this runtime'))
    costs = settings.get("costs")
    cost_name = exp.get("cost")
    if type(cost_name) is not str or not NAME.fullmatch(cost_name):
        out.append(Refusal("experiment.cost", "expected a cost class name"))
    cost = costs.get(cost_name) if type(costs) is dict and type(cost_name) is str else None
    if cost is None:
        out.append(Refusal("experiment.cost", "cost class missing from settings"))
    out.extend(cost_refusals(cost, 'settings.costs.selected'))
    facts = facts if type(facts) is dict else {}
    for field in ("private", "lock_available", "disk_ok", "containment", "bubblewrap", "subject_available"):
        if facts.get(field) is not True:
            out.append(Refusal("facts." + field, "required machine/input fact missing or false"))
    if facts.get("pruned") is True:
        out.append(Refusal("subject", "subject is pruned"))
    files = facts.get("experiment_files")
    files = files if type(files) is dict else {}
    programs = facts.get("executables")
    programs = programs if type(programs) is dict else {}
    resolved_roots = facts.get('resolved_roots')
    resolved_roots = resolved_roots if type(resolved_roots) is dict else {}
    available_artifacts = facts.get("artifacts_available")
    available_artifacts = available_artifacts if type(available_artifacts) in (list, tuple, set) else ()
    known = facts.get("known")
    known = known if type(known) is dict else {}
    if 'brake' in settings:
        out.extend(brake_settings_refusals(settings['brake'], known.get('runner', '/runner')))
    ports = settings.get('ports', {})
    out.extend(ports_settings_refusals(ports))
    ports = ports if type(ports) is dict else {}
    known = {**known, **port_values(ports)}
    named = settings.get('programs', {})
    out.extend(programs_settings_refusals(named))
    named = named if type(named) is dict else {}
    for name in sorted(uses_programs(exp, files)):
        configured = named.get(name)
        if type(configured) is not dict:
            continue
        actual = programs.get(configured.get('path')) if type(configured.get('path')) is str else None
        member = 'settings.programs.' + name
        if (type(actual) is not dict or actual.get('regular') is not True or actual.get('exec') is not True):
            out.append(Refusal(member + '.path', 'expected an executable regular file with known identity'))
        else:
            for field in ('path', 'sha256'):
                if actual.get(field) != configured.get(field):
                    out.append(Refusal(member + '.' + field, 'host identity differs from configured program'))
    browser_available = False
    if uses_browser(exp, files):
        browser = settings.get('browser')
        problems = browser_settings_refusals(browser)
        out.extend(problems)
        if not problems:
            browser_available = True
            actual = facts.get('browser')
            actual = actual if type(actual) is dict else {}
            chromium = actual.get('chromium')
            if (type(chromium) is not dict or any(chromium.get(k) != browser['chromium'][k]
                                                 for k in ('path', 'sha256'))):
                out.append(Refusal('settings.browser.chromium', 'host identity differs from configured Chromium'))
            if actual.get('fonts') != browser['fonts']['tree']:
                out.append(Refusal('settings.browser.fonts', 'host tree differs from configured fonts'))
            program = programs.get(browser['program'])
            if (type(program) is not dict or program.get('regular') is not True
                    or program.get('exec') is not True or not digest_value(program.get('sha256'))):
                out.append(Refusal('settings.browser.program', 'expected an executable regular file with known identity'))

    directories = _list(exp.get("dirs"), "dirs", out)
    held = set()
    held_facts = facts.get('held', {})
    held_facts = held_facts if type(held_facts) is dict else {}
    for i, name in enumerate(_list(exp.get('held', []), 'held', out)):
        if type(name) is not str or not NAME.fullmatch(name):
            out.append(Refusal(f'held.{i}', 'invalid name'))
        elif name in held:
            out.append(Refusal(f'held.{i}', 'duplicate name'))
        else:
            held.add(name)
            status = held_facts.get(name, 'not held')
            if status != 'kept':
                out.append(Refusal('held.' + name, status))
    dirs = set()
    for i, name in enumerate(directories):
        if type(name) is not str or not NAME.fullmatch(name):
            out.append(Refusal(f"dirs.{i}", "invalid name"))
        elif name in dirs:
            out.append(Refusal(f"dirs.{i}", "duplicate name"))
        else:
            dirs.add(name)
    groups = {}
    names = {}
    for field in ("processes", "actions", "collect", "checks", "release"):
        groups[field] = _list(exp.get(field, [] if field == 'release' else None), field, out)
        names[field] = _named(groups[field], field, out)
    if exp.get('meter') is not None:
        groups['meter'] = [exp['meter']]
        _named(groups['meter'], 'meter', out)
        for field in ('subject_spend', 'meter_seconds'):
            if type(cost) is not dict or not positive(cost.get(field)):
                out.append(Refusal('meter', 'cost class requires ' + field))
    processes = names["processes"]
    after = {}
    captures = {}
    private_sources = set()
    recipients = set()
    for i, item in enumerate(groups["processes"]):
        m = f"processes.{i}"
        p = shape(item, ("name", "argv", "env", "ready", "memory_bytes"), ("after", "files", "bootstrap"), m, out)
        name = p.get("name")
        if 'bootstrap' in p:
            bm = m + '.bootstrap'
            channel = shape(p['bootstrap'], ('stream', 'action'), (), bm, out)
            if channel.get('stream') != 'stdout':
                out.append(Refusal(bm + '.stream', 'expected stdout'))
            if type(name) is str:
                private_sources.add(name)
            action = channel.get('action')
            if type(action) is not str or action not in names['actions']:
                out.append(Refusal(bm + '.action', 'unknown seed action'))
            else:
                if action in recipients:
                    out.append(Refusal(bm + '.action', 'bootstrap recipient must be unique'))
                recipients.add(action)
                recipient = names['actions'][action]
                for field in ('stdin', 'await'):
                    if field in recipient:
                        out.append(Refusal(bm + '.action', 'bootstrap recipient must not declare ' + field))
        deps = _list(p.get("after", []), m + ".after", out)
        safe_deps = set()
        for j, dep in enumerate(deps):
            if type(dep) is not str or dep not in processes:
                out.append(Refusal(f"{m}.after.{j}", "unknown process"))
            elif dep in safe_deps:
                out.append(Refusal(f"{m}.after.{j}", "duplicate dependency"))
            else:
                safe_deps.add(dep)
        if type(name) is str and name in processes:
            after[name] = safe_deps
        memory = p.get("memory_bytes")
        if memory is not None and not positive(memory):
            out.append(Refusal(m + ".memory_bytes", "expected positive safe integer or null"))
        ready = _list(p.get("ready"), m + ".ready", out)
        if not ready:
            out.append(Refusal(m + ".ready", "at least one ready line required"))
        cs = set()
        for j, entry in enumerate(ready):
            rm = f"{m}.ready.{j}"
            r = shape(entry, ("stream", "prefix", "capture"), (), rm, out)
            if r.get("stream") not in ("stdout", "stderr"):
                out.append(Refusal(rm + ".stream", "expected stdout or stderr"))
            if 'bootstrap' in p and r.get('stream') == 'stdout':
                out.append(Refusal(rm + '.stream', 'ready rule must not read private bootstrap stdout'))
            prefix = r.get("prefix")
            if type(prefix) is not str or any(c in prefix for c in "\r\n\0"):
                out.append(Refusal(rm + ".prefix", "expected literal line prefix"))
            capture = r.get("capture")
            if capture is not None:
                if type(capture) is not str or not NAME.fullmatch(capture):
                    out.append(Refusal(rm + ".capture", "invalid name"))
                elif capture in cs:
                    out.append(Refusal(rm + ".capture", "duplicate capture"))
                else:
                    cs.add(capture)
        if type(name) is str:
            captures[name] = cs
        destinations = set()
        for j, entry in enumerate(_list(p.get("files", []), m + ".files", out)):
            fm = f"{m}.files.{j}"
            f = shape(entry, ("path",), ("from", "tree"), fm, out)
            if ('from' in f) == ('tree' in f):
                out.append(Refusal(fm + '.from', 'expected exactly one of from or tree'))
                out.append(Refusal(fm + '.tree', 'expected exactly one of from or tree'))
            path = f.get("path")
            if not relative(path):
                out.append(Refusal(fm + ".path", "expected contained relative path"))
            elif path in destinations or any(path.startswith(d + "/") or d.startswith(path + "/") for d in destinations):
                out.append(Refusal(fm + ".path", "overlapping destination"))
            else:
                destinations.add(path)
            if 'from' in f:
                source = f['from']
                if not relative(source) or source not in files:
                    out.append(Refusal(fm + ".from", "missing or uncontained experiment file"))
                elif type(files[source]) is not str:
                    out.append(Refusal(fm + ".from", "template must be UTF-8 text"))
            if 'tree' in f:
                artifact = f['tree']
                contents = facts.get('artifact_bytes', {})
                contents = contents if type(contents) is dict else {}
                if type(artifact) is not str or artifact not in artifacts or artifact not in available_artifacts:
                    out.append(Refusal(fm + '.tree', 'unknown or unavailable subject artifact'))
                else:
                    try:
                        tree.members(contents.get(artifact))
                    except Refused as error:
                        out.append(Refusal(fm + '.tree', error.refusal.why))

    ancestors = {p: set(deps) for p, deps in after.items()}
    for _ in range(len(processes)):
        for p in ancestors:
            ancestors[p].update(d for a in tuple(ancestors[p]) for d in ancestors.get(a, ()))
    for p, deps in ancestors.items():
        if p in deps:
            out.append(Refusal("processes." + p + ".after", "dependency cycle"))

    driver = exp.get("driver")
    if driver is not None:
        d = shape(driver, ("charter", "tools"), (), "driver", out)
        charter = d.get("charter")
        if not relative(charter) or charter not in files or type(files.get(charter)) is not str:
            out.append(Refusal("driver.charter", "missing UTF-8 charter in experiment"))
        tools = _list(d.get("tools"), "driver.tools", out)
        _named(tools, "driver.tools", out)
        groups["tools"] = tools
        if facts.get("driver_host") is not True:
            out.append(Refusal("facts.driver_host", "driver host unavailable"))
        configured = shape(settings.get('driver'), ('program', 'model', 'effort'), ('catalog',), 'settings.driver', out)
        # The runner derives configuration; settings select only the host and model.
        placeholder_lab = {"runner": "sha256:" + "0" * 64, "python": "3.11", "programs": [],
                           "driver": {**configured, 'config': {}}, "cost": {"name": "selected"}}
        if settings.get("driver") is None:
            out.append(Refusal("settings.driver", "driver settings required"))
        out.extend(Refusal("settings" + r.member[3:], r.why) for r in lab_refusals(placeholder_lab)
                   if r.member.startswith("lab.driver"))
    action_captures = {}
    for group, items in groups.items():
        for i, item in enumerate(items):
            m = f"driver.tools.{i}" if group == "tools" else f"{group}.{i}"
            if group != "processes":
                extra = ("scan",) if group == "collect" else (("reads", "writes", "provision") if group == "tools" else ())
                optional = (() if group in ("tools", "meter") else (('stdin',) if group == 'release'
                            else (("stdin", "await") if group == 'actions' else ("stdin", "cluster"))))
                p = shape(item, ("name", "argv", "env") + extra, optional, m, out)
                if group == 'actions' and 'await' in p:
                    am = m + '.await'
                    rule = shape(p['await'], ('process', 'stream', 'prefix', 'capture'), (), am, out)
                    if type(rule.get('process')) is not str or rule['process'] not in processes:
                        out.append(Refusal(am + '.process', 'unknown process'))
                    if rule.get('stream') not in ('stdout', 'stderr'):
                        out.append(Refusal(am + '.stream', 'expected stdout or stderr'))
                    if (type(rule.get('process')) is str and rule['process'] in private_sources
                            and rule.get('stream') == 'stdout'):
                        out.append(Refusal(am + '.stream', 'await must not read private bootstrap stdout'))
                    prefix = rule.get('prefix')
                    if type(prefix) is not str or any(c in prefix for c in '\r\n\0'):
                        out.append(Refusal(am + '.prefix', 'expected literal line prefix'))
                    capture = rule.get('capture')
                    if type(capture) is not str or not NAME.fullmatch(capture):
                        out.append(Refusal(am + '.capture', 'invalid name'))
                    elif type(p.get('name')) is str:
                        action_captures[p['name']] = (i, capture)
                if group in ("collect", "checks") and p.get("cluster", "up") not in ("up", "stopped"):
                    out.append(Refusal(m + ".cluster", "expected up or stopped"))
                if group == "collect":
                    for j, path in enumerate(_list(p.get("scan"), m + ".scan", out)):
                        if not relative(path):
                            out.append(Refusal(f"{m}.scan.{j}", "expected contained relative path"))
                if group == "tools":
                    pm = m + '.provision'
                    provision = shape(p.get('provision'), ('args', 'status'), (), pm, out)
                    args = provision.get('args')
                    if type(args) is not list or any(type(arg) is not str or '\0' in arg for arg in args):
                        out.append(Refusal(pm + '.args', 'expected literal strings without NUL'))
                    status = provision.get('status')
                    if type(status) is not int or not 0 <= status <= 255:
                        out.append(Refusal(pm + '.status', 'expected exit status 0..255'))
                    for field in ("reads", "writes"):
                        _list(p.get(field), m + "." + field, out)
            else:
                p = item if type(item) is dict else {}
            argv = _list(p.get("argv"), m + ".argv", out)
            if not argv:
                out.append(Refusal(m + ".argv", "program required"))
            env = p.get("env")
            if type(env) is not dict:
                out.append(Refusal(m + ".env", "expected complete environment object"))
            else:
                for key in env:
                    if type(key) is not str or not key or "=" in key or "\x00" in key:
                        out.append(Refusal(m + ".env", "invalid environment name"))

    for member, text, phase, process, executable in template_uses(exp, files):
        try:
            parts = _parts(text)
        except Refused as error:
            out.append(Refusal(member, error.refusal.why))
            continue
        valid = True
        for is_placeholder, name in parts:
            if not is_placeholder:
                continue
            bits = name.split(".")
            available = False
            immutable = False
            if name == "experiment":
                available = immutable = True
            elif name == "runner":
                available = immutable = True
            elif name in ('browser.program', 'browser.chromium', 'browser.fonts'):
                available = browser_available
                immutable = name in ('browser.program', 'browser.chromium')
            elif len(bits) == 2 and bits[0] == 'program':
                available = bits[1] in named and NAME.fullmatch(bits[1]) is not None
                immutable = True
                if not available:
                    out.append(Refusal(member, 'program ' + bits[1] + ' not named by the settings'))
                    valid = False
                    continue
            elif len(bits) == 2 and bits[0] == 'port':
                available = bits[1] in ports and NAME.fullmatch(bits[1]) is not None
                if not available:
                    out.append(Refusal(member, 'port ' + bits[1] + ' not named by the settings'))
                    valid = False
                    continue
            elif name == "source":
                available = subject.get("source") is not None and facts.get("source_available") is True
            elif len(bits) == 2 and bits[0] == "artifact":
                available = bits[1] in artifacts and bits[1] in available_artifacts
                immutable = True
            elif name in ("run.id", "run.seed"):
                available = True
            elif len(bits) == 2 and bits[0] == "seed":
                available = NAME.fullmatch(bits[1]) is not None
            elif len(bits) == 2 and bits[0] == "dir":
                available = bits[1] in dirs
            elif len(bits) == 2 and bits[0] == 'held':
                available = bits[1] in held and phase in ('actions', 'release') and '.argv.' in member
            elif len(bits) == 3 and bits[0] == "proc" and bits[2] == "dir":
                available = bits[1] in processes
            elif len(bits) == 4 and bits[0] == "proc" and bits[2] == "ready":
                available = bits[1] in processes and bits[3] in captures.get(bits[1], ())
                if phase == "processes":
                    available = available and bits[1] in ancestors.get(process, ()) and bits[1] != process
            elif len(bits) == 3 and bits[0] == 'action':
                declaration = action_captures.get(bits[1])
                available = (declaration is not None and bits[2] == declaration[1]
                             and (phase in ('collect', 'checks', 'release', 'meter')
                                  or phase == 'actions' and int(member.split('.')[1]) > declaration[0]))
            elif name == "collect.dir":
                available = phase == "collect"
            elif len(bits) == 2 and bits[0] == "collected":
                collection = names["collect"].get(bits[1])
                check = groups["checks"][int(member.split('.')[1])] if phase == "checks" else {}
                available = (phase == 'checks' and collection is not None
                             and (collection.get("cluster", "up") == "up" or check.get("cluster", "up") == "stopped"))
            if not available:
                out.append(Refusal(member, "unknown or unavailable template: " + name))
                valid = False
            if executable and not immutable:
                out.append(Refusal(member, "executable identity must use only experiment or artifact inputs"))
                valid = False
        if executable and valid:
            try:
                path = render(text, known)
            except Refused as error:
                out.append(Refusal(member, error.refusal.why))
                continue
            metadata = programs.get(path)
            runner = known.get('runner')
            if type(runner) is str and path.startswith(runner + '/'):
                suffix = path[len(runner) + 1:]
                if not relative(suffix) or suffix.split('/')[0] == 'tests':
                    out.append(Refusal(member, 'runner program must be in its tree outside tests/'))
            if (not path.startswith("/") or type(metadata) is not dict
                    or metadata.get("regular") is not True or metadata.get("exec") is not True
                    or not digest_value(metadata.get("sha256"))):
                out.append(Refusal(member, "program must be an absolute executable regular file with known identity"))
            elif member.startswith('driver.tools.'):
                # A tool's program is a mount in its view: bubblewrap refuses a symlink there.
                resolved = metadata.get('path', path)
                if resolved != path:
                    out.append(Refusal(member, 'expected a resolved path: ' + str(resolved)))
        if ".reads." in member or ".writes." in member:
            # These paths are roots in a tool's view, not shell expressions.
            if not text.startswith(("/", "{experiment}", "{runner}", "{browser.", "{program.", "{artifact.", "{source}", "{dir.", "{proc.")) or ".." in text.split("/"):
                out.append(Refusal(member, "expected a contained absolute template path"))
            else:
                # A root known before the run is a mount too (bubblewrap refuses a symlink).
                try:
                    root = render(text, known)
                except Refused:
                    root = None
                if type(resolved_roots.get(root)) is str and resolved_roots[root] != root:
                    out.append(Refusal(member, 'expected a resolved path: ' + resolved_roots[root]))

    for i, entry in enumerate(_list(exp.get("faults"), "faults", out)):
        m = f"faults.{i}"
        fault = shape(entry, ("kill", "signal", "after_actions"), (), m, out)
        if type(fault.get("kill")) is not str or fault["kill"] not in processes:
            out.append(Refusal(m + ".kill", "unknown process"))
        if type(fault.get("signal")) is not str or not re.fullmatch(r"SIG[A-Z0-9]+", fault["signal"]):
            out.append(Refusal(m + ".signal", "expected signal name"))
        bounds = fault.get("after_actions")
        if type(bounds) is not list or len(bounds) != 2 or not all(positive(x) for x in bounds) or bounds[0] > bounds[1]:
            out.append(Refusal(m + ".after_actions", "expected positive inclusive range"))
        if driver is None:
            out.append(Refusal(m, "fault requires a driver"))
    planted = _list(exp.get("planted"), "planted", out)
    seen = set()
    for i, signature in enumerate(planted):
        if not signature_value(signature):
            out.append(Refusal(f"planted.{i}", "expected a printable ASCII signature without spaces"))
        elif signature in seen:
            out.append(Refusal(f"planted.{i}", "duplicate signature"))
        else:
            seen.add(signature)
    return out


def key(exp_digest, subject, lab):
    if not digest_value(exp_digest):
        raise Refused("experiment", "expected a tree digest")
    refusals = subject_refusals(subject) + lab_refusals(lab)
    if refusals:
        raise Refused(refusals[0].member, refusals[0].why)
    value = {"format": "lab-key/1", "experiment": exp_digest,
             "subject": canonical.digest(subject), "lab": lab}
    # Copy through canonical bytes so later edits of inputs cannot mutate a key.
    return canonical.loads(canonical.record_bytes(value))
