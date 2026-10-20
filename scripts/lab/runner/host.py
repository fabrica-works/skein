"""Native contained trees: delegated systemd cgroups and owned pidfds (lab.md, 5.2)."""
import os
from pathlib import Path
import select
import signal
import subprocess
from .records import relative


class CapabilityError(RuntimeError):
    pass


def manager_environment():
    # These locate the user's manager. No experiment inherits them.
    uid = os.getuid()
    return dict(PATH=os.defpath, LC_ALL='C', XDG_RUNTIME_DIR=f'/run/user/{uid}',
                DBUS_SESSION_BUS_ADDRESS=f'unix:path=/run/user/{uid}/bus')


def manager(program, args, seconds):
    result = subprocess.run([program, '--user', '--no-ask-password', '--no-pager', *args],
                            env=manager_environment(), stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=seconds, check=False)
    if result.returncode:
        raise CapabilityError(result.stderr.decode('utf-8', 'replace').strip() or f'manager exited {result.returncode}')
    return result.stdout.decode('utf-8', 'strict').strip()


class Host:
    def __init__(self, unit, systemctl, cost):
        self.unit, self.cost = unit, cost
        path = manager(systemctl, ['show', '--property=ControlGroup', '--value', unit], cost['command_seconds'])
        if not path.startswith('/') or not relative(path[1:]):
            raise CapabilityError('manager returned no contained unit cgroup')
        self.root = Path('/sys/fs/cgroup') / path[1:]
        self.children = {}
        self.runner = self.root / 'runner'
        self.runner.mkdir(mode=0o700)
        (self.runner / 'cgroup.procs').write_text('0')
        if not {'memory', 'pids'} <= set((self.root / 'cgroup.controllers').read_text().split()):
            raise CapabilityError('unit does not delegate memory and pids')
        (self.root / 'cgroup.subtree_control').write_text('+memory +pids')
        if not {'memory', 'pids'} <= set((self.root / 'cgroup.subtree_control').read_text().split()):
            raise CapabilityError('memory and pids controllers did not enable')
        if not (self.root / 'cgroup.kill').is_file() or not hasattr(os, 'pidfd_open') or not hasattr(signal, 'pidfd_send_signal'):
            raise CapabilityError('cgroup.kill and pidfds are required')

    def make_tree(self, identity):
        if identity in self.children or not relative(identity) or '/' in identity:
            raise RuntimeError('tree identity is not new')
        tree = self.root / identity
        tree.mkdir(mode=0o700)
        for name in ('memory.peak', 'pids.current', 'cgroup.kill'):
            if not (tree / name).is_file():
                raise CapabilityError('missing delegated tree interface: ' + name)
        return tree

    def spawn(self, identity, argv, env, stdin_fd, *, cwd):
        """The forked child enters its tree before executing any experiment bytes."""
        tree = self.make_tree(identity)
        group_fd = os.open(tree / 'cgroup.procs', os.O_WRONLY | os.O_CLOEXEC)
        out_r, out_w = os.pipe2(os.O_CLOEXEC)
        err_r, err_w = os.pipe2(os.O_CLOEXEC)
        ack_r, ack_w = os.pipe2(os.O_CLOEXEC)
        child = None
        try:
            child = os.fork()
            if child == 0:
                try:
                    signal.set_wakeup_fd(-1)
                    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGPIPE):
                        signal.signal(sig, signal.SIG_DFL)
                    os.write(group_fd, b'0')
                    os.dup2(stdin_fd, 0)
                    os.dup2(out_w, 1)
                    os.dup2(err_w, 2)
                    os.chdir(cwd)
                    # Keep only the close-on-exec error channel beside stdio.
                    os.closerange(3, ack_w)
                    os.closerange(ack_w + 1, os.sysconf('SC_OPEN_MAX'))
                    os.execve(argv[0], argv, env)
                except BaseException as error:
                    os.write(ack_w, str(error).encode('utf-8', 'replace'))
                    os._exit(127)
            for fd in (out_w, err_w, ack_w, group_fd):
                os.close(fd)
            pidfd = os.pidfd_open(child)
            os.set_blocking(ack_r, False)
            for fd in (out_r, err_r):
                os.set_blocking(fd, False)
            events = os.open(tree / 'cgroup.events', os.O_RDONLY | os.O_CLOEXEC)
            owned = dict(pid=child, pidfd=pidfd, tree=tree, events=events,
                         streams={'stdout': out_r, 'stderr': err_r}, exec_ack=ack_r, reaped=False, killed=False,
                         peak_bytes=None, source=None)
            self.children[identity] = owned
            return owned
        except BaseException:
            # A failed spawn cannot leave an unowned tree behind.
            if child:
                (tree / 'cgroup.kill').write_text('1')
                os.waitpid(child, 0)
            for fd in (out_r, out_w, err_r, err_w, ack_r, ack_w, group_fd):
                try:
                    os.close(fd)
                except OSError:
                    pass
            raise

    def exited(self, identity):
        child = self.children[identity]
        if child['reaped']:
            return False
        return os.waitid(os.P_PID, child['pid'], os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None

    def signal_first(self, identity, sig):
        child = self.children[identity]
        if child['reaped'] or self.exited(identity):
            return False
        # Only the status of an owned, unreaped child is read under /proc.
        try:
            with open(f"/proc/{child['pid']}/status", encoding='ascii') as stream:
                for line in stream:
                    if line.startswith('VmHWM:'):
                        fields = line.split()
                        if len(fields) == 3 and fields[2] == 'kB':
                            child.update(peak_bytes=int(fields[1]) * 1024, source='VmHWM')
        except (OSError, ValueError, UnicodeError):
            pass
        try:
            signal.pidfd_send_signal(child['pidfd'], sig)
        except ProcessLookupError:
            return False
        return True

    def reap(self, identity):
        child = self.children[identity]
        pid, status, usage = os.wait4(child['pid'], os.WNOHANG)
        if pid == 0:
            return None
        child['reaped'] = True
        if child['peak_bytes'] is None:
            child.update(peak_bytes=int(usage.ru_maxrss) * 1024, source='wait4')
        code = os.waitstatus_to_exitcode(status)
        if code >= 0:
            return dict(status=code)
        try:
            name = signal.Signals(-code).name
        except ValueError:
            name = 'SIG' + str(-code)
        return dict(signal=name)

    def counts(self, identity):
        child = self.children[identity]
        tree = child['tree']
        os.lseek(child['events'], 0, os.SEEK_SET)
        events = dict(line.split() for line in os.read(child['events'], 65536).decode('ascii').splitlines())
        return dict(populated=events['populated'] == '1',
                    tree_peak_bytes=int((tree / 'memory.peak').read_text().strip()),
                    pids=int((tree / 'pids.current').read_text().strip()))

    def kill_tree(self, identity):
        child = self.children[identity]
        child['killed'] = True
        (child['tree'] / 'cgroup.kill').write_text('1')

    def remove(self, identity):
        child = self.children[identity]
        if self.counts(identity)['populated']:
            raise RuntimeError('refusing to remove a populated tree')
        def remove_children(path):
            for entry in path.iterdir():
                if entry.is_dir():
                    remove_children(entry)
            path.rmdir()
        remove_children(child['tree'])
        descriptors = [child['pidfd'], child['events'], *child['streams'].values()]
        if child['exec_ack'] is not None:
            descriptors.append(child['exec_ack'])
        for fd in descriptors:
            os.close(fd)
        del self.children[identity]

    def poll(self, timeout_ms, wake_fd):
        poller = select.poll()
        poller.register(wake_fd, select.POLLIN)
        for child in self.children.values():
            if not child['reaped']:
                poller.register(child['pidfd'], select.POLLIN)
            if child['exec_ack'] is not None:
                poller.register(child['exec_ack'], select.POLLIN | select.POLLHUP)
            os.lseek(child['events'], 0, os.SEEK_SET)
            os.read(child['events'], 65536)
            poller.register(child['events'], select.POLLPRI | select.POLLERR)
            for fd in child['streams'].values():
                poller.register(fd, select.POLLIN | select.POLLHUP)
        return poller.poll(timeout_ms)


def namespace_probe(bwrap, python, seconds):
    """An inert, no-model namespace capability check, before run admission."""
    argv = [bwrap, '--unshare-user', '--unshare-pid', '--ro-bind', '/usr', '/usr']
    for path in ('/lib', '/lib64'):
        if Path(path).exists():
            argv.extend(['--ro-bind', path, path])
    argv.extend(['--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--',
                 python, '-I', '-B', '-S', '-c', 'pass'])
    result = subprocess.run(argv, env={'PATH': os.defpath, 'LC_ALL': 'C'},
                            stdin=subprocess.DEVNULL, capture_output=True, timeout=seconds, check=False)
    if result.returncode:
        raise CapabilityError('bubblewrap namespace probe: ' + result.stderr.decode('utf-8', 'replace').strip())
    return dict(argv=argv, status=result.returncode, stdout=result.stdout, stderr=result.stderr)


def await_removed(path, seconds):
    """Observe systemd's removal of this owned run cgroup after unit completion."""
    import time
    group = Path(path)
    if not str(group).startswith('/sys/fs/cgroup/') or '..' in group.parts:
        raise RuntimeError('invalid owned cgroup path')
    try:
        fd = os.open(group / 'cgroup.events', os.O_RDONLY | os.O_CLOEXEC)
    except FileNotFoundError:
        return
    deadline = time.monotonic_ns() + seconds * 1_000_000_000
    poller = select.poll()
    poller.register(fd, select.POLLPRI | select.POLLERR | select.POLLHUP)
    try:
        while group.exists():
            remaining = (deadline - time.monotonic_ns()) // 1_000_000
            if remaining <= 0:
                raise CapabilityError('owned run cgroup was not removed within stop_seconds: ' + str(group))
            os.lseek(fd, 0, os.SEEK_SET)
            try:
                os.read(fd, 65536)
            except OSError:
                if not group.exists():
                    return
                raise
            poller.poll(remaining)
    finally:
        os.close(fd)
