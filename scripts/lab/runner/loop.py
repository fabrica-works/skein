"""One native event loop carrying out the pure life (lab.md, 10.4)."""
from datetime import datetime, timezone
import os
import platform
import signal
import time
from . import canonical, checks, evidence, judge, life, store
from .host import CapabilityError, Host


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
        self.base = spec['key_path'] + '/runs/' + spec['run']
        self.key = canonical.loads(lab.read(spec['key_path'] + '/key.json'))
        exp_base = 'store/experiments/' + self.key['experiment']
        self.exp = canonical.loads(lab.read(exp_base + '/experiment.json'))
        self.cost = self.key['lab']['cost']
        self.started = utc()
        self.origin = time.monotonic_ns()
        self.host = None
        self.records, self.command_records = {}, []
        self.effects = []
        self.finishing = False
        self.publishing = False
        self.failures, self.undecided, self.errors = [], [], []
        self.interrupt = None
        self.items = {}
        self.texts = {f['path']: f['data'].decode('utf-8', 'strict') for f in store.snapshot(lab.root / exp_base)
                      if any(f['path'] == x['from'] for p in self.exp['processes'] for x in p.get('files', []))}
        subject = canonical.loads(lab.read('store/subjects/' + self.key['subject'] + '/subject.json'))
        absolute = lambda path: str(lab.root / path)
        known = {'experiment': absolute(exp_base), 'source': absolute('store/subjects/' + self.key['subject'] + '/source'),
                 'run.id': spec['run'], 'run.seed': spec['seed']}
        known.update({'artifact.' + n: absolute('store/subjects/' + self.key['subject'] + '/artifacts/' + n) for n in subject['artifacts']})
        known.update({'dir.' + n: absolute(self.base + '/dirs/' + n) for n in self.exp['dirs']})
        known.update({'proc.' + p['name'] + '.dir': absolute(self.base + '/scratch/' + p['name']) for p in self.exp['processes']})
        known.update({'collected.' + c['name']: absolute(self.base + '/collected/' + c['name']) for c in self.exp['collect']})
        self.state = life.initial(self.exp, spec['seed'], self.cost, known, self.texts)

    def ms(self):
        return (time.monotonic_ns() - self.origin) // 1_000_000

    def timeline(self, event, **fields):
        self.lab.append_line(self.base + '/events.jsonl', dict(format='lab-event/1', event=event, ms=self.ms(), **fields))

    def feed(self, kind, **fields):
        event = dict(kind=kind, ms=self.ms(), **fields)
        self.state, effects = life.step(self.state, event)
        if kind == 'exit':
            self.effects[0:0] = effects
        else:
            self.effects.extend(effects)

    def provision(self):
        paths = ['scratch/' + p['name'] for p in self.exp['processes']]
        paths += ['dirs/' + n for n in self.exp['dirs']]
        paths += ['collected/' + c['name'] for c in self.exp['collect']]
        paths += ['commands', 'proc']
        for path in paths:
            os.close(self.lab.directory(self.base + '/' + path, create=True))
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(dict(format='lab-live/1', unit=self.spec['unit'])))
        try:
            self.host = self.host_factory(self.spec['unit'], self.spec['systemctl'], self.cost)
        except (OSError, CapabilityError) as error:
            raise CapabilityError('native provision refused: ' + str(error)) from error
        self.timeline('containment', cgroup=str(self.host.root), controllers=['memory', 'pids'])
        self.feed('provisioned')

    def spawn(self, effect):
        name, group = effect['name'], effect['group']
        phase = self.state.data['phase']
        expected = dict(processes='start', actions='seed', collect='collect', checks='check')[group]
        if phase != expected or self.state.data['current'] != name:
            return
        identity = ('proc-' if group == 'processes' else group + '-') + name
        prefix = 'proc/' + name + '/' if group == 'processes' else 'commands/' + group + '-' + name + '.'
        record = dict(name=name, group=group, argv=effect['argv'], env=effect['env'], started_ms=self.ms(),
                      ready_ms=None, captures={}, exit=None,
                      memory=dict(peak_bytes=None, source=None, bound_bytes=None, tree_peak_bytes=None), output={})
        if group == 'processes':
            self.records[name] = record
            definition = next(p for p in self.exp['processes'] if p['name'] == name)
            record['memory']['bound_bytes'] = definition['memory_bytes']
        else:
            self.command_records.append(record)
        cwd = self.state.data['known'].get('proc.' + name + '.dir', str(self.lab.root / self.base)) if group == 'processes' else str(self.lab.root / self.base)
        for file in effect['files']:
            target = self.base + '/scratch/' + name + '/' + file['path']
            fd = self.lab.create_file(target)
            try:
                _write(fd, file['text'].encode('utf-8', 'strict'))
                os.fsync(fd)
            finally:
                os.close(fd)
        stdin_fd = os.open(os.devnull, os.O_RDONLY | os.O_CLOEXEC)
        outputs = {}
        try:
            if effect['stdin'] is not None:
                path = prefix + 'stdin'
                self.lab.write_once(self.base + '/' + path, effect['stdin'].encode('utf-8', 'strict'))
                os.close(stdin_fd)
                stdin_fd = os.open(self.lab.root / self.base / path, store.FILE_FLAGS)
                record['stdin'] = self.file_record(path)
            for stream in ('stdout', 'stderr'):
                path = prefix + stream
                outputs[stream] = dict(path=path, fd=self.lab.create_file(self.base + '/' + path), buffer=b'')
            owned = self.host.spawn(identity, effect['argv'], effect['env'], stdin_fd, cwd=cwd)
        except BaseException:
            for output in outputs.values():
                os.close(output['fd'])
            raise
        finally:
            os.close(stdin_fd)
        self.items[identity] = dict(name=name, group=group, record=record, outputs=outputs,
                                    cleanup=None, kill_deadline=None, timed_out=False, cancelled=False,
                                    panic=[], panic_message=None)
        self.feed('spawned', name=name, group=group)
        self.timeline('start', name=name, group=group, argv=effect['argv'], env=effect['env'], pid=owned['pid'])

    def file_record(self, path):
        data = self.lab.read(self.base + '/' + path)
        return dict(path=path, bytes=len(data), sha256=store.file_digest(data))

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

    def cancel_command(self, name):
        for identity, item in self.items.items():
            if item['group'] != 'processes' and item['name'] == name and item['cleanup'] is None:
                item['cancelled'] = True
                item['timed_out'] = any(b['name'] == 'command_seconds' and b['who'] == name for b in self.state.data['bounds'])
                self.host.kill_tree(identity)
                item['kill_deadline'] = self.ms() + self.cost['stop_seconds'] * 1000

    def effect(self, request):
        op = request['op']
        if op == 'validate':
            self.feed('validated')
        elif op == 'provision':
            self.provision()
        elif op == 'phase':
            self.timeline('phase', phase=request['phase'])
        elif op == 'spawn':
            self.spawn(request)
        elif op == 'ready':
            record = self.records[request['name']]
            record.update(ready_ms=self.ms(), captures=request['captures'])
            self.timeline('ready', name=request['name'], captures=request['captures'])
        elif op == 'signal_first':
            if self.host.signal_first('proc-' + request['name'], signal.SIGTERM):
                self.feed('signalled', name=request['name'])
                self.timeline('signal', name=request['name'], signal='SIGTERM', target='first')
        elif op == 'kill_tree':
            identity = 'proc-' + request['name']
            self.host.kill_tree(identity)
            self.items[identity]['kill_deadline'] = self.ms() + self.cost['stop_seconds'] * 1000
            self.timeline('kill_tree', name=request['name'])
        elif op == 'reap_tree':
            self.begin_cleanup('proc-' + request['name'])
        elif op == 'cancel_command':
            self.cancel_command(request['name'])
        elif op == 'bound':
            self.failures.append(checks.failure(f"bound/{request['name']}/{request['who']}", 'bound', 'cost class bound passed'))
            self.timeline('bound', name=request['name'], who=request['who'])
        elif op == 'unavailable':
            self.undecided.append(dict(check=request['group'] + '/' + request['name'], reason=request['reason']))
            self.command_records.append(dict(group=request['group'], name=request['name'], skipped=request['reason']))
            self.timeline('unavailable', **{k: v for k, v in request.items() if k != 'op'})
        elif op == 'cut':
            self.timeline('cut', phase=request['phase'])
        elif op == 'judge':
            self.finishing = True

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
            os.fchmod(output['fd'], 0o400)
            os.close(output['fd'])
            output['fd'] = None
            item['record']['output'][stream] = self.file_record(output['path'])

    def output_line(self, item, stream, line, *, complete):
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
        self.failures.extend(item['panic'])
        if group != 'processes':
            result = record['exit']
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
                self.undecided.append(dict(check=name, reason='check cut by process stop or interruption'))
            if group == 'actions':
                self.failures.extend(checks.action(name, record))
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
        self.host.remove(identity)
        del self.items[identity]
        self.timeline('tree_empty', name=name, group=group)
        if group == 'processes':
            self.feed('tree_empty', name=name)
        else:
            self.feed('command_done', name=name, record=record)

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
        self.publishing = True
        self.freeze_collections()
        for name, record in self.records.items():
            panic = [f for f in self.failures if f['kind'] == 'panic' and f['signature'].startswith('panic/' + name + '/')]
            failures, unknown = checks.process(name, record, panic)
            self.failures.extend(f for f in failures if f not in self.failures)
            self.undecided.extend(unknown)
        self.errors.extend(self.state.data['errors'])
        outcome = judge.outcome(self.failures, self.errors, self.undecided, None, driver_required=False)
        self.failures.extend(checks.planted(self.exp['planted'], self.failures, outcome))
        outcome = judge.outcome(self.failures, self.errors, self.undecided, None, driver_required=False)
        for i, failure in enumerate(self.failures):
            failure['id'] = 'failure-' + str(i + 1)
        record = dict(format='lab-run/1', run=self.spec['run'], key=self.spec['key'], experiment=self.exp['name'],
                      seed=self.spec['seed'], attempt=self.spec['attempt'], campaign=self.spec['campaign'],
                      started=self.started, ended=utc(), outcome=outcome, schedule=self.state.data['schedule'],
                      processes=self.records, commands=self.command_records, driver=None,
                      failures=self.failures, undecided=self.undecided, errors=self.errors, bounds=self.state.data['bounds'],
                      usage=dict(elapsed_ms=self.ms(), longest_ms=self.longest_ms(), driver=None, triage=[]),
                      about=dict(hostname=platform.node(), kernel=platform.release(), processors=os.cpu_count(),
                                 unit=self.spec['unit'], manager=self.spec.get('manager_version'),
                                 memory_bytes=os.sysconf('SC_PHYS_PAGES') * os.sysconf('SC_PAGE_SIZE')))
        self.timeline('outcome', outcome=outcome)
        evidence.publish(self.lab, self.spec['key_path'], record)
        if self.host is None or not self.host.children:
            self.lab.remove_live(self.base + '/live.json')
        return record

    def longest_ms(self):
        return (self.cost['command_seconds']
                + len(self.exp['processes']) * (self.cost['ready_seconds'] + 2 * self.cost['stop_seconds'])
                + sum(len(self.exp[g]) for g in ('actions', 'collect', 'checks'))
                * (self.cost['command_seconds'] + self.cost['stop_seconds'])) * 1000

    def run(self, admitted=None):
        parent, name = self.lab._parent(self.base, create=True)
        try:
            os.mkdir(name, 0o700, dir_fd=parent)
        finally:
            os.close(parent)
        # The marker precedes all native provisioning, including controller failures.
        self.lab.write_once(self.base + '/live.json', canonical.record_bytes(dict(format='lab-live/1', unit=self.spec['unit'])))
        wake_r, wake_w = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
        handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
        def interrupted(sig, _frame):
            self.interrupt = sig
        for sig in handlers:
            signal.signal(sig, interrupted)
        old_wakeup = signal.set_wakeup_fd(wake_w)
        self.feed('begin')
        try:
            if admitted is not None:
                admitted()
            while True:
                try:
                    if self.interrupt is not None and self.state.data['interrupted'] is None:
                        self.effects.clear()
                        self.timeline('interrupted', signal=signal.Signals(self.interrupt).name)
                        self.feed('interrupt', signal=self.interrupt, detail=signal.Signals(self.interrupt).name)
                    self.observe()
                    if self.effects:
                        request = self.effects.pop(0)
                        # Observation can invalidate a queued spawn.
                        self.effect(request)
                        continue
                    if self.finishing and (self.host is None or not self.host.children):
                        return self.finish()
                    deadline = self.state.data['deadline']
                    deadlines = [x[field] for x in self.items.values() for field in ('cleanup', 'kill_deadline')
                                 if x[field] is not None]
                    if deadline is not None:
                        deadlines.append(deadline)
                    timeout = max(0, min(deadlines) - self.ms()) if deadlines else None
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
                    self.effects.clear()
                    if self.state.data['phase'] == 'stop':
                        # A broken host cannot promise more cleanup. systemd owns the unit fallback.
                        self.errors.append(dict(what='containment', detail=str(error)))
                        return self.finish()
                    what = 'containment/provision' if isinstance(error, CapabilityError) and self.state.data['phase'] == 'provision' else 'runner'
                    self.feed('error', what=what, detail=str(error))
        finally:
            signal.set_wakeup_fd(old_wakeup)
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
            os.close(wake_r)
            os.close(wake_w)
