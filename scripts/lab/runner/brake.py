"""Admission effects before a run id exists, while the caller holds the lab lock."""
import os
import selectors
import signal
import subprocess
import time
from . import canonical, experiment


CAP = 64 * 1024


def kill_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run(argv, env, seconds):
    """Run a brake with bounded in-memory output and a deadline for its whole group."""
    started = time.monotonic()
    buffers = {'stdout': b'', 'stderr': b''}
    with subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, start_new_session=True) as process:
        try:
            with selectors.DefaultSelector() as poller:
                for name in buffers:
                    stream = getattr(process, name)
                    os.set_blocking(stream.fileno(), False)
                    poller.register(stream, selectors.EVENT_READ, name)
                while poller.get_map():
                    remaining = started + seconds - time.monotonic()
                    if remaining <= 0:
                        kill_group(process)
                        return None, buffers, True
                    for key, _events in poller.select(remaining):
                        data = os.read(key.fd, CAP)
                        if data:
                            buffers[key.data] = (buffers[key.data] + data)[-CAP:]
                        else:
                            poller.unregister(key.fileobj)
                try:
                    status = process.wait(timeout=max(0, started + seconds - time.monotonic()))
                except subprocess.TimeoutExpired:
                    kill_group(process)
                    return None, buffers, True
                return status, buffers, False
        finally:
            kill_group(process)
            process.wait()


def admit(lab, exp, cost, runner):
    """Return (admission event, refusal line); null experiments bypass pause and brake."""
    if exp['driver'] is None and exp.get('meter') is None:
        return None, None
    try:
        os.stat('pause', dir_fd=lab.fd, follow_symlinks=False)
    except FileNotFoundError:
        pass
    else:
        return None, 'pause'
    settings = canonical.loads(lab.read('settings.json'))
    command = settings.get('brake')
    if command is None:
        return None, None
    known = {'runner': runner}
    argv = [experiment.render(v, known) for v in command['argv']]
    env = {k: experiment.render(v, known) for k, v in command['env'].items()}
    started = time.monotonic_ns()
    try:
        status, output, timed_out = run(argv, env, cost['command_seconds'])
    except OSError as error:
        return None, 'exit ' + str(error)
    lines = output['stdout'].decode('utf-8', 'replace').splitlines()
    line = next((line for line in reversed(lines) if line.strip()), None)
    if status != 0 or timed_out:
        return None, line if line is not None else 'exit ' + ('timeout' if timed_out else str(status))
    return dict(line=line, ms=(time.monotonic_ns() - started) // 1_000_000), None
