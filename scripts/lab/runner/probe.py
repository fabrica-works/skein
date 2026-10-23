"""No-model checks of the agent's command boundary (lab.md, 6.2)."""
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
from .views import profile_toml


CHECKS = ('canary_read', 'home_list', 'source_write', 'network', 'pid_namespace',
          'canary_process', 'work_write')

# Inputs arrive on stdin so the process token is absent from the probe's argv.
PROGRAM = r'''
import errno, json, os, socket, sys
c = json.load(sys.stdin)
r = {}
def denied(operation):
    try:
        operation()
    except OSError as error:
        return error.errno in (errno.EACCES, errno.EPERM, errno.ENOENT, errno.EROFS)
    return False
def read_canary():
    with open(c['canary'], 'rb') as stream:
        stream.read(1)
r['canary_read'] = denied(read_canary)
r['home_list'] = denied(lambda: os.listdir(c['home']))
def write_source():
    with open(c['source_file'], 'x') as stream:
        stream.write('probe')
    os.unlink(c['source_file'])
r['source_write'] = denied(write_source)
try:
    connection = socket.socket()
except PermissionError:
    r['network'] = True
else:
    try:
        connection.connect(('127.0.0.1', c['port']))
    except OSError:
        pass
    connection.close()
    r['network'] = False
r['pid_namespace'] = os.readlink('/proc/self/ns/pid') != c['namespace']
token = c['token'].encode()
r['canary_process'] = True
for name in os.listdir('/proc'):
    if name.isdecimal():
        try:
            with open('/proc/' + name + '/cmdline', 'rb') as stream:
                if token in stream.read().split(b'\0'):
                    r['canary_process'] = False
        except FileNotFoundError:
            pass
try:
    with open(c['work_file'], 'x') as stream:
        stream.write(c['token'])
    with open(c['work_file']) as stream:
        r['work_write'] = stream.read() == c['token']
    os.unlink(c['work_file'])
except OSError:
    r['work_write'] = False
print(json.dumps(r), flush=True)
'''


class BoundaryError(RuntimeError):
    def __init__(self, results):
        self.results = results
        super().__init__('boundary probe failed: ' + ', '.join(k for k, v in results.items() if not v['ok']))


def boundary_probe(codex_executable, run_directory, *, source, work, canary, home,
                   seconds, profile=None):
    """Return per-check evidence, or raise with that evidence before provisioning.

    The join calls this before Host.make_tree or any experiment process starts.
    All probe files belong to this run; its CODEX_HOME contains only config.toml.
    """
    previous_umask = os.umask(0o077)
    child = None
    try:
        root = Path(run_directory) / 'probe'
        root.mkdir(mode=0o700)
        codex_home = root / 'codex'
        codex_home.mkdir(mode=0o700)
        inert_home = root / 'home'
        inert_home.mkdir(mode=0o700)
        executable = os.path.realpath(codex_executable)
        text = profile_toml(executable, source, work) if profile is None else profile
        (codex_home / 'config.toml').write_text(text)
        # A missing fixture could make a read refusal vacuously succeed.
        if not Path(canary).is_file() or not Path(home).is_dir():
            raise ValueError('probe requires an existing canary and home directory')
        source_directory = Path(source) if source is not None else root / 'source'
        if source is None:
            source_directory.mkdir(mode=0o700)
        if not source_directory.is_dir() or not Path(work).is_dir():
            raise ValueError('probe requires existing source and work directories')
        token = 'lab-probe-' + secrets.token_hex(16)
        child = subprocess.Popen([os.path.realpath(sys.executable), '-I', '-B', '-S', '-c',
            'import sys; print("ready",flush=True); sys.stdin.buffer.read()', token],
            env={}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        # A readiness line, rather than elapsed time, establishes that the token is live.
        if child.stdout.readline() != b'ready\n':
            raise RuntimeError('probe canary failed to start')
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
            listener.bind(('127.0.0.1', 0))
            listener.listen(1)
            configuration = dict(canary=str(canary), home=str(home),
                source_file=str(source_directory / token), work_file=str(Path(work) / token),
                port=listener.getsockname()[1], namespace=os.readlink('/proc/self/ns/pid'), token=token)
            argv = [executable, 'sandbox', '-P', 'lab', '-C', work, '--',
                    '/usr/bin/python3', '-I', '-B', '-S', '-c', PROGRAM]
            try:
                result = subprocess.run(argv, input=json.dumps(configuration).encode(),
                    capture_output=True, timeout=seconds, check=False, cwd=work,
                    env=dict(PATH=os.defpath, HOME=str(inert_home), CODEX_HOME=str(codex_home), LC_ALL='C'))
                observed = json.loads(result.stdout) if result.returncode == 0 else {}
                if type(observed) is not dict:
                    observed = {}
                detail = result.stderr.decode('utf-8', 'replace')
            except (subprocess.TimeoutExpired, ValueError) as error:
                observed, detail = {}, str(error)
        results = {name: dict(ok=observed.get(name) is True, detail=detail) for name in CHECKS}
        if not all(value['ok'] for value in results.values()):
            raise BoundaryError(results)
        return results
    finally:
        if child is not None:
            child.stdin.close()
            child.wait(timeout=seconds)
            child.stdout.close()
        os.umask(previous_umask)
