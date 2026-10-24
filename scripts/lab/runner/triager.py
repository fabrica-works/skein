"""One bounded, contained Codex triage, with retained evidence only (lab.md, 9.2)."""
import json
import os
from pathlib import Path
import select
import signal
import time
from . import canonical, codex, driver, finding_store, probe, scripted, store, transport, triage, views
from .host import CapabilityError, Host
from .loop import utc
from .records import Refused, triage_refusals


def retained(lab, run_base, destination):
    """Never walk scratch, dirs, or the driver's host, rehearsal, provisioning or probe homes."""
    for name in ('run.json', 'events.jsonl'):
        lab.write_once(destination + '/' + name, lab.read(run_base + '/' + name))
    for name in ('proc', 'commands', 'collected', 'driver'):
        try:
            fd = lab.directory(run_base + '/' + name)
        except FileNotFoundError:
            continue
        os.close(fd)
        excluded = ('home', 'rehearsal/home', 'provision/home', 'probe') if name == 'driver' else ()
        for file in store.snapshot(lab.root / run_base / name, exclude=excluded):
            lab.write_once(destination + '/' + name + '/' + file['path'], file['data'], executable=file['exec'])
    fd = lab.directory(destination)
    try:
        lab._freeze(fd)
    finally:
        os.close(fd)


def report(lab, run_base, run):
    ref = (run.get('driver') or {}).get('files', {}).get('report.json')
    if ref is None:
        return None
    data = lab.read(run_base + '/driver/report.json')
    if store.file_digest(data) != ref['sha256'] or len(data) != ref['bytes']:
        raise Refused('triage.report', 'retained report differs from run record')
    return codex.report(data.decode('utf-8', 'strict'))


class Triage:
    def __init__(self, lab, spec):
        self.lab, self.spec, self.base = lab, spec, spec['base']
        self.cost = spec['bounds']
        self.root = lab.root / self.base
        self.started = utc()
        self.origin = None
        self.observed = codex.initial()
        self.expected, self.inventory, self.probe = None, None, None
        self.invocation, self.protocol, self.host = None, None, None
        self.errors, self.events = [], []
        self.outcome = None
        self.input, self.output = None, None
        self.overshoot = None
        self.stop_at = None
        self.stopping = False
        self.end = None
        self.files = {}
        self.endpoint = None

    def ms(self):
        return 0 if self.origin is None else (time.monotonic_ns() - self.origin) // 1_000_000

    def error(self, what, detail):
        self.errors.append(dict(what=what, detail=detail))
        if self.outcome != 'bound':
            self.outcome = 'error'

    def bound(self, name):
        if not any(e['what'] == 'bound/' + name for e in self.errors):
            self.errors.append(dict(what='bound/' + name, detail='triage bound passed'))
        self.outcome = 'bound'

    def received(self, message):
        data = (json.dumps(dict(t_ms=self.ms(), **message), ensure_ascii=False, allow_nan=False) + '\n').encode()
        view = memoryview(data)
        while view:
            size = os.write(self.files['events.jsonl'], view)
            if size == 0:
                raise OSError('triage event write made no progress')
            view = view[size:]
        if 'method' in message and 'id' not in message:
            self.observed = codex.fold(self.observed, message, self.ms(), triage=True)
            if self.observed['unhandled'] is not None:
                raise Refused('triage.audit', self.observed['unhandled'])
            if message['method'] in ('turn/completed', 'thread/tokenUsage/updated'):
                total = triage.token_total(self.observed)
                if total is not None and total > self.cost['triage_tokens']:
                    self.overshoot = total - self.cost['triage_tokens']
                    self.bound('triage_tokens')

    def inventoried(self, responses):
        self.inventory = codex.observe(responses, self.expected)
        if any(row['state'] == 'mismatch' for row in self.inventory.values()):
            raise Refused('triage.inventory', 'host inventory differs from expected')
        self.protocol.start_turn()

    def prepare(self):
        transport.check_network(self.spec)
        lab, spec = self.lab, self.spec
        settings = canonical.loads(lab.read('settings.json'))
        from .cli import _triager_settings
        configured = _triager_settings(settings)
        if configured != spec['triager']:
            raise Refused('settings.triager', 'triager changed before provision')
        history, records, _paths, facts = finding_store.load(lab)
        runs = [r for r in records.get(spec['key'], []) if r['run'] == spec['run']]
        if len(runs) != 1:
            raise Refused('triage.run', 'judged run unavailable')
        self.supplied = triage.input_value(runs[0], history, records,
                                          report(lab, spec['run_base'], runs[0]), facts)
        lab.write_once(self.base + '/input.json', canonical.record_bytes(self.supplied))
        retained(lab, spec['run_base'], self.base + '/evidence')
        for name in ('work', 'home'):
            os.close(lab.directory(self.base + '/' + name, create=True))
        home = self.root / 'rehearsal/home' if 'rehearsal' in spec else Path(settings.get('codex_home', str(lab.root / 'codex')))
        if 'rehearsal' in spec:
            os.close(lab.directory(self.base + '/rehearsal', create=True))
        profile = views.profile_toml(configured['program']['path'], str(self.root / 'evidence'),
                                     str(self.root / 'work'), name='triage')
        text = codex.config_toml('', '', spec['run'], [], configured['model'], configured['effort'],
            profile, catalog=configured.get('catalog', {}).get('path'), mcp=False)
        lab.write_once(self.base + '/config.toml', text.encode())
        try:
            lab.read('canary')
        except FileNotFoundError:
            lab.write_once('canary', b'lab boundary canary\n')
        try:
            self.probe = probe.boundary_probe(configured['program']['path'], self.root,
                source=str(self.root / 'evidence'), work=str(self.root / 'work'),
                canary=str(lab.root / 'canary'), home=str(Path.home()), seconds=self.cost['command_seconds'],
                profile=profile, permission='triage')
        except probe.BoundaryError as error:
            self.probe = error.results
            raise
        finally:
            if self.probe is not None:
                lab.write_once(self.base + '/probe.json', canonical.record_bytes(self.probe))
        driver._write_config(home, text.encode())
        self.expected = codex.expected_inventory([], str(home / 'config.toml'), profile='triage')
        prompt = (Path(__file__).resolve().parents[1] / 'preamble/triager.md').read_text()
        prompt += '\n\n' + lab.read(self.base + '/input.json').decode()
        schema = json.loads((Path(__file__).resolve().parents[1] / 'schemas/triage.json').read_text())
        self.protocol = transport.AppServer(str(self.root / 'work'), prompt, schema,
                                            self.received, self.inventoried, permission='triage')
        self.invocation = dict(argv=[configured['program']['path'], 'app-server', '--stdio', '--strict-config'],
            env=dict(PATH=os.defpath, LANG='C.UTF-8', HOME=str(self.root / 'home'), CODEX_HOME=str(home)),
            cwd=str(self.root / 'work'))

    def run_host(self, wake):
        self.host = Host(self.spec['unit'], self.spec['systemctl'], self.cost)
        self.lab.append_line(self.base + '/events.jsonl', dict(event='containment', cgroup=str(self.host.root)))
        if 'rehearsal' in self.spec:
            self.endpoint = scripted.Endpoint(self.lab, self.base + '/rehearsal', self.host,
                                              self.spec['rehearsal']['value'], self.cost['command_seconds'])
            address = self.endpoint.ready(wake)
            text = codex.with_endpoint(self.lab.read(self.base + '/config.toml').decode(), address)
            driver._write_config(Path(self.invocation['env']['CODEX_HOME']), text.encode())
        read, self.input = os.pipe2(os.O_CLOEXEC)
        os.set_blocking(self.input, False)
        try:
            child = self.host.spawn('triager', self.invocation['argv'], self.invocation['env'], read,
                                    cwd=self.invocation['cwd'])
        finally:
            os.close(read)
        self.origin = time.monotonic_ns()
        buffers = {'stdout': b'', 'ack': b''}
        cleanup_at = None
        while 'triager' in self.host.children:
            if self.endpoint is not None:
                self.endpoint.poll()
            now = self.ms()
            if self.outcome is None and now >= self.cost['triage_seconds'] * 1000:
                self.bound('triage_seconds')
            completed = any(t['completed_ms'] is not None for t in self.observed['turns'])
            if (self.outcome is not None or completed) and not self.stopping:
                self.stopping = True
                os.close(self.input)
                self.input = None
                self.stop_at = now + self.cost['stop_seconds'] * 1000
                if self.outcome is not None:
                    self.host.signal_first('triager', signal.SIGTERM)
            if self.stop_at is not None and now >= self.stop_at:
                self.host.kill_tree('triager')
                self.stop_at = None
                cleanup_at = now + self.cost['stop_seconds'] * 1000
            if child['exec_ack'] is not None:
                data = os.read(child['exec_ack'], 65536) if select.select([child['exec_ack']], [], [], 0)[0] else None
                if data:
                    buffers['ack'] += data
                elif data == b'':
                    os.close(child['exec_ack'])
                    child['exec_ack'] = None
                    if buffers['ack']:
                        self.error('triage.host', buffers['ack'].decode('utf-8', 'replace'))
            for name, fd in list(child['streams'].items()):
                try:
                    data = os.read(fd, 65536)
                except BlockingIOError:
                    continue
                if not data:
                    os.close(fd)
                    del child['streams'][name]
                    if name == 'stdout' and buffers['stdout']:
                        self.error('triage.transport', 'partial app-server line at EOF')
                    continue
                self.write_log('host.' + name, data)
                if name == 'stdout':
                    buffers['stdout'] += data
                    while b'\n' in buffers['stdout']:
                        line, buffers['stdout'] = buffers['stdout'].split(b'\n', 1)
                        try:
                            self.protocol.accept(line)
                        except (ValueError, RuntimeError, KeyError, TypeError, AttributeError) as error:
                            self.error('triage.transport', str(error))
            if not child['reaped'] and self.host.exited('triager'):
                self.end = self.host.reap('triager')
                if self.end is not None:
                    if self.end.get('status') != 0:
                        self.error('triage.host', 'host ended: ' + str(self.end))
                    self.host.kill_tree('triager')
                    cleanup_at = self.ms() + self.cost['stop_seconds'] * 1000
            if child['reaped'] and not child['streams'] and not self.host.counts('triager')['populated']:
                self.host.remove('triager')
                break
            if cleanup_at is not None and self.ms() >= cleanup_at:
                raise RuntimeError('triage tree did not drain within stop_seconds')
            if not self.stopping and (self.outcome is not None or any(
                    t['completed_ms'] is not None for t in self.observed['turns'])):
                continue
            if self.input is not None and self.protocol.outgoing and self.outcome is None:
                try:
                    size = os.write(self.input, self.protocol.outgoing)
                    self.protocol.outgoing = self.protocol.outgoing[size:]
                except BlockingIOError:
                    pass
                except BrokenPipeError:
                    self.error('triage.transport', 'host closed stdin')
            deadlines = [d for d in (self.stop_at, cleanup_at) if d is not None]
            if self.outcome is None and not completed:
                deadlines.append(self.cost['triage_seconds'] * 1000)
            descriptors = [(self.input, select.POLLOUT)] if self.input is not None and self.protocol.outgoing else []
            transport.driver_poll(self.host, max(0, min(deadlines) - self.ms()) if deadlines else None, wake, descriptors)
            try:
                os.read(wake, 4096)
            except BlockingIOError:
                pass
        if self.endpoint is not None:
            self.endpoint.stop(wake, self.cost['stop_seconds'])
        if self.outcome is None and not any(t['completed_ms'] is not None for t in self.observed['turns']):
            self.error('triage.host', 'host ended without a completed turn')
        if self.outcome is None and (self.inventory is None or not self.protocol.turn_sent):
            self.error('triage.inventory', 'host ended before inventory and turn admission')

    def write_log(self, name, data):
        view = memoryview(data)
        while view:
            size = os.write(self.files[name], view)
            if size == 0:
                raise OSError('triage log write made no progress')
            view = view[size:]

    def run(self, admitted):
        parent, name = self.lab._parent(self.base, create=True)
        try:
            os.mkdir(name, 0o700, dir_fd=parent)
        finally:
            os.close(parent)
        context = dict(format='lab-triage/1', network=self.spec.get('network', 'host'), run=self.spec['run'], key=self.spec['key'], n=self.spec['triage_n'],
            started=self.started, ended=self.started, outcome='error', invocation=None,
            expected_inventory=None, inventory=None, probe=None, input=None, output=None, bounds=self.cost,
            usage=dict(seconds=None, turns=0, tokens=dict(input=None, cached=None, output=None), tokens_overshoot=None),
            errors=[], finding_events=[], brake=self.spec.get('brake'))
        if 'rehearsal' in self.spec:
            context['rehearsal'] = dict(script=self.spec['rehearsal']['script'])
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(dict(format='lab-live/1',
            unit=self.spec['unit'], systemctl=self.spec['systemctl'], context=context)))
        for name in ('events.jsonl', 'host.stdout', 'host.stderr'):
            self.files[name] = self.lab.create_file(self.base + '/' + name)
        wake_r, wake_w = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
        handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
        def interrupted(sig, _frame):
            self.error('interrupted', signal.Signals(sig).name)
        previous_wakeup = signal.set_wakeup_fd(wake_w)
        for sig in handlers:
            signal.signal(sig, interrupted)
        cleaned = False
        try:
            admitted()
            try:
                self.prepare()
                self.run_host(wake_r)
                cleaned = self.host is None or not self.host.children
                if self.outcome is None:
                    output = triage.output_value(self.observed['final_message'], self.supplied)
                    # A rehearsal plans the same additions, so it fails where the paid triage would, and appends none.
                    history, records, _paths, facts = finding_store.load(self.lab)
                    additions = triage.additions(self.supplied, output, history, records, utc(), facts)
                    if 'rehearsal' not in self.spec:
                        self.events = finding_store.append_triage(self.lab, additions)
                    self.outcome = 'proposed'
            except Exception as error:
                self.error('containment' if isinstance(error, CapabilityError) else 'triage', str(error))
                cleaned = self.host is None or not self.host.children
            final = self.observed['final_message']
            self.lab.write_once(self.base + '/output.json', (final + '\n').encode() if type(final) is str else b'null\n')
            if self.inventory is None and self.expected is not None:
                self.inventory = codex.observe(self.protocol.responses, self.expected)
            for name, fd in self.files.items():
                os.fsync(fd)
            references = {}
            for name in ('input.json', 'output.json'):
                try:
                    data = self.lab.read(self.base + '/' + name)
                except FileNotFoundError:
                    references[name] = None
                else:
                    references[name] = dict(path=name, bytes=len(data), sha256=store.file_digest(data))
            value = dict(format='lab-triage/1', network=self.spec.get('network', 'host'), run=self.spec['run'], key=self.spec['key'], n=self.spec['triage_n'],
                started=self.started, ended=utc(), outcome=self.outcome, invocation=self.invocation,
                expected_inventory=self.expected, inventory=self.inventory, probe=self.probe,
                input=references['input.json'], output=references['output.json'], bounds=self.cost,
                usage=dict(seconds=None if self.origin is None else (self.ms() + 999) // 1000,
                    turns=len(self.observed['turns']), tokens=self.observed['tokens'], tokens_overshoot=self.overshoot),
                errors=self.errors, finding_events=[dict(id=i, value=v) for i, v in self.events], brake=self.spec.get('brake'))
            if 'rehearsal' in self.spec:
                value['rehearsal'] = dict(script=self.spec['rehearsal']['script'])
            problems = triage_refusals(value)
            if problems:
                raise Refused(problems[0].member, problems[0].why)
            self.lab.write_once(self.base + '/triage.json', canonical.record_bytes(value))
            if cleaned:
                self.lab.remove_live(self.base + '/live.json')
            return value
        finally:
            if self.host is not None:
                for identity in self.host.children:
                    self.host.kill_tree(identity)
            if self.endpoint is not None:
                self.endpoint.close()
            if self.input is not None:
                os.close(self.input)
            for fd in self.files.values():
                os.close(fd)
            signal.set_wakeup_fd(previous_wakeup)
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
            os.close(wake_r)
            os.close(wake_w)
