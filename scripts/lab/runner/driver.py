"""The host's private stdio and tool broker, carried by the run's native loop."""
import json
import os
from pathlib import Path
import select
import signal
import socket
import stat
import sys
from . import canonical, codex, experiment, probe, store, transport, views, script
from .records import Refused


def _send(connection, value):
    connection['out'] += canonical.record_bytes(value)


def _write_config(home, data):
    """Only config.toml is opened; credentials and other host state stay with Codex."""
    parent = store.checked_directory(home.parent)
    try:
        try:
            os.mkdir(home.name, 0o700, dir_fd=parent)
        except FileExistsError:
            pass
        fd = os.open(home.name, store.DIR_FLAGS, dir_fd=parent)
    finally:
        os.close(parent)
    try:
        info = os.fstat(fd)
        if info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise Refused('settings.codex_home', 'expected a private owned directory')
        try:
            info = os.stat('config.toml', dir_fd=fd, follow_symlinks=False)
        except FileNotFoundError:
            pass
        else:
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise Refused('settings.codex_home', 'config must be a private owned regular file')
        temporary = '.lab-config-' + str(os.getpid())
        out = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=fd)
        try:
            with os.fdopen(out, 'wb') as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            os.rename(temporary, 'config.toml', src_dir_fd=fd, dst_dir_fd=fd)
            os.fsync(fd)
        finally:
            try:
                os.unlink(temporary, dir_fd=fd)
            except FileNotFoundError:
                pass
    finally:
        os.close(fd)


class Driver:
    def __init__(self, run, *, provisioning=False):
        self.run = run
        self.provisioning = provisioning
        self.rehearsal = not provisioning and 'rehearsal' in run.spec
        self.endpoint_base = None
        self.base = run.base + ('/driver/provision' if provisioning else '/driver')
        self.identity = 'provision-host' if provisioning else 'driver'
        self.endpoint = None
        self.address = None
        self.provision_record = None
        self.host_end = None
        self.endpoint_end = None
        self.started_ms = None
        self.listener = None
        self.connections = []
        self.input = None
        self.stop_deadline = None
        self.stopped = False
        self.closed = False
        self.invocation = None
        self.protocol = None
        self.calls = {}
        self.files = {}
        self.expected = None
        self.events_fd = None
        self.protocol_failed = False
        self.probe = None
        self.command_deadlines = {}

    @property
    def state(self):
        return self.run.state.data['tool_provision' if self.provisioning else 'driver']

    def feed(self, kind, **fields):
        if self.provisioning:
            kind = {'driver_event': 'provision_event', 'driver_inventory': 'provision_inventory',
                    'driver_call': 'provision_call'}[kind]
        self.run.feed(kind, **fields)

    def start_endpoint(self):
        run, lab = self.run, self.run.lab
        self.endpoint = 'provision-endpoint' if self.provisioning else 'rehearsal-endpoint'
        self.endpoint_base = self.base if self.provisioning else self.base + '/rehearsal'
        self.started_ms = run.ms()
        for path in ('home', 'calls'):
            os.close(lab.directory(self.endpoint_base + '/' + path, create=True))
        value = script.provisioning(run.exp['driver']['tools']) if self.provisioning else run.spec['rehearsal']['value']
        lab.write_once(self.endpoint_base + '/script.json', canonical.record_bytes(value))
        root = lab.root / self.endpoint_base
        argv = [os.path.realpath(sys.executable), '-I', '-B', '-S', str(Path(__file__).with_name('launch.py')),
                str(lab.root), 'endpoint', run.spec['run'], str(root / 'script.json'), str(root)]
        run.spawn_driver(argv, dict(PATH=os.defpath, LANG='C.UTF-8'), None, str(root),
                         identity=self.endpoint, base=self.endpoint_base, group='endpoint')
        if self.rehearsal:
            run.feed('driver_spawned')

    def endpoint_line(self, stream, line):
        if stream != 'stdout':
            return
        if self.address is not None or not line.startswith(b'listening 127.0.0.1:'):
            raise RuntimeError('invalid endpoint readiness')
        self.address = line.decode().removeprefix('listening ')
        if not self.state['ending']:
            data = self.run.lab.read(self.run.base + '/driver/config.toml').decode()
            self.run.lab.write_once(self.endpoint_base + '/config.toml', codex.with_endpoint(data, self.address).encode())
            self.start_host()

    def stop_endpoint(self):
        if (self.endpoint in self.run.host.children and not self.run.host.children[self.endpoint]['reaped']
                and not self.run.items[self.endpoint]['cancelled']):
            self.run.host.signal_first(self.endpoint, signal.SIGTERM)
            self.run.items[self.endpoint]['cancelled'] = True
            self.run.items[self.endpoint]['kill_deadline'] = self.run.ms() + self.run.cost['stop_seconds'] * 1000

    def prepare(self):
        run, lab = self.run, self.run.lab
        known = run.state.data['known']
        root = lab.root / self.base
        for path in ('work', 'home', 'calls'):
            os.close(lab.directory(self.base + '/' + path, create=True))
        settings = canonical.loads(lab.read('settings.json'))
        home = lab.root / self.base / 'rehearsal/home' if self.rehearsal else Path(settings.get('codex_home', str(lab.root / 'codex')))
        config = run.key['lab']['driver']['config']
        names = {**known, 'driver.work': str(root / 'work'), 'codex.home': str(home),
                 'lab.root': str(lab.root)}
        toml = config['toml']
        for name, value in names.items():
            toml = toml.replace('{' + name + '}', json.dumps(value, ensure_ascii=False)[1:-1])
        subject = canonical.loads(lab.read('store/subjects/' + run.key['subject'] + '/subject.json'))
        source = known['source'] if subject['source'] is not None else None
        profile = views.profile_toml(run.key['lab']['driver']['program']['path'], source, str(root / 'work'))
        if profile not in toml:
            raise RuntimeError('host and probe permission profiles differ')
        try:
            lab.read('canary')
        except FileNotFoundError:
            lab.write_once('canary', b'lab boundary canary\n')
        try:
            self.probe = probe.boundary_probe(run.key['lab']['driver']['program']['path'], root,
                source=source, work=str(root / 'work'), canary=str(lab.root / 'canary'),
                home=str(Path.home()), seconds=run.cost['command_seconds'], profile=profile)
        except probe.BoundaryError as error:
            self.probe = error.results
            raise
        finally:
            if self.probe is not None:
                lab.write_once(self.base + '/probe.json', canonical.record_bytes(self.probe))
        lab.write_once(self.base + '/config.toml', toml.encode('utf-8'))

    def start(self):
        if self.provisioning or self.rehearsal:
            self.start_endpoint()
        else:
            self.start_host()

    def start_host(self):
        run, lab = self.run, self.run.lab
        known = run.state.data['known']
        root = lab.root / run.base / 'driver'
        settings = canonical.loads(lab.read('settings.json'))
        home = lab.root / self.endpoint_base / 'home' if self.endpoint_base is not None else Path(settings.get('codex_home', str(lab.root / 'codex')))
        config = run.key['lab']['driver']['config']
        _write_config(home, lab.read((self.endpoint_base or self.base) + '/config.toml'))
        tools = []
        for definition in run.exp['driver']['tools']:
            item = life_render(run, definition)
            tools.append(dict(name=definition['name'], argv=item['argv'], env=item['env'],
                **{field: [experiment.render(v, known) for v in definition[field]] for field in ('writes', 'reads')}))
        subject = canonical.loads(lab.read('store/subjects/' + run.key['subject'] + '/subject.json'))
        lab.write_once(self.base + '/tools.json', canonical.record_bytes(dict(format='lab-driver-tools/1',
            run=run.spec['run'], work=str(root / 'work'), source=known['source'] if subject['source'] is not None else None,
            socket=str(lab.root / self.base / 'broker.sock'), tools=tools)))
        for path in ('events.jsonl', 'calls.jsonl', 'transcript.jsonl'):
            opened = lab.create_file(self.base + '/' + path)
            if path == 'events.jsonl':
                self.events_fd = opened
            else:
                os.close(opened)
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        # Absolute run paths exceed sockaddr_un. Bind through the opened parent directory.
        directory = lab.directory(self.base)
        try:
            self.listener.bind(f'/proc/self/fd/{directory}/broker.sock')
        finally:
            os.close(directory)
        self.listener.listen()
        self.listener.setblocking(False)
        self.expected = codex.expected_inventory([t['name'] for t in tools], str(home / 'config.toml'))
        prompt = (Path(__file__).resolve().parents[1] / 'preamble/driver.md').read_text()
        prompt += '\n\n' + lab.read('store/experiments/' + run.key['experiment'] + '/' + run.exp['driver']['charter']).decode('utf-8', 'strict')
        schema = config['schema']
        self.protocol = transport.AppServer(str(root / 'work'), prompt, schema, self.received, self.inventory)
        read, self.input = os.pipe2(os.O_CLOEXEC)
        os.set_blocking(self.input, False)
        argv = [run.key['lab']['driver']['program']['path'], *config['argv']]
        env = dict(PATH=os.defpath, LANG='C.UTF-8', HOME=str(root / 'home'), CODEX_HOME=str(home))
        self.invocation = dict(argv=argv, env=env, cwd=str(root / 'work'))
        try:
            run.spawn_driver(argv, env, read, str(root / 'work'), identity=self.identity, base=self.base)
        finally:
            os.close(read)
        if not self.provisioning and not self.rehearsal:
            run.feed('driver_spawned')

    def received(self, message):
        # Broker ends precede MCP results; retain them before a host completion can stop the drive.
        if message.get('method') == 'item/completed' and message.get('params', {}).get('item', {}).get('type') == 'mcpToolCall':
            self.poll()
        # Host events are evidence bytes, outside the key's canonical JSON subset.
        data = (json.dumps(dict(t_ms=self.run.ms(), **message), ensure_ascii=False, allow_nan=False) + '\n').encode('utf-8')
        while data:
            n = os.write(self.events_fd, data)
            if n == 0:
                raise OSError('driver event write made no progress')
            data = data[n:]
        if 'method' in message and 'id' not in message:
            self.feed('driver_event', message=message)

    def inventory(self, responses):
        observed = codex.observe(responses, self.expected)
        self.feed('driver_inventory', inventory=observed)

    def descriptors(self):
        result = []
        if self.listener is not None:
            result.append((self.listener.fileno(), select.POLLIN))
        if self.input is not None and self.protocol.outgoing and not self.state['ending']:
            result.append((self.input, select.POLLOUT))
        result.extend((c['socket'].fileno(), select.POLLIN | (select.POLLOUT if c['out'] else 0))
                      for c in self.connections)
        return result

    def can_go(self):
        state = self.run.state.data
        driver = self.state
        return (state['phase'] == 'drive' and driver is not None and driver['inventory'] is not None
                and not driver['ending'] and not any(v['state'] == 'mismatch' for v in driver['inventory'].values()))

    def poll(self):
        if self.protocol is None:
            return
        if self.input is not None and self.protocol.outgoing and not self.state['ending']:
            try:
                n = os.write(self.input, self.protocol.outgoing)
                self.protocol.outgoing = self.protocol.outgoing[n:]
            except BlockingIOError:
                pass
        if self.listener is not None:
            while True:
                try:
                    connection, _ = self.listener.accept()
                except BlockingIOError:
                    break
                connection.setblocking(False)
                self.connections.append(dict(socket=connection, buffer=b'', out=b'', pending=[], active=set()))
        for connection in list(self.connections):
            try:
                data = connection['socket'].recv(65536)
            except BlockingIOError:
                data = None
            if data == b'':
                if connection['active'] or connection['pending'] or connection['buffer']:
                    raise RuntimeError('tool broker disconnected during a call')
                connection['socket'].close()
                self.connections.remove(connection)
                continue
            if data:
                connection['buffer'] += data
                while b'\n' in connection['buffer']:
                    line, connection['buffer'] = connection['buffer'].split(b'\n', 1)
                    self.broker(connection, json.loads(line, object_pairs_hook=canonical._pairs))
            while connection['pending'] and self.can_go():
                self.admit(connection)
            if connection['out']:
                try:
                    n = connection['socket'].send(connection['out'])
                    connection['out'] = connection['out'][n:]
                except BlockingIOError:
                    pass
        if not self.provisioning and self.command_deadlines and self.run.ms() >= min(self.command_deadlines.values()):
            self.run.feed('driver_command_bound')
        if self.stop_deadline is not None and self.run.ms() >= self.stop_deadline:
            if self.identity in self.run.host.children:
                self.run.host.kill_tree(self.identity)
                self.run.items[self.identity]['kill_deadline'] = self.run.ms() + self.run.cost['stop_seconds'] * 1000
            self.stop_deadline = None
        if self.endpoint is not None and self.stopped and self.identity not in self.run.host.children:
            self.stop_endpoint()

    def broker(self, connection, message):
        if type(message) is not dict:
            raise RuntimeError('tool broker message is not an object')
        if set(message) == {'start'}:
            start = message['start']
            if (type(start) is not dict or set(start) != {'tool', 'args', 'stdin'}
                    or type(start['args']) is not list or any(type(a) is not str for a in start['args'])
                    or type(start['stdin']) is not bool
                    or start['tool'] not in [t['name'] for t in self.run.exp['driver']['tools']]):
                raise RuntimeError('invalid tool broker start')
            pending_ms = self.run.ms()
            if self.state['ending']:
                _send(connection, dict(refuse='driver ended'))
                return
            item = dict(type='mcpToolCall', server='lab', tool=start['tool'],
                        arguments=dict(args=start['args'], stdin='' if start['stdin'] else None))
            self.feed('driver_call', item=item)
            number = self.state['call_number']
            self.run.timeline('provision_call' if self.provisioning else 'driver_call', n=number, tool=start['tool'])
            if self.state['ending']:
                _send(connection, dict(refuse='driver bound passed'))
                return
            connection['pending'].append((start, pending_ms, number))
            if self.can_go():
                self.admit(connection)
        elif set(message) == {'end', 'status', 'signal'}:
            n = message['end']
            if (type(n) is not int or n not in connection['active']
                    or not (type(message['status']) is int and message['signal'] is None
                            or message['status'] is None and type(message['signal']) is str)):
                raise RuntimeError('invalid tool broker end')
            record = dict(event='end', n=n, t_ms=self.run.ms(), status=message['status'], signal=message['signal'])
            for stream in ('stdin', 'stdout', 'stderr'):
                if stream != 'stdin' or self.calls[n]['stdin']:
                    record[stream] = self.run.file_record(self.base[len(self.run.base) + 1:] + f'/calls/{n}.{stream}')
            self.run.lab.append_line(self.base + '/calls.jsonl', record)
            self.calls[n]['end'] = record
            connection['active'].remove(n)
            self.command_deadlines.pop(n, None)
        else:
            raise RuntimeError('invalid tool broker message')

    def admit(self, connection):
        start, pending_ms, n = connection['pending'].pop(0)
        self.run.feed('tick')
        if not self.provisioning and self.command_deadlines and self.run.ms() >= min(self.command_deadlines.values()):
            self.run.feed('driver_command_bound')
        if not self.can_go():
            _send(connection, dict(refuse='driver ended'))
            return
        connection['active'].add(n)
        self.command_deadlines[n] = self.run.ms() + self.run.cost['command_seconds'] * 1000
        self.calls[n] = dict(**start, end=None)
        self.run.lab.append_line(self.base + '/calls.jsonl', dict(event='start', n=n, tool=start['tool'],
            args=start['args'], stdin=dict(path=self.base[len(self.run.base) + 1:] + f'/calls/{n}.stdin') if start['stdin'] else None, t_ms=pending_ms))
        _send(connection, dict(go=n))

    def stop(self):
        if self.stopped:
            return
        self.stopped = True
        self.close_channels()
        if self.identity not in self.run.host.children:
            self.stop_endpoint()
            if self.provisioning and self.endpoint not in self.run.host.children:
                self.run.end_tools_provision(self)
        if self.identity in self.run.host.children and not self.run.host.children[self.identity]['reaped']:
            observed = self.state['observed']
            completed = any(t['completed_ms'] is not None for t in observed['turns'])
            if not completed or self.run.state.data['bounds'] or self.run.state.data['errors']:
                self.run.host.signal_first(self.identity, signal.SIGTERM)
                self.run.items[self.identity]['cancelled'] = True
            self.stop_deadline = self.run.ms() + self.run.cost['stop_seconds'] * 1000

    def close_channels(self):
        if self.input is not None:
            os.close(self.input)
            self.input = None
        if self.listener is not None:
            self.listener.close()
            self.listener = None
        for connection in self.connections:
            connection['socket'].close()
        self.connections.clear()
        self.command_deadlines.clear()
        try:
            (self.run.lab.root / self.base / 'broker.sock').unlink()
        except FileNotFoundError:
            pass

    def finish_provision(self):
        self.close_channels()
        if self.events_fd is not None:
            os.fsync(self.events_fd)
            os.close(self.events_fd)
            self.events_fd = None
        errors, offered, calls = [], set(), []
        root = self.run.lab.root / self.base
        for path in sorted((root / 'requests').glob('*.json')):
            if path.name.endswith('.http.json'):
                continue
            try:
                offered.update(script.offered(json.loads(path.read_bytes())))
            except (ValueError, TypeError, AttributeError) as error:
                errors.append(dict(what='provision/host', detail='invalid endpoint request: ' + str(error)))
        for name in script.admitted(sorted(offered), self.run.exp['driver']['tools']):
            errors.append(dict(what='provision/offered/' + name, detail='unadmitted offered tool'))
        for tool in self.run.exp['driver']['tools']:
            found = [c for c in self.calls.values() if c['tool'] == tool['name']
                     and c['args'] == tool['provision']['args'] and not c['stdin']]
            if not found:
                errors.append(dict(what='provision/' + tool['name'] + '/missing', detail='provisioning call never reached the broker'))
                continue
            for call in found:
                end = call['end']
                if end is None:
                    continue
                item = dict(tool=tool['name'], args=call['args'],
                            files={k: v for k, v in end.items() if k in ('stdin', 'stdout', 'stderr')})
                field = 'signal' if end['signal'] is not None else 'status'
                item[field] = end[field]
                calls.append(item)
                if field == 'signal' or end['status'] != tool['provision']['status']:
                    suffix = '/signal:' + end['signal'] if field == 'signal' else '/exit:' + str(end['status'])
                    errors.append(dict(what='provision/' + tool['name'] + suffix, detail='provisioning call ended otherwise'))
        observed = self.state['observed']
        completed = any(t['completed_ms'] is not None for t in observed['turns'])
        if not completed or self.host_end is None or self.host_end.get('status') != 0 or self.protocol_failed:
            errors.append(dict(what='provision/host', detail='host failed or ended without completing the turn'))
        inventory = self.state['inventory']
        if inventory is None and self.expected is not None:
            inventory = codex.observe(self.protocol.responses if self.protocol is not None else {}, self.expected)
        record = dict(invocation=self.invocation, inventory=inventory, offered=sorted(offered), calls=calls,
                      started_ms=self.started_ms, ended_ms=self.run.ms())
        self.closed = True
        return record, errors

    def finish(self):
        self.close_channels()
        if self.events_fd is not None:
            os.fsync(self.events_fd)
            os.close(self.events_fd)
            self.events_fd = None
        self.closed = True
        socket_path = self.run.lab.root / self.base / 'broker.sock'
        try:
            socket_path.unlink()
        except FileNotFoundError:
            pass
        state = self.state
        if state is None:
            if self.probe is not None:
                self.files['probe.json'] = self.run.file_record('driver/probe.json')
            return dict(invocation=None, provision=self.provision_record, probe=self.probe, expected_inventory=None, inventory=None, actions=[],
                turns=[], end=None, started_ms=None, ended_ms=None, report_error=None, files=self.files), dict(elapsed_ms=None, seconds=None,
                actions=0, turns=0, tokens=dict(input=None, cached=None, output=None), input_tokens_overshoot=None)
        observed = state['observed']
        for action in observed['actions']:
            entry = dict(action)
            if action['n'] in self.calls:
                entry['broker'] = self.calls[action['n']]
            self.run.lab.append_line(self.base + '/transcript.jsonl', entry)
        report = observed['report']
        if report is not None:
            self.run.lab.write_once(self.base + '/report.json', canonical.record_bytes(report))
        for path in ('config.toml', 'tools.json', 'events.jsonl', 'calls.jsonl', 'transcript.jsonl', 'report.json', 'probe.json'):
            try:
                self.files[path] = self.run.file_record('driver/' + path)
            except FileNotFoundError:
                pass
        inventory = state['inventory']
        if inventory is None and self.expected is not None:
            inventory = codex.observe(self.protocol.responses if self.protocol is not None else {}, self.expected)
        record = dict(invocation=self.invocation, provision=self.provision_record, probe=self.probe, expected_inventory=self.expected, inventory=inventory,
            actions=observed['actions'], turns=observed['turns'], end=self.run.items.get('driver', {}).get('record', {}).get('exit'),
            started_ms=state['started_ms'], ended_ms=state['ended_ms'], report_error=observed['report_error'], files=self.files)
        elapsed = ((state['ended_ms'] if state['ended_ms'] is not None else self.run.ms()) - state['started_ms']
                   if state['started_ms'] is not None else None)
        usage = dict(elapsed_ms=elapsed, seconds=None if elapsed is None else (elapsed + 999) // 1000,
                     actions=len(observed['actions']), turns=len(observed['turns']), tokens=observed['tokens'],
                     input_tokens_overshoot=observed['overshoot'])
        return record, usage


def life_render(run, item):
    from .life import _render
    return _render(run.state.data, item)
