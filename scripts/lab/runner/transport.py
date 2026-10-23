"""Private unit bootstrap and lock-descriptor transport; no public worker command."""
import array
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import secrets
import select
import signal
import socket
import subprocess
import sys
import time
from . import canonical, evidence, store
from .host import CapabilityError, await_removed, manager, manager_environment, namespace_probe
from .records import Refused


def _receive(connection, size):
    data = b''
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            raise RuntimeError('unit transport ended early')
        data += chunk
    return data


def _admission(connection):
    marker = _receive(connection, 1)
    if marker == b'R':
        detail = _receive(connection, int.from_bytes(_receive(connection, 8), 'big')).decode('utf-8', 'strict')
        raise Refused('provision', detail)
    if marker != b'A':
        raise CapabilityError('unit did not acknowledge admission')


def _runtime_known(lab, key):
    """Recheck requested host inputs before provisioning and supply their paths."""
    from .cli import _browser_facts, _experiment_input, _program_facts, _runner_root
    from . import experiment
    known = {'runner': str(_runner_root())}
    if any('version' in p for p in key['lab']['programs']):
        settings = canonical.loads(lab.read('settings.json'))
        exp, files = _experiment_input(lab.root / 'store/experiments' / key['experiment'])
        texts = {}
        for file in files:
            try:
                texts[file['path']] = file['data'].decode('utf-8', 'strict')
            except UnicodeError:
                texts[file['path']] = None
        used = experiment.uses_programs(exp, texts)
        paths, facts, problems = _program_facts(settings, used)
        if problems:
            raise Refused('settings.programs', '; '.join(str(p) for p in problems))
        for name in sorted(used):
            member = 'settings.programs.' + name
            programs = settings.get('programs')
            configured = programs.get(name) if type(programs) is dict else None
            current = facts.get(paths.get('program.' + name))
            if (type(configured) is not dict or current is None
                    or any(current[k] != configured[k] for k in ('path', 'sha256'))
                    or configured not in key['lab']['programs']):
                raise Refused(member, 'program changed before provision')
        known.update(paths)
    if 'browser' not in key['lab']:
        return known
    try:
        settings = canonical.loads(lab.read('settings.json'))
        browser = settings.get('browser')
        paths, facts, problems = _browser_facts(browser)
        if problems:
            raise RuntimeError('; '.join(str(p) for p in problems))
        expected = key['lab']['browser']
        if (browser['chromium'] != expected['chromium']
                or any(facts['chromium'][k] != expected['chromium'][k] for k in ('path', 'sha256'))):
            raise RuntimeError('settings.browser.chromium')
        if facts['fonts'] != expected['fonts']:
            raise RuntimeError('settings.browser.fonts')
        if not any(all(program[k] == facts['program'][k] for k in ('path', 'sha256'))
                   for program in key['lab']['programs']):
            raise RuntimeError('settings.browser.program')
        known.update(paths)
    except (OSError, Refused, RuntimeError) as error:
        raise RuntimeError('browser changed before provision: ' + str(error)) from error
    return known


def worker(endpoint, token):
    """Only the transient unit calls this package entry point."""
    import fcntl
    from .loop import Run
    os.umask(0o077)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.connect(endpoint)
        connection.sendall(token.encode('ascii'))
        marker, ancillary, flags, _address = connection.recvmsg(1, socket.CMSG_SPACE(array.array('i').itemsize))
        descriptors = array.array('i')
        for level, kind, data in ancillary:
            if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                descriptors.frombytes(data[:len(data) - len(data) % descriptors.itemsize])
        if marker != b'L' or flags & socket.MSG_CTRUNC or len(descriptors) != 1:
            for fd in descriptors:
                os.close(fd)
            raise RuntimeError('missing exclusive lab lock descriptor')
        lock_fd = descriptors[0]
        os.set_inheritable(lock_fd, False)
        try:
            spec = json.loads(_receive(connection, int.from_bytes(_receive(connection, 8), 'big')))
            with store.open_lab(spec['root']) as lab:
                info = os.stat('lock', dir_fd=lab.fd, follow_symlinks=False)
                held = os.fstat(lock_fd)
                if (info.st_dev, info.st_ino) != (held.st_dev, held.st_ino):
                    raise RuntimeError('transport did not pass this lab lock')
                fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                key = canonical.loads(lab.read(spec['key_path'] + '/key.json'))
                from .cli import _program, _runner_digest, _subject
                if canonical.digest(key) != spec['key'] or key['lab']['runner'] != _runner_digest():
                    raise RuntimeError('runner or key changed before provision')
                _subject(lab, key['subject'])
                try:
                    known = _runtime_known(lab, key)
                except Refused as error:
                    detail = str(error).encode('utf-8')
                    connection.sendall(b'R' + len(detail).to_bytes(8, 'big') + detail)
                    return 2
                for program in key['lab']['programs']:
                    current = _program(program['path'])
                    if any(current[k] != program[k] for k in ('path', 'sha256')):
                        raise RuntimeError('program changed before provision: ' + program['path'])
                exp_path = 'store/experiments/' + key['experiment']
                if canonical.tree_digest(store.walk(lab.root / exp_path)) != key['experiment']:
                    raise RuntimeError('experiment changed before provision')
                run = Run(lab, spec)
                run.state.data['known'].update(known)
                record = run.run(admitted=lambda: connection.sendall(b'A'))
                interrupted = next((e['detail'] for e in record['errors'] if e['what'] == 'interrupted'), None)
                if interrupted is not None:
                    return 128 + int(getattr(signal, interrupted))
                return {'pass': 0, 'fail': 1, 'incomplete': 3, 'error': 4}[record['outcome']]
        finally:
            os.close(lock_fd)


def launch(lab, lock_fd, key_path, seed, campaign):
    """Start one unit and keep the same flock alive in both outer and worker."""
    key = canonical.loads(lab.read(key_path + '/key.json'))
    from .cli import _runtime_cost
    cost = _runtime_cost(lab, key)
    _runtime_known(lab, key)
    programs = {Path(p['path']).name: p['path'] for p in key['lab']['programs']}
    systemctl, systemd_run = programs.get('systemctl'), programs.get('systemd-run')
    bwrap = programs.get('bwrap')
    if not systemctl or not systemd_run or not bwrap:
        raise Refused('containment', 'systemd-run, systemctl and bwrap identities are required')
    try:
        manager_version = manager(systemctl, ['show', '--property=Version', '--value'], cost['command_seconds'])
        namespace_probe(bwrap, os.path.realpath(sys.executable), cost['command_seconds'])
    except (OSError, subprocess.SubprocessError, CapabilityError) as error:
        raise Refused('containment', str(error)) from error
    previous = evidence.read(lab, key_path)
    now = datetime.now(timezone.utc)
    run_id = now.strftime('%Y%m%dT%H%M%S') + f'{now.microsecond // 1000:03d}Z-' + secrets.token_hex(3)
    unit = 'lab-' + run_id + '.service'
    os.ftruncate(lock_fd, 0)
    os.pwrite(lock_fd, (unit + '\n').encode('ascii'), 0)
    os.fsync(lock_fd)
    spec = dict(root=str(lab.root), key_path=key_path, key=key_path.rsplit('/', 1)[-1],
                run=run_id, unit=unit, seed=seed, campaign=campaign,
                attempt=1 + sum(e['seed'] == seed for e in previous), systemctl=systemctl,
                manager_version=manager_version, bounds=cost)
    token = secrets.token_hex(16)
    endpoint = str(lab.root / ('.transport-' + secrets.token_hex(6)))
    runner_root = str(Path(__file__).resolve().parent.parent)
    code = 'import sys; sys.path.insert(0,sys.argv[1]); from runner.transport import worker; sys.exit(worker(sys.argv[2],sys.argv[3]))'
    command = [systemd_run, '--user', '--no-ask-password', '--quiet', '--wait', '--pipe', '--collect',
               '--expand-environment=no', '--service-type=exec', '--unit=' + unit,
               '--property=Delegate=yes', '--property=KillMode=control-group',
               '--property=TimeoutStopSec=' + str(cost['stop_seconds']),
               '--property=TimeoutStartSec=' + str(cost['ready_seconds']),
               os.path.realpath(sys.executable), '-I', '-B', '-S', '-c', code, runner_root, endpoint, token]
    saved = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
    received, sent = [], 0
    def relay(sig, _frame):
        received.append(sig)
    process = None
    process_fd = None
    accepted = False
    transport_dir = 'transport/' + run_id
    # Launch diagnostics survive a refused bootstrap. They are never a run or evidence line.
    out = lab.create_file(transport_dir + '/stdout')
    err = lab.create_file(transport_dir + '/stderr')
    wake_r, wake_w = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
    old_wakeup = signal.set_wakeup_fd(wake_w)
    for sig in saved:
        signal.signal(sig, relay)
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(endpoint)
            listener.listen(1)
            listener.setblocking(False)
            process = subprocess.Popen(command, env=manager_environment(), stdin=subprocess.DEVNULL,
                                       stdout=out, stderr=err)
            process_fd = os.pidfd_open(process.pid)
            deadline = time.monotonic() + cost['ready_seconds']
            connection = None
            while process.poll() is None:
                if sent < len(received):
                    # Send to the unit's main, allowing its life to stop in reverse order.
                    if accepted:
                        for sig in received[sent:]:
                            manager(systemctl, ['kill', '--kill-whom=main', '--signal=' + signal.Signals(sig).name, unit], cost['command_seconds'])
                        sent = len(received)
                if not accepted:
                    left = deadline - time.monotonic()
                    if left <= 0:
                        raise CapabilityError('unit bootstrap exceeded ready_seconds')
                    ready, _, _ = select.select([listener, wake_r, process_fd], [], [], left)
                    if wake_r in ready:
                        os.read(wake_r, 4096)
                    if listener in ready:
                        connection, _ = listener.accept()
                        with connection:
                            remaining = deadline - time.monotonic()
                            if remaining <= 0:
                                raise CapabilityError('unit bootstrap exceeded ready_seconds')
                            connection.settimeout(remaining)
                            if _receive(connection, len(token)) != token.encode('ascii'):
                                raise CapabilityError('unit transport token differs')
                            connection.sendmsg([b'L'], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', [lock_fd]))])
                            data = canonical.dumps(spec)
                            connection.sendall(len(data).to_bytes(8, 'big') + data)
                            _admission(connection)
                            accepted = True
                else:
                    # pidfd completion and the wakeup pipe avoid polling or wall sleeps.
                    ready, _, _ = select.select([process_fd, wake_r], [], [])
                    if wake_r in ready:
                        os.read(wake_r, 4096)
            status = process.wait()
        if not accepted:
            diagnostic = lab.read(transport_dir + '/stderr').decode('utf-8', 'replace').strip()
            raise Refused('containment', diagnostic or f'unit bootstrap exited {status}')
        base = key_path + '/runs/' + run_id
        try:
            events = lab.read(base + '/events.jsonl')
        except FileNotFoundError:
            events = b''
        for line in events.splitlines(keepends=True):
            event = canonical.loads(line)
            if event['event'] == 'containment':
                await_removed(event['cgroup'], cost['stop_seconds'])
        path = base + '/run.json'
        try:
            record = canonical.loads(lab.read(path))
        except FileNotFoundError as error:
            raise RuntimeError('unit ended without run.json; retained live events require owner recovery') from error
        expected = {'pass': 0, 'fail': 1, 'incomplete': 3, 'error': 4}[record['outcome']]
        interrupted = next((e['detail'] for e in record['errors'] if e['what'] == 'interrupted'), None)
        if interrupted is not None:
            expected = 128 + int(getattr(signal, interrupted))
        if status != expected:
            raise RuntimeError(f'unit status {status} differs from recorded outcome {record["outcome"]}')
        if received:
            return record, 128 + received[0]
        return record, expected
    except BaseException as error:
        if process is not None and process.poll() is None:
            try:
                manager(systemctl, ['kill', '--kill-whom=all', '--signal=SIGKILL', unit], cost['command_seconds'])
                process.wait(timeout=cost['stop_seconds'])
            except (OSError, subprocess.SubprocessError, CapabilityError):
                # KillMode=control-group remains the unit's ownership fallback.
                pass
        if not accepted and isinstance(error, (OSError, CapabilityError, subprocess.SubprocessError)):
            raise Refused('containment', str(error)) from error
        raise
    finally:
        if process_fd is not None:
            os.close(process_fd)
        signal.set_wakeup_fd(old_wakeup)
        for sig, handler in saved.items():
            signal.signal(sig, handler)
        for fd in (out, err, wake_r, wake_w):
            os.close(fd)
        try:
            os.unlink(endpoint)
        except FileNotFoundError:
            pass
