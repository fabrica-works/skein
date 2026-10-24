"""One native event loop carrying out the pure life (lab.md, 10.4)."""
from datetime import datetime, timezone
import os
import platform
import signal
import time
from . import bootstrap, canonical, checks, driver, evidence, held, judge, life, meter, store, transport
from .host import CapabilityError, Host
from .records import Refused


FINISH_EFFECTS = ('cancel_command', 'stop_driver', 'unavailable', 'bound', 'driver_use', 'bootstrap_received', 'bootstrap_delivered', 'give_held', 'meter_failure')


def utc():
    return datetime.now(timezone.utc).isoformat(timespec='milliseconds').replace('+00:00', 'Z')


def _write(fd, data):
    while data:
        written = os.write(fd, data)
        if written == 0:
            raise OSError('evidence write made no progress')
        data = data[written:]


class Run:
    def __init__(self, lab, spec, host_factory=Host):
        self.lab, self.spec, self.host_factory = lab, spec, host_factory
        self.base = spec.get('base', spec['key_path'] + '/runs/' + spec['run'])
        self.key = canonical.loads(lab.read(spec['key_path'] + '/key.json'))
        exp_base = 'store/experiments/' + self.key['experiment']
        self.exp = canonical.loads(lab.read(exp_base + '/experiment.json'))
        self.cost = spec['bounds']
        self.started = utc()
        self.origin = time.monotonic_ns()
        self.host = None
        self.driver = None
        self.drive = None
        self.driver_record, self.driver_usage = None, None
        self.records, self.command_records = {}, []
        self.held_records = []
        self.effects = []
        self.finishing = False
        self.publishing = False
        self.failures, self.undecided, self.errors = [], [], []
        self.interrupt = None
        self.items = {}
        self.bootstrap_states = {p['name']: bootstrap.State() for p in self.exp['processes'] if 'bootstrap' in p}
        self.texts = {f['path']: f['data'].decode('utf-8', 'strict') for f in store.snapshot(lab.root / exp_base)
                      if any(f['path'] == x.get('from') for p in self.exp['processes'] for x in p.get('files', []))}
        subject = canonical.loads(lab.read('store/subjects/' + self.key['subject'] + '/subject.json'))
        absolute = lambda path: str(lab.root / path)
        known = {'experiment': absolute(exp_base), 'source': absolute('store/subjects/' + self.key['subject'] + '/source'),
                 'run.id': spec['run'], 'run.seed': spec['seed']}
        known.update({'artifact.' + n: absolute('store/subjects/' + self.key['subject'] + '/artifacts/' + n) for n in subject['artifacts']})
        known.update({'dir.' + n: absolute(self.base + '/dirs/' + n) for n in self.exp['dirs']})
        known.update({'proc.' + p['name'] + '.dir': absolute(self.base + '/scratch/' + p['name']) for p in self.exp['processes']})
        known.update({'collected.' + c['name']: absolute(self.base + '/collected/' + c['name']) for c in self.exp['collect']})
        known.update({'held.' + n: absolute(self.base + '/held/' + n) for n in self.exp.get('held', [])})
        self.state = life.initial(self.exp, spec['seed'], self.cost, known, self.texts)

    def ms(self):
        return (time.monotonic_ns() - self.origin) // 1_000_000

    def timeline(self, event, *, ms=None, **fields):
        self.lab.append_line(self.base + '/events.jsonl', dict(format='lab-event/1', event=event,
                             ms=self.ms() if ms is None else ms, **fields))

    def drop_bootstrap(self):
        self.bootstrap_states = {name: bootstrap.drop(state) for name, state in self.bootstrap_states.items()}

    def feed(self, kind, *, ms=None, **fields):
        event = dict(kind=kind, ms=self.ms() if ms is None else ms, **fields)
        self.state, effects = life.step(self.state, event)
        if kind in ('interrupt', 'error') or any(e['op'] == 'cut' for e in effects):
            self.drop_bootstrap()
        if kind in ('exit', 'bootstrap_failed'):
            self.effects[0:0] = effects
        else:
            self.effects.extend(effects)

    def provision(self):
        paths = ['scratch/' + p['name'] for p in self.exp['processes']]
        paths += ['dirs/' + n for n in self.exp['dirs']]
        paths += ['collected/' + c['name'] for c in self.exp['collect']]
        paths += ['commands', 'proc']
        if self.exp.get('meter') is not None:
            paths.append('meter')
        for path in paths:
            os.close(self.lab.directory(self.base + '/' + path, create=True))
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(dict(format='lab-live/1', unit=self.spec['unit'])))
        self.take_held()
        if self.exp['driver'] is not None:
            self.driver = driver.Driver(self)
            self.driver.prepare()
            self.drive = self.driver
        try:
            self.host = self.host_factory(self.spec['unit'], self.spec['systemctl'], self.cost)
        except (OSError, CapabilityError) as error:
            raise CapabilityError('native provision refused: ' + str(error)) from error
        self.timeline('containment', cgroup=str(self.host.root), controllers=['memory', 'pids'])
        self.feed('provisioned')

    def take_held(self):
        for name in self.exp.get('held', []):
            held.take(self.lab, self.base, name, self.spec['run'], self.started)
            ms = self.ms()
            self.held_records.append(dict(name=name, taken_ms=ms, given_ms=None))
            self.timeline('held_taken', ms=ms, name=name)

    def give_held(self):
        for record in self.held_records:
            if record['given_ms'] is not None:
                continue
            name = record['name']
            try:
                given = held.give_back(self.lab, self.base, name)
            except (OSError, Refused):
                given = False
            if given:
                ms = self.ms()
                record['given_ms'] = ms
                self.timeline('held_given', ms=ms, name=name)
            else:
                self.failures.append(checks.failure('release/held:' + name, 'release', 'held file not given back'))

    def spawn(self, effect):
        name, group = effect['name'], effect['group']
        phase = self.state.data['phase']
        if group == 'meter':
            active = self.state.data['meter']
            if phase != 'drive' or not active['active'] or active['n'] != effect['n']:
                return
        expected = ('stopped-' + ('collect' if group == 'collect' else 'check') if effect.get('cluster') == 'stopped'
                    else dict(processes='start', actions='seed', collect='collect', checks='check', release='release', meter='drive')[group])
        if phase != expected or group != 'meter' and self.state.data['current'] != name:
            return
        identity = ('proc-' if group == 'processes' else group + '-') + name
        prefix = 'proc/' + name + '/' if group == 'processes' else 'commands/' + group + '-' + name + '.'
        if group == 'meter':
            identity = 'meter-' + str(effect['n'])
            prefix = 'meter/' + str(effect['n']) + '.'
        record = dict(name=name, group=group, argv=effect['argv'], env=effect['env'], started_ms=self.ms(),
                      ready_ms=None, captures={}, exit=None,
                      memory=dict(peak_bytes=None, source=None, bound_bytes=None, tree_peak_bytes=None), output={})
        if group == 'meter':
            record['n'] = effect['n']
        if group == 'processes':
            self.records[name] = record
            definition = next(p for p in self.exp['processes'] if p['name'] == name)
            record['memory']['bound_bytes'] = definition['memory_bytes']
            if 'bootstrap' in definition:
                record['bootstrap'] = dict(**definition['bootstrap'], received_ms=None, delivered_ms=None)
                record['output']['stdout'] = None
        else:
            if effect.get('cluster') == 'stopped':
                record['cluster'] = 'stopped'
            if 'bootstrap' in effect:
                record.update(bootstrap=effect['bootstrap'], stdin=None, output=dict(stdout=None, stderr=None))
            if group != 'meter' and effect.get('cluster') != 'stopped':
                self.command_records.append(record)
            if group == 'actions' and 'await' in self.exp['actions'][self.state.data['index']]:
                record['awaited'] = None
        cwd = self.state.data['known'].get('proc.' + name + '.dir', str(self.lab.root / self.base)) if group == 'processes' else str(self.lab.root / self.base)
        for file in effect['files']:
            if 'tree' in file:
                continue
            target = self.base + '/scratch/' + name + '/' + file['path']
            fd = self.lab.create_file(target)
            try:
                _write(fd, file['text'].encode('utf-8', 'strict'))
                os.fsync(fd)
            finally:
                os.close(fd)
        for file in effect['files']:
            if 'tree' in file:
                data = self.lab.read('store/subjects/' + self.key['subject'] + '/artifacts/' + file['tree'])
                self.lab.unpack_tree(self.base + '/scratch/' + name + '/' + file['path'], data)
        stdin_fd = None
        outputs = {}
        source = effect.get('bootstrap', {}).get('process')
        line = None
        try:
            if source is not None:
                self.bootstrap_states[source], line = bootstrap.take(self.bootstrap_states[source])
                if line is None:
                    raise RuntimeError('private bootstrap input unavailable')
                stdin_fd, writer = os.pipe2(os.O_CLOEXEC)
                try:
                    # The framed line fits one empty Linux pipe while its read end is held.
                    if os.write(writer, line) != len(line):
                        raise OSError('private bootstrap pipe write incomplete')
                finally:
                    line = None
                    os.close(writer)
            else:
                stdin_fd = os.open(os.devnull, os.O_RDONLY | os.O_CLOEXEC)
                if effect['stdin'] is not None:
                    path = prefix + 'stdin'
                    self.lab.write_once(self.base + '/' + path, effect['stdin'].encode('utf-8', 'strict'))
                    os.close(stdin_fd)
                    stdin_fd = None
                    stdin_fd = os.open(self.lab.root / self.base / path, store.FILE_FLAGS)
                    record['stdin'] = self.file_record(path)
            for stream in ('stdout', 'stderr'):
                if source is not None or group == 'processes' and 'bootstrap' in record and stream == 'stdout':
                    outputs[stream] = None
                    record['output'][stream] = None
                else:
                    path = prefix + stream
                    outputs[stream] = dict(path=path, fd=self.lab.create_file(self.base + '/' + path), buffer=b'')
            owned = self.host.spawn(identity, effect['argv'], effect['env'], stdin_fd, cwd=cwd)
            if effect.get('cluster') == 'stopped':
                self.command_records.append(record)
            if source is not None:
                self.feed('bootstrap_delivered', name=source)
        except BaseException:
            self.drop_bootstrap()
            for output in outputs.values():
                if output is not None:
                    os.close(output['fd'])
            raise
        finally:
            line = None
            if stdin_fd is not None:
                os.close(stdin_fd)
        self.items[identity] = dict(name=name, group=group, record=record, outputs=outputs,
                                    cleanup=None, kill_deadline=None, timed_out=False, cancelled=False,
                                    panic=[], panic_message=None)
        self.feed('spawned', name=name, group=group)
        self.timeline('start', name=name, group=group, argv=effect['argv'], env=effect['env'], pid=owned['pid'])

    def file_record(self, path):
        data = self.lab.read(self.base + '/' + path)
        return dict(path=path, bytes=len(data), sha256=store.file_digest(data))

    def spawn_driver(self, argv, env, stdin_fd, cwd, *, identity='driver', base=None, group='driver'):
        base = self.base + '/driver' if base is None else base
        record = dict(name=identity, group=group, argv=argv, env=env, started_ms=self.ms(),
                      exit=None, memory=dict(peak_bytes=None, source=None, tree_peak_bytes=None), output={})
        outputs = {}
        try:
            for stream in ('stdout', 'stderr'):
                path = base[len(self.base) + 1:] + ('/endpoint.' if group == 'endpoint' else '/host.') + stream
                outputs[stream] = dict(path=path, fd=self.lab.create_file(self.base + '/' + path), buffer=b'')
            if stdin_fd is None:
                with open(os.devnull, 'rb') as null:
                    owned = self.host.spawn(identity, argv, env, null.fileno(), cwd=cwd)
            else:
                owned = self.host.spawn(identity, argv, env, stdin_fd, cwd=cwd)
        except BaseException:
            for output in outputs.values():
                os.close(output['fd'])
            raise
        self.items[identity] = dict(name=identity, group=group, record=record, outputs=outputs,
                                   cleanup=None, kill_deadline=None, timed_out=False, cancelled=False,
                                   panic=[], panic_message=None)
        self.timeline('start', name=identity, group=group, argv=argv, env=env, pid=owned['pid'])

    def begin_cleanup(self, identity):
        item = self.items[identity]
        if item['cleanup'] is not None:
            return
        counts = self.host.counts(identity)
        item['record']['memory']['tree_peak_bytes'] = counts['tree_peak_bytes']
        item['cleanup'] = (item['kill_deadline'] if item['kill_deadline'] is not None
                           else self.ms() + self.cost['stop_seconds'] * 1000)
        item['kill_deadline'] = None
        survivors = checks.survivors(item['name'], counts)
        self.failures.extend(survivors)
        if survivors:
            self.timeline('survivors', name=item['name'], pids=counts['pids'])
            self.host.kill_tree(identity)

    def cancel_command(self, name, group=None, timed_out=False):
        for identity, item in self.items.items():
            if item['group'] != 'processes' and item['name'] == name and (group is None or item['group'] == group):
                item['cancelled'] = True
                item['timed_out'] = item['timed_out'] or timed_out
                if item['cleanup'] is None:
                    self.host.kill_tree(identity)
                    item['kill_deadline'] = self.ms() + self.cost['stop_seconds'] * 1000

    def effect(self, request):
        op = request['op']
        if op == 'validate':
            self.feed('validated')
        elif op == 'provision':
            self.provision()
        elif op == 'give_held':
            self.give_held()
        elif op == 'phase':
            self.timeline('phase', phase=request['phase'])
        elif op == 'spawn':
            self.spawn(request)
        elif op == 'provision_tools':
            if self.state.data['phase'] == 'drive' and not self.state.data['tool_provision']['ending']:
                self.driver = driver.Driver(self, provisioning=True)
                self.driver.start()
        elif op == 'start_driver':
            if self.state.data['phase'] == 'drive' and not self.state.data['driver']['ending']:
                self.driver.start()
        elif op == 'driver_turn':
            if self.driver is not None and self.driver.can_go():
                self.driver.protocol.start_turn()
        elif op == 'stop_driver':
            if self.driver is not None:
                self.driver.stop()
        elif op == 'driver_use':
            report = request['report']
            reasons = ([] if report['done'] else ['charter not completed']) + [s['step'] for s in report['unseen']]
            self.failures.extend(checks.failure('use/untriaged', 'use', reason, ['driver/report.json']) for reason in reasons)
        elif op in ('bootstrap_received', 'bootstrap_delivered'):
            field = 'received_ms' if op == 'bootstrap_received' else 'delivered_ms'
            self.records[request['process']]['bootstrap'][field] = request['ms']
            self.timeline(op, ms=request['ms'], process=request['process'], action=request['action'])
        elif op == 'ready':
            record = self.records[request['name']]
            record.update(ready_ms=self.ms(), captures=request['captures'])
            self.timeline('ready', name=request['name'], captures=request['captures'])
        elif op == 'awaited':
            record = next(c for c in self.command_records if c['group'] == 'actions' and c['name'] == request['name'])
            result = record['exit']
            timed_out = any(b['name'] == 'command_seconds' and b.get('who') == request['name'] for b in self.state.data['bounds'])
            if not timed_out and (result is None or result.get('status') == 0 and not result['stopped'] and not result['killed']):
                record['awaited'] = dict(ms=request['ms'], captures=request['captures'])
            self.timeline('awaited', ms=request['ms'], name=request['name'], captures=request['captures'])
        elif op == 'signal_first':
            identity = 'proc-' + request['name']
            if 'fault' in request and identity not in self.host.children:
                return
            if self.host.signal_first(identity, getattr(signal, request['signal'])):
                if 'fault' in request:
                    ms = self.ms()
                    self.feed('fault_fired', ms=ms, index=request['fault'])
                    fault = self.state.data['schedule']['faults'][request['fault']]
                    self.timeline('fault', ms=ms, index=fault['index'], kill=fault['kill'],
                                  signal=fault['signal'], n=fault['n'], target='first')
                else:
                    self.feed('signalled', name=request['name'])
                    self.timeline('signal', name=request['name'], signal=request['signal'], target='first')
        elif op == 'kill_tree':
            identity = 'proc-' + request['name']
            self.host.kill_tree(identity)
            self.items[identity]['kill_deadline'] = self.ms() + self.cost['stop_seconds'] * 1000
            self.timeline('kill_tree', name=request['name'])
        elif op == 'reap_tree':
            self.begin_cleanup('proc-' + request['name'])
        elif op == 'cancel_command':
            self.cancel_command(request['name'], request.get('group'), request.get('timed_out', False))
        elif op == 'meter_failure':
            self.failures.append(checks.failure(request['signature'], 'meter', 'consecutive meter readings failed'))
        elif op == 'bound':
            if request['name'] == 'command_seconds' and 'who' in request:
                for item in self.items.values():
                    if item['group'] != 'processes' and item['name'] == request['who']:
                        item['timed_out'] = True
                for record in self.command_records:
                    if record['group'] == 'actions' and record['name'] == request['who'] and 'awaited' in record:
                        record['awaited'] = None
            suffix = '/' + request['who'] if 'who' in request else ''
            self.failures.append(checks.failure(f"bound/{request['name']}" + suffix, 'bound', 'cost class bound passed'))
            self.timeline('bound', name=request['name'], **({'who': request['who']} if 'who' in request else {}))
        elif op == 'unavailable':
            self.undecided.append(dict(check=request['group'] + '/' + request['name'], reason=request['reason']))
            record = dict(group=request['group'], name=request['name'], skipped=request['reason'])
            if request.get('cluster') == 'stopped':
                record['cluster'] = 'stopped'
            if request['group'] == 'actions' and any(a['name'] == request['name'] and 'await' in a for a in self.exp['actions']):
                record['awaited'] = None
            self.command_records.append(record)
            self.timeline('unavailable', **{k: v for k, v in request.items() if k != 'op'})
        elif op == 'cut':
            self.timeline('cut', phase=request['phase'])
        elif op == 'judge':
            self.finishing = True

    def private_output(self, item, data):
        if item['group'] != 'processes':
            return
        name = item['name']
        state, event = bootstrap.feed(self.bootstrap_states[name], data)
        self.bootstrap_states[name] = state
        if event == 'received':
            self.feed('bootstrap_received', name=name)
        elif event is not None:
            self.failures.append(checks.failure('bootstrap/' + name + '/' + event, 'bootstrap',
                                                 'private bootstrap framing failed'))
            self.feed('bootstrap_failed', name=name)

    def read_output(self, identity, stream):
        item = self.items[identity]
        child = self.host.children[identity]
        if stream not in child['streams']:
            return
        fd = child['streams'][stream]
        output = item['outputs'][stream]
        try:
            data = os.read(fd, 65536)
        except BlockingIOError:
            return
        if output is None:
            self.private_output(item, data)
            if not data:
                os.close(fd)
                del child['streams'][stream]
            return
        if data:
            _write(output['fd'], data)
            output['buffer'] += data
            while b'\n' in output['buffer']:
                line, output['buffer'] = output['buffer'].split(b'\n', 1)
                self.output_line(item, stream, line[:-1] if line.endswith(b'\r') else line, complete=True)
        else:
            if output['buffer']:
                self.output_line(item, stream, output['buffer'], complete=False)
            os.close(fd)
            del child['streams'][stream]
            os.fsync(output['fd'])
            os.fchmod(output['fd'], 0o600 if item['group'] in ('driver', 'endpoint') else 0o400)
            os.close(output['fd'])
            output['fd'] = None
            item['record']['output'][stream] = self.file_record(output['path'])

    def output_line(self, item, stream, line, *, complete):
        if item['group'] == 'endpoint':
            if not complete:
                raise RuntimeError('endpoint ended with a partial line')
            self.driver.endpoint_line(stream, line)
            return
        if item['group'] == 'driver':
            if stream == 'stdout' and not self.driver.protocol_failed:
                try:
                    if not complete:
                        raise RuntimeError('app-server ended with a partial message')
                    self.driver.protocol.accept(line.decode('utf-8', 'strict'))
                except (Refused, RuntimeError, ValueError, KeyError, TypeError, UnicodeError) as error:
                    self.driver.protocol_failed = True
                    if self.driver.provisioning:
                        self.feed('provision_error', what='provision/host', detail=str(error))
                    else:
                        self.feed('error', what='driver/transport', detail=str(error))
            return
        text = line.decode('utf-8', 'replace')
        if stream == 'stderr':
            if item['panic_message'] is not None:
                item['panic_message']['summary'] = text
                item['panic_message'] = None
            found = checks.panics(item['name'], text, [item['outputs'][stream]['path']])
            item['panic'].extend(found)
            if found:
                item['panic_message'] = found[-1]
        if item['group'] == 'processes' and complete:
            try:
                ready_text = line.decode('utf-8', 'strict')
            except UnicodeError:
                return
            self.feed('line', name=item['name'], stream=stream, text=ready_text)

    def read_spawn_ack(self, identity):
        child = self.host.children[identity]
        if child['exec_ack'] is None:
            return
        try:
            data = os.read(child['exec_ack'], 65536)
        except BlockingIOError:
            return
        if data:
            record = self.items[identity]['record']
            record['spawn_error'] = record.get('spawn_error', '') + data.decode('utf-8', 'replace')
            self.feed('error', what='spawn/' + self.items[identity]['name'], detail=record['spawn_error'])
        else:
            os.close(child['exec_ack'])
            child['exec_ack'] = None

    def observe(self):
        if self.host is None:
            return
        # Reap all observed exits before readiness can request another spawn.
        for identity, child in list(self.host.children.items()):
            item = self.items[identity]
            self.read_spawn_ack(identity)
            if not child['reaped'] and self.host.exited(identity):
                result = self.host.reap(identity)
                if result is not None:
                    record = item['record']
                    record['memory'].update(peak_bytes=child['peak_bytes'], source=child['source'])
                    if item['group'] == 'processes':
                        self.feed('exit', name=item['name'], result=result, killed=child['killed'])
                        record['exit'] = self.state.data['processes'][item['name']]['exit']
                    else:
                        record['exit'] = dict(ms=self.ms(), phase=self.state.data['phase'],
                                              stopped=item['cancelled'], killed=child['killed'], **result)
                    self.timeline('exit', name=item['name'], group=item['group'], **record['exit'])
                    if item['group'] != 'processes':
                        self.begin_cleanup(identity)
                        if item['group'] == 'driver':
                            self.driver.poll()
                            self.driver.close_channels()
        for identity, child in list(self.host.children.items()):
            item = self.items[identity]
            for stream in list(child['streams']):
                self.read_output(identity, stream)
            if item['kill_deadline'] is not None and not child['reaped'] and self.ms() >= item['kill_deadline']:
                raise RuntimeError('killed first process did not exit within stop_seconds: ' + item['name'])
            if item['cleanup'] is not None:
                counts = self.host.counts(identity)
                item['record']['memory']['tree_peak_bytes'] = counts['tree_peak_bytes']
                if not counts['populated'] and not child['streams'] and child['exec_ack'] is None:
                    self.complete(identity)
                elif self.ms() >= item['cleanup']:
                    # Keep the tree for systemd's whole-unit cleanup; never claim it emptied.
                    self.errors.append(dict(what='containment/' + item['name'], detail='tree did not empty within stop_seconds'))
                    self.timeline('cleanup_failed', name=item['name'], pids=counts['pids'])
                    raise RuntimeError('tree did not empty within stop_seconds')

    def complete(self, identity):
        item = self.items[identity]
        name, group, record = item['name'], item['group'], item['record']
        if group != 'meter':
            self.failures.extend(item['panic'])
        error_start = len(self.errors)
        if group != 'processes':
            result = record['exit']
            if group == 'collect' or record.get('cluster') == 'stopped':
                result['killed'] = result['killed'] or self.host.children[identity]['killed']
                result['stopped'] = result['stopped'] or item['cancelled']
            if group == 'checks' and not item['cancelled']:
                failures, unknown, errors = checks.command(name, result.get('status'),
                    self.lab.read(self.base + '/' + record['output']['stdout']['path']), timed_out=item['timed_out'])
                for failure in failures:
                    failure['evidence'] = [record['output']['stdout']['path']]
                self.failures.extend(failures)
                self.undecided.extend(unknown)
                self.errors.extend(errors)
                record['check'] = dict(failures=failures, undecided=unknown, errors=errors)
            elif group == 'checks' and item['cancelled'] and not item['timed_out']:
                check = 'checks/' + name if record.get('cluster') == 'stopped' else name
                self.undecided.append(dict(check=check, reason='check cut by process stop or interruption'))
            if group == 'actions':
                timed_out = any(b['name'] == 'command_seconds' and b.get('who') == name for b in self.state.data['bounds'])
                if 'awaited' in record and (result.get('status') != 0 or item['cancelled'] or result['killed'] or timed_out):
                    record['awaited'] = None
                self.failures.extend(checks.action(name, record))
            if group == 'release':
                self.failures.extend(checks.release(name, record))
            if group == 'collect':
                try:
                    files = store.snapshot(self.lab.root / self.base / 'collected' / name)
                    record['collected'] = store.entries(files)
                    definition = next(c for c in self.exp['collect'] if c['name'] == name)
                    by_path = {f['path']: f['data'] for f in files}
                    for path in definition['scan']:
                        if path not in by_path:
                            self.undecided.append(dict(check='panics/' + name, reason='collected log missing: ' + path))
                        else:
                            self.failures.extend(checks.panics(name, by_path[path].decode('utf-8', 'replace'), ['collected/' + name + '/' + path]))
                except (OSError, ValueError) as error:
                    self.errors.append(dict(what='collection/' + name, detail=str(error)))
        if group == 'meter':
            self.complete_meter(item)
        self.host.remove(identity)
        if group == 'driver' and not self.driver.provisioning:
            self.driver.host_end = record['exit']
            self.driver.stop_endpoint()
            self.feed('driver_closed', result=record['exit'], stopped=item['cancelled'])
            self.driver_record, self.driver_usage = self.driver.finish()
            self.driver_record['end'] = record['exit']
            self.driver_record['files'].update(record['output'])
        if group == 'driver' and self.driver.provisioning:
            self.driver.host_end = record['exit']
            self.driver.stop_endpoint()
        del self.items[identity]
        self.timeline('tree_empty', name=name, group=group)
        if group == 'endpoint':
            self.driver.endpoint_end = record['exit']
            if self.driver.identity in self.host.children or record['exit'].get('status') != 0:
                if self.driver.provisioning:
                    self.feed('provision_error', what='provision/host', detail='endpoint failed or ended before its host')
                else:
                    self.feed('error', what='driver/host', detail='endpoint failed or ended before its host')
        if (group in ('driver', 'endpoint') and self.driver.provisioning
                and self.driver.identity not in self.host.children and self.driver.endpoint not in self.host.children):
            self.end_tools_provision(self.driver)
        if group == 'processes':
            self.feed('tree_empty', name=name)
        elif group not in ('driver', 'meter', 'endpoint'):
            self.feed('command_done', name=name, record=record)
        if group == 'collect' and len(self.errors) != error_start:
            self.feed('error', what='command/' + name, detail='command evidence could not be completed')

    def end_tools_provision(self, session):
        provision, errors = session.finish_provision()
        if session.endpoint_end is None or session.endpoint_end.get('status') != 0:
            errors.append(dict(what='provision/host', detail='endpoint ended otherwise'))
        self.drive.provision_record = provision
        self.driver = self.drive
        self.feed('tools_provisioned', errors=errors)

    def complete_meter(self, item):
        record = item['record']
        output = record['output']['stdout']['path']
        spend = meter.reading(self.lab.read(self.base + '/' + output))
        active = self.state.data['meter']
        timed_out = (item['timed_out'] or active['timed_out']
                     or active['deadline'] is not None and self.ms() >= active['deadline'])
        result = record['exit']
        good = result.get('status') == 0 and not timed_out
        spend = spend if good else None
        self.timeline('meter', n=record['n'], timed_out=timed_out, spend=spend,
                      **{k: result[k] for k in ('status', 'signal') if k in result})
        self.feed('meter_done', n=record['n'], record=dict(exit=result, timed_out=timed_out, spend=spend))

    def freeze_collections(self):
        if self.host is not None and self.host.children:
            return
        for command in self.command_records:
            if command['group'] != 'collect' or 'skipped' in command:
                continue
            path = self.base + '/collected/' + command['name']
            try:
                files = store.snapshot(self.lab.root / path)
                command['collected'] = store.entries(files)
                for file in files:
                    parent, name = self.lab._parent(path + '/' + file['path'])
                    try:
                        opened = os.open(name, store.FILE_FLAGS, dir_fd=parent)
                        try:
                            os.fchmod(opened, 0o500 if file['exec'] else 0o400)
                            os.fsync(opened)
                        finally:
                            os.close(opened)
                    finally:
                        os.close(parent)
                fd = self.lab.directory(path)
                try:
                    self.lab._freeze(fd)
                finally:
                    os.close(fd)
            except (OSError, ValueError) as error:
                self.errors.append(dict(what='collection/' + command['name'], detail=str(error)))

    def finish(self):
        self.drop_bootstrap()
        self.publishing = True
        self.freeze_collections()
        for name, record in self.records.items():
            panic = [f for f in self.failures if f['kind'] == 'panic' and f['signature'].startswith('panic/' + name + '/')]
            stop_bound = any(b['name'] == 'stop_seconds' and b.get('who') == name for b in self.state.data['bounds'])
            failures, unknown = checks.process(name, record, panic, stop_bound=stop_bound)
            self.failures.extend(f for f in failures if f not in self.failures)
            self.undecided.extend(unknown)
        self.errors.extend(self.state.data['errors'])
        self.undecided.extend(checks.faults(self.state.data['schedule']['faults']))
        if self.driver is not None and self.driver_record is None:
            self.driver_record, self.driver_usage = self.driver.finish()
        observed = self.state.data['driver']
        report = observed['observed']['report'] if observed is not None else None
        required = self.exp['driver'] is not None
        outcome = judge.outcome(self.failures, self.errors, self.undecided, report, driver_required=required)
        self.failures.extend(checks.planted(self.exp['planted'], self.failures, outcome))
        outcome = judge.outcome(self.failures, self.errors, self.undecided, report, driver_required=required)
        for i, failure in enumerate(self.failures):
            failure['id'] = 'failure-' + str(i + 1)
        record = dict(format='lab-run/1', run=self.spec['run'], key=self.spec['key'], experiment=self.exp['name'],
                      seed=self.spec['seed'], attempt=self.spec['attempt'], campaign=self.spec['campaign'],
                      started=self.started, ended=utc(), outcome=outcome, schedule=self.state.data['schedule'],
                      processes=self.records, commands=self.command_records, driver=self.driver_record, held=self.held_records,
                      failures=self.failures, undecided=self.undecided, errors=self.errors, bounds=self.cost,
                      usage=dict(elapsed_ms=self.ms(), longest_ms=self.longest_ms(), driver=self.driver_usage, triage=[],
                                 subject_spend=self.state.data['meter']['spend'] if self.state.data['meter'] is not None else None,
                                 windows=observed['observed']['windows'] if observed is not None else None),
                      about=dict(hostname=platform.node(), kernel=platform.release(), processors=os.cpu_count(),
                                 unit=self.spec['unit'], manager=self.spec.get('manager_version'),
                                 memory_bytes=os.sysconf('SC_PHYS_PAGES') * os.sysconf('SC_PAGE_SIZE')))
        self.timeline('outcome', outcome=outcome)
        if 'rehearsal' in self.spec:
            from .records import run_refusals
            record['rehearsal'] = dict(script=self.spec['rehearsal']['script'])
            problems = run_refusals(record)
            if problems:
                raise Refused(problems[0].member, problems[0].why)
            self.lab.write_once(self.base + '/run.json', canonical.record_bytes(record))
        else:
            evidence.publish(self.lab, self.spec['key_path'], record)
        if self.host is None or not self.host.children:
            self.lab.remove_live(self.base + '/live.json')
        return record

    def longest_ms(self):
        return (self.cost['command_seconds']
                + (self.cost['command_seconds'] + self.cost['driver_seconds'] + 3 * self.cost['stop_seconds'] if self.exp['driver'] is not None else 0)
                + (self.cost['command_seconds'] + self.cost['stop_seconds'] if self.exp.get('meter') is not None else 0)
                + (self.cost['stop_seconds'] if 'rehearsal' in self.spec and self.exp['driver'] is not None else 0)
                + len(self.exp['processes']) * (self.cost['ready_seconds'] + 2 * self.cost['stop_seconds'])
                + sum(len(self.exp.get(g, [])) for g in ('actions', 'collect', 'checks', 'release'))
                * (self.cost['command_seconds'] + self.cost['stop_seconds'])) * 1000

    def run(self, admitted=None):
        parent, name = self.lab._parent(self.base, create=True)
        try:
            os.mkdir(name, 0o700, dir_fd=parent)
        finally:
            os.close(parent)
        if 'rehearsal' in self.spec:
            self.lab.write_once(self.base + '/key.json', canonical.record_bytes(self.key))
        # The marker precedes all native provisioning, including controller failures.
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(dict(format='lab-live/1', unit=self.spec['unit'])))
        wake_r, wake_w = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
        handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
        def interrupted(sig, _frame):
            self.interrupt = sig
        for sig in handlers:
            signal.signal(sig, interrupted)
        old_wakeup = signal.set_wakeup_fd(wake_w)
        if self.spec.get('brake') is not None:
            self.timeline('brake', line=self.spec['brake']['line'], duration_ms=self.spec['brake']['ms'])
        self.feed('begin')
        try:
            if admitted is not None:
                admitted()
            while True:
                try:
                    if self.interrupt is not None and self.state.data['interrupted'] is None:
                        if self.state.data['phase'] not in ('release', 'stop'):
                            self.effects[:] = [r for r in self.effects if r['op'] in FINISH_EFFECTS]
                        self.timeline('interrupted', signal=signal.Signals(self.interrupt).name)
                        self.feed('interrupt', signal=self.interrupt, detail=signal.Signals(self.interrupt).name)
                    self.observe()
                    if self.state.data['meter'] is not None:
                        self.feed('meter_due')
                    if self.driver is not None:
                        self.driver.poll()
                    if self.effects:
                        request = self.effects.pop(0)
                        # Observation can invalidate a queued spawn.
                        self.effect(request)
                        continue
                    if self.finishing and (self.host is None or not self.host.children):
                        return self.finish()
                    if self.state.data['phase'] == 'stop' and (self.host is None or not self.host.children):
                        self.feed('closed')
                        continue
                    deadline = self.state.data['deadline']
                    deadlines = [x[field] for x in self.items.values() for field in ('cleanup', 'kill_deadline')
                                 if x[field] is not None]
                    if deadline is not None:
                        deadlines.append(deadline)
                    active_meter = self.state.data['meter']
                    if active_meter is not None and self.state.data['phase'] == 'drive':
                        fields = ('deadline',) if active_meter['active'] else ('due',)
                        deadlines.extend(active_meter[field] for field in fields if active_meter[field] is not None)
                    if self.driver is not None:
                        deadlines.extend(self.driver.command_deadlines.values())
                    if self.driver is not None and self.driver.stop_deadline is not None:
                        deadlines.append(self.driver.stop_deadline)
                    timeout = max(0, min(deadlines) - self.ms()) if deadlines else None
                    if self.driver is not None:
                        transport.driver_poll(self.host, timeout, wake_r, self.driver.descriptors())
                    else:
                        self.host.poll(timeout, wake_r)
                    try:
                        os.read(wake_r, 4096)
                    except BlockingIOError:
                        pass
                    self.observe()
                    self.feed('tick')
                except Exception as error:
                    if self.publishing:
                        raise
                    self.timeline('machinery_error', detail=str(error))
                    self.effects[:] = [r for r in self.effects if r['op'] in FINISH_EFFECTS]
                    phase = self.state.data['phase']
                    what = 'containment/provision' if isinstance(error, CapabilityError) and phase == 'provision' else 'runner'
                    if self.driver is not None and self.driver.provisioning:
                        self.feed('provision_error', what='provision/host', detail=str(error))
                    else:
                        self.feed('error', what=what, detail=str(error))
                    if phase in ('stop', 'stopped-collect', 'stopped-check', 'judge', 'closed'):
                        # A broken host cannot promise more cleanup. systemd owns the unit fallback.
                        pending, self.effects = self.effects, []
                        for request in pending:
                            if request['op'] in FINISH_EFFECTS:
                                try:
                                    self.effect(request)
                                except Exception as cleanup_error:
                                    self.errors.append(dict(what='containment', detail=str(cleanup_error)))
                        for item in self.items.values():
                            if item['group'] == 'checks' and item['record'].get('cluster') == 'stopped':
                                self.undecided.append(dict(check='checks/' + item['name'],
                                                           reason='check cut by machinery error'))
                        return self.finish()
        finally:
            self.drop_bootstrap()
            if self.driver is not None:
                self.driver.close_channels()
            signal.set_wakeup_fd(old_wakeup)
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
            os.close(wake_r)
            os.close(wake_w)
