#!/usr/bin/env python3
"""Durable admission and cgroup recovery for heavy; build-infrastructure.md."""
import contextlib
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import tomllib
import uuid

os.umask(0o077)
KIT = Path(__file__).resolve().parent
ROOT = Path(os.environ.get('DEV_ROOT', str(Path.home() / 'src/rust'))).resolve()
TMP = Path(os.environ.get('DEV_TMP', str(ROOT / 'tmp'))).resolve()
BASE = TMP / 'heavy'
LOGS = TMP / 'logs'
PUEUE = shutil.which('pueue') or 'pueue'
BOOT_FILE = Path(os.environ.get('HEAVY_BOOT_ID_FILE', '/proc/sys/kernel/random/boot_id'))
CONFIG = Path(os.environ.get('PUEUE_CONFIG_PATH', str(
    Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'pueue/pueue.yml'))).resolve()
PREFIX = 'skein-heavy-' + hashlib.sha256(str(BASE).encode()).hexdigest()[:12]
CGROOT = Path('/sys/fs/cgroup')


class Refusal(Exception):
    pass


def queue_identity():
    # Hash only endpoint-selection lines, without emitting config contents or
    # reading queue authentication files. A changed socket/data selection must
    # not silently reuse an existing host ledger.
    selected = []
    if CONFIG.exists():
        selected = [line.strip() for line in CONFIG.read_text().splitlines() if re.match(
            r'\s*(pueue_directory|runtime_directory|use_unix_socket|unix_socket_path|host|port):', line)]
    return hashlib.sha256(json.dumps([str(CONFIG), os.environ.get('PUEUE_PROFILE', ''), selected]).encode()).hexdigest()


def pueue_command(*args):
    profile = os.environ.get('PUEUE_PROFILE')
    return [PUEUE, *(['--profile', profile] if profile else []), *args]


QUEUE = queue_identity()


def run(args, **kw):
    return subprocess.run([str(x) for x in args], stdin=subprocess.DEVNULL,
                          capture_output=True, text=True, **kw)


def queue(*args):
    # Never request task records: those include environments and output.
    if args[0] in ('status', 'log', 'env'):
        raise Refusal('unsafe queue operation')
    result = run(pueue_command(*args))
    if result.returncode:
        raise Refusal('queue operation failed: ' + args[0])
    return result.stdout


def boot():
    if BOOT_FILE != Path('/proc/sys/kernel/random/boot_id') and (
            os.environ.get('HEAVY_RECOVERY_FIXTURE') != '1' or not BOOT_FILE.resolve().is_relative_to(TMP)):
        raise Refusal('boot identity override requires an isolated recovery fixture')
    return BOOT_FILE.read_text().strip()


def identity(pid=None):
    pid = pid or os.getpid()
    try:
        # The command name may contain spaces and parentheses.
        stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        if stat[0] == 'Z':
            return None
        ticks = stat[19]
        return {'pid': pid, 'ticks': ticks, 'boot': boot()}
    except FileNotFoundError:
        return None


def alive(owner):
    return owner is not None and identity(owner['pid']) == owner


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def atomic(path, value):
    storage_fault('snapshot' if path.name == 'state.json' else 'receipt')
    temporary = path.with_name(path.name + '.new')
    with temporary.open('w', encoding='utf-8') as out:
        os.fchmod(out.fileno(), 0o600)
        json.dump(value, out, separators=(',', ':'), sort_keys=True)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())
    if value.get('schema') == 'heavy-receipt-v1':
        fault('receipt_fsynced', value['token'])
    if path.name == 'state.json':
        gate = value.get('gate')
        if gate and any(event['event'] == 'pause_begin' for event in value.get('pending', [])):
            fault('intent_fsynced', gate)
        for task in value.get('tasks', {}).values():
            if task['state'] == 'released' and any(event['id'] == task['token'] + ':release_end'
                                                   for event in value.get('pending', [])):
                fault('release_fsynced', task['token'])
    os.replace(temporary, path)
    sync_dir(path.parent)


def complete_lines(path):
    """Discard an interrupted final append, while the host lock is held."""
    if not path.exists():
        return []
    data = path.read_bytes()
    end = data.rfind(b'\n') + 1
    if end != len(data):
        with path.open('r+b') as out:
            out.truncate(end)
            out.flush()
            os.fsync(out.fileno())
    return data[:end].decode().splitlines()


def durable_append(path, lines):
    if not lines:
        return
    storage_fault('journal')
    with path.open('a', encoding='utf-8') as out:
        os.fchmod(out.fileno(), 0o600)
        for line in lines:
            out.write(line + '\n')
        out.flush()
        os.fsync(out.fileno())
    sync_dir(path.parent)


class Ledger:
    def __init__(self):
        self.path = BASE / 'state.json'
        if self.path.exists():
            self.data = json.loads(self.path.read_text())
            if self.data.get('version') != 1:
                raise Refusal('unsupported host ledger version')
            if self.data['queue'] != QUEUE:
                raise Refusal('state directory belongs to a different queue identity')
        else:
            self.data = {'version': 1, 'queue': QUEUE, 'tasks': {}, 'maintenance': {}, 'resume_requests': [],
                         'gate': None, 'pending': [], 'installation': None}
        self.flush()

    @property
    def tasks(self):
        return self.data['tasks']

    def event(self, task, key, **extra):
        token = task['token'] if task else 'host'
        now = time.time_ns()
        fields = {'id': token + ':' + key, 'event': key, 'time_ns': now, 'queue': QUEUE,
                  'boot': boot(), 'token': token,
                  'source': sys.argv[1] if len(sys.argv) > 1 and sys.argv[1].startswith('--') else 'submit',
                  'free_disk_bytes': shutil.disk_usage(ROOT).free,
                  'free_memory_bytes': available_memory()}
        if task:
            fields.update({key: task[key] for key in ('lane', 'label', 'repo', 'tree', 'policy')})
        fields.update(extra)
        self.data['pending'].append(fields)

    def save(self):
        atomic(self.path, self.data)
        self.flush()

    def flush(self):
        # The snapshot's outbox precedes both journals. Event IDs and unique log
        # paths make replay after append/fsync/rename interruptions idempotent.
        pending_rows = [task for task in self.tasks.values() if 'row' in task and not task.get('row_durable')]
        if not self.data['pending'] and not pending_rows:
            return
        events = BASE / 'events.jsonl'
        seen = {json.loads(line)['id'] for line in complete_lines(events)}
        missing = []
        for event in self.data['pending']:
            if event['id'] not in seen:
                missing.append(json.dumps(event, separators=(',', ':'), sort_keys=True))
                seen.add(event['id'])
        durable_append(events, missing)
        if missing:
            fault('events_fsynced', self.data['pending'][-1]['token'])
        rows = complete_lines(LOGS / 'heavy.tsv')
        if pending_rows:
            logs = {line.split('\t')[8] for line in rows if len(line.split('\t')) == 11}
            durable_append(LOGS / 'heavy.tsv', [task['row'] for task in pending_rows if task['log'] not in logs])
            for task in pending_rows:
                task['row_durable'] = True
        self.data['pending'] = []
        atomic(self.path, self.data)


@contextlib.contextmanager
def locked():
    BASE.mkdir(parents=True, exist_ok=True, mode=0o700)
    LOGS.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (BASE / 'host.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield Ledger()


def available_memory():
    for line in Path('/proc/meminfo').read_text().splitlines():
        if line.startswith('MemAvailable:'):
            return int(line.split()[1]) * 1024
    return 0


def properties(unit):
    result = run(['systemctl', '--user', 'show', unit,
                  '--property=LoadState,ActiveState,ControlGroup,CPUUsageNSec,MemoryPeak'])
    values = dict(line.split('=', 1) for line in result.stdout.splitlines() if '=' in line)
    if not values:
        raise Refusal('cannot inspect owned systemd unit')
    return values


def own_cgroup(task):
    props = properties(task['unit'])
    group = props.get('ControlGroup', '')
    if group:
        if Path(group).name != task['unit'] or not group.startswith('/user.slice/'):
            raise Refusal('unexpected task cgroup')
        for field in ('CPUUsageNSec', 'MemoryPeak'):
            if props.get(field, '').isdigit() and int(props[field]) < 18446744073709551615:
                task.setdefault('resources', {})[field] = max(
                    int(props[field]), task.get('resources', {}).get(field, 0))
        return CGROOT / group.lstrip('/')
    if props.get('ActiveState') == 'active':
        raise Refusal('active owned unit has no cgroup evidence')
    return None


def populated(path):
    try:
        return 'populated 1' in (path / 'cgroup.events').read_text()
    except FileNotFoundError:
        if path.exists():
            raise Refusal('cgroup v2 population evidence is unavailable')
        return False


def scope_empty(task):
    path = own_cgroup(task)
    recorded = task.get('cgroup')
    if recorded and populated(CGROOT / recorded.lstrip('/')):
        return False
    return path is None or not populated(path)


def kill_scope(task):
    # Unit names are token-derived and never reused. Mark closing durably before
    # this call: a late-created service's guard cannot start a payload.
    own_cgroup(task)
    run(['systemctl', '--user', 'kill', '--kill-whom=all', '--signal=KILL', task['unit']])
    run(['systemctl', '--user', 'stop', '--no-block', task['unit']])
    return scope_empty(task)


def fault(point, token):
    # Fixture-only barriers live in the explicitly isolated DEV_TMP. A kill of
    # this process while blocked models loss at the production entrypoint.
    directory = os.environ.get('HEAVY_FAULT_DIR')
    if directory:
        path = Path(directory).resolve()
        if os.environ.get('HEAVY_RECOVERY_FIXTURE') != '1' or not path.is_relative_to(TMP):
            raise Refusal('fault barriers require an isolated recovery fixture')
        if (path / ('arm-' + point)).exists():
            (path / (token + '-' + point)).write_text(str(os.getpid()))
            while not (path / ('continue-' + point)).exists():
                time.sleep(0.02)


def storage_fault(point):
    directory = os.environ.get('HEAVY_FAULT_DIR')
    if not directory:
        return
    path = Path(directory).resolve()
    if os.environ.get('HEAVY_RECOVERY_FIXTURE') != '1' or not path.is_relative_to(TMP):
        raise Refusal('storage faults require an isolated recovery fixture')
    if (path / ('enospc-' + point)).exists():
        (path / ('hit-enospc-' + point)).touch()
        raise OSError(errno.ENOSPC, 'injected isolated storage write failure')


RECOVERY_DIAGNOSTIC = (
    'heavy: recovery/storage failure; task ownership is retained. Free space, restore/restart '
    'the SAME recovery service if needed, then run heavy --reconcile with the SAME queue and '
    'DEV_TMP state. RETAIN the ledger; do not delete/reset it. Rerun land if completion is '
    'recovered with exit 137.')


def waiter_failure():
    # Inspect reported failures, never infer one from task age or tick delays.
    for suffix in ('-reconcile.timer', '-reconcile.service'):
        result = run(['systemctl', '--user', 'show', PREFIX + suffix,
                      '--property=LoadState,ActiveState,Result,ExecMainStatus'])
        values = dict(line.split('=', 1) for line in result.stdout.splitlines() if '=' in line)
        if result.returncode or values.get('LoadState') != 'loaded':
            return True
        if suffix.endswith('.timer') and values.get('ActiveState') != 'active':
            return True
        if values.get('ActiveState') == 'failed' or values.get('Result', 'success') not in ('', 'success'):
            return True
    return False


def apply_holds(ledger):
    held = set(ledger.data['maintenance'])
    gate = ledger.data['gate']
    if not gate and not held and not ledger.data.get('resume_requests'):
        return
    groups = json.loads(queue('group', '--json'))
    if gate:
        held.update(name for name in groups if name != 'gate')
    for name in sorted(held):
        if name in groups and groups[name]['status'] == 'Running':
            queue('pause', '--wait', '--group', name)
    if not gate:
        resumed = []
        for name in ledger.data.get('resume_requests', []):
            if name not in held:
                queue('start', '--group', name)
                resumed.append(name)
        if resumed:
            ledger.data['resume_requests'] = [name for name in ledger.data['resume_requests'] if name not in resumed]
            ledger.save()


def finish(ledger, task):
    now = time.time()
    task['state'] = 'released'
    task.setdefault('code', 137)
    if task.get('outcome') == 'recovered':
        task['code'] = 137
    if not task.get('payload_started') or not task.get('payload_ended'):
        task['code'] = task['code'] or 137
        if task.get('outcome') in (None, 'completed'):
            task['outcome'] = 'refused'
    task['release_complete'], task['cgroup_empty'] = True, True
    task['ended'] = now
    waited = int(task.get('payload_started', now) - task['submitted'])
    ran = int(now - task.get('payload_started', now))
    task['waited'], task['ran'] = waited, ran
    command = ' '.join(task['args'])
    clean = lambda value: str(value).replace('\t', ' ').replace('\r', ' ').replace('\n', ' ')
    task['row'] = '\t'.join(map(clean, [task.get('id', '-'), task['lane'],
        time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now)), waited, ran, task['code'],
        f'{available_memory() / 1073741824:.1f}', task['repo'], task['log'], command, task['label']]))
    ledger.event(task, 'release_end', outcome=task.get('outcome', 'completed'), code=task['code'],
                 cgroup_empty=True, cgroup=task.get('cgroup'))
    ledger.event(task, 'task_completion', code=task['code'])
    ledger.save()
    fault('release_saved', task['token'])
    cleanup_transport(task)


def cleanup_transport(task):
    (BASE / (task['token'] + '.env')).unlink(missing_ok=True)
    sync_dir(BASE)


def publish_receipt(ledger, task):
    """Seal a separate log after release, then publish replayable evidence."""
    if not task.get('row_durable') or not task.get('release_complete'):
        return
    receipts = BASE / 'receipts'
    path = receipts / (task['token'] + '.json')
    if path.exists():
        return
    receipts.mkdir(exist_ok=True, mode=0o700)
    sync_dir(BASE)
    log = Path(task['log'])
    if not task.get('log_complete'):
        # A delayed service can only open the capture file. It can never
        # append to this completed log, including after receipt publication.
        temporary = log.with_suffix('.new')
        storage_fault('seal')
        with temporary.open('wb') as out:
            os.fchmod(out.fileno(), 0o600)
            capture = Path(task['capture'])
            if capture.exists():
                with capture.open('rb') as source:
                    shutil.copyfileobj(source, out)
            out.flush()
            os.fsync(out.fileno())
        os.replace(temporary, log)
        sync_dir(log.parent)
        task['log_complete'] = True
        ledger.save()
    with log.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    receipt = {'schema': 'heavy-receipt-v1', 'token': task['token'], 'id': task.get('id', ''),
               'queue': task['queue'], 'lane': task['lane'], 'tree': task['tree'],
               'policy': task['policy'], 'args': task['args'], 'outcome': task.get('outcome', 'refused'),
               'code': task['code'], 'payload_started': bool(task.get('payload_started')),
               'payload_ended': bool(task.get('payload_ended')), 'release_complete': True,
               'cgroup_empty': True, 'elapsed_s': max(0.0, task.get('payload_ended', 0) - task.get('payload_started', 0)) if task.get('payload_ended') else 0.0,
               'log': task['log'], 'log_sha256': digest}
    fault('receipt_ready', task['token'])
    atomic(path, receipt)
    fault('receipt_published', task['token'])


def observer_unit(task):
    return PREFIX + '-' + task['token'] + '-observe.service'


def ensure_observer(task):
    # The timer owns this observer independently of the submitting client.
    # It never uses a queue ID for a destructive operation.
    unit = observer_unit(task)
    if properties(unit).get('ActiveState') in ('active', 'activating'):
        return
    env = BASE / 'reconcile.env'
    result = run(['systemd-run', '--user', '--quiet', '--collect', '--unit=' + unit,
                  '--property=EnvironmentFile=' + str(env), '--property=UMask=0077',
                  '--property=StandardInput=null', '--property=StandardOutput=null',
                  '--property=StandardError=null', sys.executable, Path(__file__).resolve(),
                  '--observe', task['token']])
    if result.returncode:
        raise Refusal('cannot start queued completion observer')


def observe(token):
    with locked() as ledger:
        task = ledger.tasks[token]
        if task['state'] != 'queued':
            return 0
        task_id = task['id']
    # Only the wait exit is observed. No queue records, logs or environments
    # are requested or retained. Completion/missing ID is never success proof.
    waiter = subprocess.Popen(pueue_command('wait', '--quiet', task_id),
                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL)
    try:
        while waiter.poll() is None:
            current = json.loads((BASE / 'state.json').read_text())['tasks'][token]
            if current['state'] != 'queued':
                return 0
            time.sleep(0.1)
        if waiter.returncode:
            return 2  # Transport failure: retry through the independent timer.
        fault('queue_observed', token)
        with locked() as ledger:
            task = ledger.tasks[token]
            if task['state'] == 'queued':
                task.update(state='closing', outcome='cancelled', code=137)
                ledger.event(task, 'release_begin', outcome='cancelled')
                ledger.event(task, 'queue_completion', reason='unclaimed token; terminal or absent queue ID')
                ledger.save()
            # A terminal queue row cannot prove a launched payload ended.
            # The common path always kills/proves the owned cgroup empty.
            reconcile(ledger)
        return 0
    finally:
        if waiter.poll() is None:
            waiter.terminate()
        waiter.wait()


def reconcile(ledger):
    # Persistent gate holds precede queue mutations even after a queue restart.
    queue_available = True
    try:
        apply_holds(ledger)
    except Refusal:
        # Queue availability is needed to resume holds, never to kill an
        # orphan's scope. Keep gate intent durable until the queue returns.
        queue_available = False
    for task in ledger.tasks.values():
        if task['state'] == 'enqueuing':
            try:
                queue('group', '--json')
            except Refusal:
                queue_available = False
                continue
            # Re-enqueue is safe for the known ID: the guard still admits
            # only a queued token, under this lock. If the first enqueue
            # succeeded before its caller died, the wrapper can claim it.
            run(pueue_command('enqueue', task['id']))
            task['state'] = 'queued'
            ledger.event(task, 'enqueue_recovery')
            ledger.save()
        if task['state'] == 'submitted' and not alive(task.get('submitter')):
            task['state'] = 'waiting'
        if task['state'] == 'queued' and queue_available:
            ensure_observer(task)
        if task['state'] == 'released':
            cleanup_transport(task)
            publish_receipt(ledger, task)
        if task['state'] in ('submitted', 'queued', 'released'):
            continue
        orphan = task['boot'] != boot() or not alive(task.get('wrapper'))
        if task['state'] != 'closing' and (orphan or task['state'] == 'finishing'):
            task['state'] = 'closing'
            task['outcome'] = 'recovered' if orphan else 'completed'
            ledger.event(task, 'release_begin', outcome=task['outcome'])
            if orphan:
                ledger.event(task, 'recovery', reason='wrapper identity ended or boot changed')
            ledger.save()
        if task['state'] != 'closing':
            continue
        if not kill_scope(task):
            continue
        fault('release_empty', task['token'])
        if ledger.data['gate'] == task['token']:
            if not queue_available:
                continue
            # Still closing, and gate remains durable until every resume is
            # replayable. An operator hold is checked on every retry.
            try:
                for group in task.get('resume', []):
                    if group not in ledger.data['maintenance']:
                        queue('start', '--group', group)
            except Refusal:
                queue_available = False
                continue
            fault('resumed', task['token'])
            ledger.data['gate'] = None
        finish(ledger, task)
        publish_receipt(ledger, task)
    return queue_available


def probe_containment():
    # Real kernel probe: a session change must remain in the same task cgroup.
    probe = PREFIX + '-probe-' + uuid.uuid4().hex
    result = run(['systemd-run', '--user', '--quiet', '--wait', '--pipe', '--collect',
                  '--unit=' + probe, '--property=ExitType=cgroup', '--property=KillMode=control-group',
                  '--property=Delegate=no', '/bin/sh', '-c',
                  'group=$(cut -d: -f3 /proc/self/cgroup); '
                  'grep -qx "populated 1" "/sys/fs/cgroup$group/cgroup.events" || exit 1; '
                  'cat /proc/self/cgroup; setsid sh -c "cat /proc/self/cgroup"'])
    groups = result.stdout.strip().splitlines()
    if result.returncode or len(groups) != 2 or groups[0] != groups[1] or probe not in groups[0]:
        raise Refusal('systemd cgroup containment with setsid is unsupported')
    return groups


def refresh_probe(ledger):
    installed = ledger.data['installation']
    if installed and installed['boot'] != boot():
        groups = probe_containment()
        installed.update(boot=boot(), probe=groups)
        ledger.event(None, 'containment_probe:' + uuid.uuid4().hex, cgroups=groups, reason='boot changed')
        ledger.save()


def ensure_reconciler(ledger):
    installed = ledger.data['installation']
    if not installed:
        raise Refusal('containment/reconciler not installed; run heavy --install-reconciler')
    refresh_probe(ledger)
    if run(['systemctl', '--user', 'is-active', '--quiet', PREFIX + '-reconcile.timer']).returncode:
        raise Refusal('independent reconciler timer is not active')


def safe_environment():
    banned = re.compile(r'^CLAUDE_CODE_|TOKEN|SECRET|PASSWORD|PASSWD|API_KEY|CREDENTIAL', re.I)
    return {key: value for key, value in os.environ.items() if not banned.search(key)}


def selects_kache():
    # Resolve the two independent Cargo wrapper slots in increasing config
    # precedence, then apply [env] and dedicated/generic environment overrides.
    names = ('PATH', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WRAPPER',
             'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')
    cargo_home = Path(os.environ.get('CARGO_HOME', str(Path.home() / '.cargo'))).resolve()
    directories = [cargo_home, *(parent / '.cargo' for parent in reversed((Path.cwd(), *Path.cwd().parents)))]
    build, settings, origins, seen = {}, {}, {}, set()
    for directory in directories:
        path = directory / 'config'
        if not path.is_file():
            path = directory / 'config.toml'
        if not path.is_file() or path.resolve() in seen:
            continue
        seen.add(path.resolve())
        try:
            with path.open('rb') as source:
                config = tomllib.load(source)
        except (OSError, ValueError):
            raise Refusal('cannot resolve Cargo wrapper configuration') from None
        if not isinstance(config.get('build', {}), dict) or not isinstance(config.get('env', {}), dict):
            raise Refusal('cannot resolve Cargo wrapper configuration')
        if 'include' in config:
            raise Refusal('Cargo configuration includes are outside the declared policy')
        for field in ('rustc-wrapper', 'rustc-workspace-wrapper'):
            if field in config.get('build', {}):
                build[field] = config['build'][field]
                origins[field] = directory.parent
        for name in names:
            if name in config.get('env', {}):
                value = config['env'][name]
                if isinstance(value, dict) and isinstance(settings.get(name), dict):
                    settings[name].update(value)
                else:
                    settings[name] = value
                origins[name] = directory.parent
    environment = dict(os.environ)
    for name, value in settings.items():
        value = {'value': value} if isinstance(value, str) else value
        if not isinstance(value, dict) or not isinstance(value.get('value'), str):
            raise Refusal('cannot resolve Cargo wrapper configuration')
        if name not in environment or value.get('force', False):
            setting = value['value']
            if value.get('relative', False):
                setting = str((origins[name] / setting).resolve())
            environment[name] = setting
    for field, dedicated in (('rustc-wrapper', 'RUSTC_WRAPPER'),
                             ('rustc-workspace-wrapper', 'RUSTC_WORKSPACE_WRAPPER')):
        generic = 'CARGO_BUILD_' + field.upper().replace('-', '_')
        value = environment.get(dedicated, environment.get(generic, build.get(field, '')))
        if not isinstance(value, str):
            raise Refusal('cannot resolve Cargo wrapper configuration')
        if not value:
            continue
        base = Path.cwd() if dedicated in environment or generic in environment else origins[field]
        command = str(base / value) if '/' in value and not Path(value).is_absolute() else value
        resolved = shutil.which(command, path=environment.get('PATH'))
        if Path(value).name == 'kache' or (resolved and Path(resolved).resolve().name == 'kache'):
            return True
    return False


def environment_file(path, env):
    # systemd EnvironmentFile syntax: double quotes, escaped slash/quote/dollar/
    # backtick; literal newlines are supported inside the quoted value.
    with path.open('w') as out:
        for key, value in env.items():
            if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', key):
                continue
            value = value.replace('\\', '\\\\').replace('"', '\\"').replace('$', '\\$').replace('`', '\\`')
            out.write(key + '="' + value + '"\n')
        out.flush()
        os.fsync(out.fileno())
    sync_dir(path.parent)


def install(interval):
    if not re.fullmatch(r'[0-9]+(?:\.[0-9]+)?s?', interval) or float(interval.rstrip('s')) <= 0:
        raise Refusal('invalid reconciler interval')
    groups = probe_containment()
    with locked() as ledger:
        env = {key: os.environ[key] for key in ('PATH', 'DEV_ROOT', 'DEV_TMP', 'PUEUE_CONFIG_PATH',
               'PUEUE_PROFILE', 'HEAVY_BOOT_ID_FILE', 'HEAVY_RECOVERY_FIXTURE', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS') if key in os.environ}
        env['DEV_ROOT'], env['DEV_TMP'] = str(ROOT), str(TMP)
        if os.environ.get('HEAVY_RECOVERY_FIXTURE') == '1' and 'HEAVY_FAULT_DIR' in os.environ:
            env['HEAVY_FAULT_DIR'] = os.environ['HEAVY_FAULT_DIR']
        environment_file(BASE / 'reconcile.env', env)
        units = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'systemd/user'
        if any(char in str(path) for path in (BASE, units, KIT) for char in '\r\n'):
            raise Refusal('systemd integration paths must be single-line paths')
        units.mkdir(parents=True, exist_ok=True)
        # Quote systemd specifiers and ExecStart words independently of a shell.
        quote = lambda value: '"' + str(value).replace('%', '%%').replace('\\', '\\\\').replace('"', '\\"') + '"'
        service = PREFIX + '-reconcile.service'
        timer = PREFIX + '-reconcile.timer'
        (units / service).write_text('[Unit]\nDescription=heavy owned cgroup recovery\nStartLimitIntervalSec=0\n'
            '[Service]\nType=oneshot\nUMask=0077\nEnvironmentFile=' + str(BASE / 'reconcile.env').replace('%', '%%') + '\n'
            'ExecStart=' + quote(sys.executable) + ' ' + quote(Path(__file__).resolve()) + ' --reconcile\n')
        (units / timer).write_text('[Unit]\nDescription=heavy recovery polling\n[Timer]\n'
            'OnBootSec=' + interval + '\nOnUnitInactiveSec=' + interval + '\nAccuracySec=100ms\n'
            'Unit=' + service + '\n[Install]\nWantedBy=timers.target\n')
        run(['systemctl', '--user', 'daemon-reload'], check=True)
        run(['systemctl', '--user', 'enable', '--now', timer], check=True)
        ledger.data['installation'] = {'boot': boot(), 'interval': interval, 'probe': groups,
                                        'units': str(units)}
        ledger.event(None, 'containment_probe:' + uuid.uuid4().hex, cgroups=groups, interval=interval)
        ledger.save()
    print('heavy: independent reconciler ' + timer + '; interval ' + interval)


def maintenance(group, pause):
    with locked() as ledger:
        if group == 'gate' or group not in json.loads(queue('group', '--json')):
            raise Refusal('maintenance requires a known non-gate group')
        if pause:
            ledger.data['maintenance'][group] = True
            ledger.data['resume_requests'] = [name for name in ledger.data.get('resume_requests', []) if name != group]
        else:
            ledger.data['maintenance'].pop(group, None)
            ledger.data.setdefault('resume_requests', []).append(group)
        ledger.event(None, 'maintenance:' + uuid.uuid4().hex, group=group, held=pause)
        ledger.save()
        apply_holds(ledger)
        reconcile(ledger)


def gate_candidate():
    head = run(['git', 'rev-parse', 'HEAD'])
    tree = run(['git', 'rev-parse', 'HEAD^{tree}'])
    status = run(['git', 'status', '--porcelain', '--untracked-files=normal', '--ignore-submodules=none'])
    states = [run(['git', 'rev-parse', '--git-path', name])
              for name in ('rebase-merge', 'rebase-apply', 'MERGE_HEAD')]
    if head.returncode or tree.returncode or status.returncode or status.stdout.strip() or any(
            state.returncode or Path(state.stdout.strip()).exists() for state in states):
        raise Refusal('gate candidate HEAD/index/worktree or rebase state is not clean; rerun land')
    if run(['git', 'rev-parse', 'HEAD']).stdout.strip() != head.stdout.strip():
        raise Refusal('gate candidate commit changed during validation; rerun land')
    return head.stdout.strip(), tree.stdout.strip()


def check_gate_candidate(ledger, task):
    if task['lane'] != 'gate' or task['tree'] == 'unversioned':
        return True
    try:
        commit, tree = gate_candidate()
        valid = commit == task['commit'] and tree == task['tree']
    except Refusal:
        valid = False
    if not valid:
        task.update(state='finishing', code=2, outcome='refused',
                    refusal='gate candidate HEAD/index/worktree or rebase changed; rerun land')
        ledger.event(task, 'refusal', reason=task['refusal'])
        ledger.save()
    return valid


def submit(args, lane, tail_lines):
    if os.environ.get('HEAVY_TASK'):
        raise Refusal('already inside a heavy task; run the command directly')
    commit = run(['git', 'rev-parse', 'HEAD']).stdout.strip() or 'unversioned'
    tree = run(['git', 'rev-parse', 'HEAD^{tree}']).stdout.strip() or 'unversioned'
    if lane == 'gate' and run(['git', 'rev-parse', '--is-inside-work-tree']).returncode == 0:
        commit, tree = gate_candidate()
    token = uuid.uuid4().hex
    with locked() as ledger:
        ensure_reconciler(ledger)
        if not reconcile(ledger):
            raise Refusal('queue is unavailable; admission is held')
        task = {'token': token, 'lane': lane, 'args': args, 'label': os.environ.get('HEAVY_LABEL', Path.cwd().name),
                'repo': str(Path.cwd()), 'tree': tree, 'commit': commit, 'policy': os.environ.get('SKEIN_GATE_POLICY_ID', 'gate-policy-unavailable') if lane == 'gate' else 'build-4',
                'boot': boot(), 'queue': QUEUE, 'unit': PREFIX + '-' + token + '.service',
                'submitter': identity(),
                'submitted': time.time(), 'state': 'submitted', 'log': str(LOGS / (token + '.log')),
                'capture': str(LOGS / (token + '.capture'))}
        ledger.tasks[token] = task
        Path(task['capture']).touch(mode=0o600)
        ledger.event(task, 'submit')
        ledger.save()
        env = safe_environment()
        env['HEAVY_TASK'] = '1'
        env['NEXTEST_DOUBLE_SPAWN'] = os.environ.get('NEXTEST_DOUBLE_SPAWN', '0')
        env.pop('NEXTEST_TEST_THREADS', None)
        env['CARGO_BUILD_JOBS'] = os.environ.get('HEAVY_BUILD_JOBS_EFFECTIVE', '4') if lane == 'build' else '8'
        env['HEAVY_LANE'] = lane
        # A stashed task cannot beat the durable token -> queue-ID mapping.
        result = run(pueue_command('add', '--stashed', '--print-task-id', '--group', lane,
                      '--working-directory', task['repo'], '--label', task['label'], '--escape', '--',
                      KIT / 'heavy', '--task', token), env=env)
        if result.returncode or not result.stdout.strip().isdigit():
            raise Refusal('queue submission failed')
        task['id'] = result.stdout.strip()
        task['state'] = 'enqueuing'
        ledger.save()
        fault('enqueue_intent', token)
        queue('enqueue', task['id'])
        task['state'] = 'queued'
        ledger.save()
        ensure_observer(task)
    print(f"heavy: queued task {task['id']} in the {lane} lane token {token}", file=sys.stderr, flush=True)
    warned, next_health = False, 0.0
    while True:
        if time.monotonic() >= next_health:
            failed = waiter_failure()
            if failed and not warned:
                print(RECOVERY_DIAGNOSTIC, file=sys.stderr, flush=True)
            warned = failed
            next_health = time.monotonic() + 1
        # Atomic snapshots let clients observe completion without contending
        # with admission. The row marker follows the durable event/TSV flush.
        try:
            task = json.loads((BASE / 'state.json').read_text())['tasks'][token]
        except (OSError, ValueError):
            print(RECOVERY_DIAGNOSTIC, file=sys.stderr, flush=True)
            return 2
        if task['state'] == 'released' and (BASE / 'receipts' / (token + '.json')).is_file():
            break
        time.sleep(0.1)
    if task.get('refusal'):
        print('heavy: ' + task['refusal'], file=sys.stderr, flush=True)
    lines = Path(task['log']).read_text(errors='replace').splitlines()
    if len(lines) > tail_lines:
        print(f"heavy: … {len(lines) - tail_lines} earlier lines in {task['log']}")
    if tail_lines:
        print('\n'.join(lines[-tail_lines:]))
    print(f"heavy: task {task['id']} ({lane}) exit {task['code']}, waited {task['waited']}s, "
          f"ran {task['ran']}s, log {task['log']}")
    print(f"heavy: receipt {BASE / 'receipts' / (token + '.json')} token {token}")
    return task['code']


def wrapper(token):
    with locked() as ledger:
        reconcile(ledger)
        task = ledger.tasks[token]
        if task['state'] != 'queued':
            raise Refusal('token is not queued; refusing replay')
        task['wrapper'] = identity()
        task['boot'] = boot()
        task['state'] = 'waiting'
        ledger.event(task, 'wrapper_start')
        ledger.save()
    while True:
        with locked() as ledger:
            queue_available = reconcile(ledger)
            task = ledger.tasks[token]
            if task['state'] == 'released':
                return task['code']
            if not queue_available:
                time.sleep(0.1)
                continue
            if task['lane'] == 'gate':
                if ledger.data['gate'] not in (None, token):
                    ready = False
                else:
                    if ledger.data['gate'] is None:
                        groups = json.loads(queue('group', '--json'))
                        task['resume'] = [name for name, group in groups.items()
                                          if name != 'gate' and group['status'] == 'Running']
                        task['initially_paused'] = [name for name, group in groups.items()
                                                   if name != 'gate' and group['status'] != 'Running']
                        ledger.data['gate'] = token
                        ledger.event(task, 'pause_begin', groups=task['resume'], initially_paused=task['initially_paused'])
                        ledger.save()  # Pause intent is durable BEFORE any pause.
                        fault('intent', token)
                        apply_holds(ledger)
                        ledger.event(task, 'pause_end')
                        ledger.event(task, 'drain_begin')
                        ledger.save()
                        fault('paused', token)
                    ready = not any(other['token'] != token and other['lane'] == 'build'
                                    and other['state'] in ('launching', 'active', 'finishing', 'closing')
                                    for other in ledger.tasks.values())
                    if ready:
                        ledger.event(task, 'drain_end')
            else:
                ready = not ledger.data['gate'] and sum(other['lane'] == 'build' and
                    other['state'] in ('launching', 'active', 'finishing', 'closing')
                    for other in ledger.tasks.values()) < 2
                ready = ready and 'build' not in ledger.data['maintenance']
            if ready:
                # Lazy-starting kache here would place the shared daemon inside
                # this task. Its separate user service must already own it.
                if selects_kache() and run(
                        ['systemctl', '--user', 'is-active', '--quiet', 'kache.service']).returncode:
                    task['state'], task['code'] = 'finishing', 2
                    task['refusal'] = 'selected Cargo wrapper requires the shared kache.service; start that service or deliberately disable the selected wrapper'
                    ledger.event(task, 'refusal', reason=task['refusal'])
                    ledger.save()
                    continue
                task['state'] = 'launching'
                ledger.event(task, 'admit', build_slots=2, jobs=os.environ.get('CARGO_BUILD_JOBS'))
                environment_file(BASE / (token + '.env'), safe_environment())
                ledger.save()
                # The in-cgroup guard takes the same lock and checks the durable
                # token. A delayed StartTransientUnit after orphan recovery can
                # create no payload, even if this wrapper dies during launch.
                launcher = subprocess.Popen(['systemd-run', '--user', '--quiet', '--wait', '--collect',
                    '--unit=' + task['unit'], '--service-type=exec', '--property=ExitType=cgroup',
                    '--property=KillMode=control-group', '--property=Delegate=no',
                    '--property=CPUAccounting=yes', '--property=MemoryAccounting=yes',
                    '--property=EnvironmentFile=' + str(BASE / (token + '.env')),
                    '--property=StandardOutput=append:' + task['capture'], '--property=StandardError=inherit',
                    '--working-directory=' + task['repo'], str(KIT / 'heavy'), '--payload', token],
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                break
        time.sleep(0.1)
    while True:
        current = json.loads((BASE / 'state.json').read_text())['tasks'][token]
        if current['state'] == 'released' and current.get('row_durable'):
            launcher.wait()
            return current['code']
        ended = launcher.poll() is not None
        if current['state'] not in ('finishing', 'closing') and not ended:
            time.sleep(0.1)
            continue
        with locked() as ledger:
            task = ledger.tasks[token]
            if ended and task['state'] in ('launching', 'active'):
                task['state'], task['code'] = 'finishing', 2 if task['state'] == 'launching' else 137
                ledger.event(task, 'refusal', reason='contained service failed before payload entry')
                ledger.save()
            reconcile(ledger)
            if task['state'] == 'released':
                return task['code']
        time.sleep(0.1)


def phase(token, name, **values):
    with locked() as ledger:
        task = ledger.tasks[token]
        if name == 'payload_end':
            task['payload_ended'] = time.time()
        ledger.event(task, name, **values)
        ledger.save()


def gc(token, serial):
    phase(token, 'gc_begin:' + str(serial))
    # Only connect to an existing shared service; never create a lazy daemon.
    if shutil.which('kache') and run(['systemctl', '--user', 'is-active', '--quiet', 'kache.service']).returncode == 0:
        run(['kache', 'gc'])
    phase(token, 'gc_end:' + str(serial))


def prepare_payload_policy(token):
    # Called only after the durable ownership guard marks this cgroup active.
    # All runtime version launchers and their descendants belong to its scope.
    names = ('SKEIN_GATE_POLICY_ID', 'SKEIN_TEST_CHROMIUM', 'SKEIN_TEST_FONTS', 'SKEIN_GATE_POLICY_HELPER')
    result = run(['bash', '-c', 'source "$1"; gate_policy_prepare || exit 2; '
                  'printf "%s\\0" "$SKEIN_GATE_POLICY_ID" "$SKEIN_TEST_CHROMIUM" '
                  '"$SKEIN_TEST_FONTS" "$SKEIN_GATE_POLICY_HELPER"',
                  'heavy-policy', KIT / 'gate-policy.sh'])
    values = result.stdout.split('\0')
    with locked() as ledger:
        task = ledger.tasks[token]
        if task['state'] != 'active':
            return False
        expected = os.environ.get('SKEIN_GATE_POLICY_ID')
        if result.returncode or len(values) != 5 or values[0] != task['policy'] or (
                expected and values[0] != expected):
            task.update(state='finishing', code=2, outcome='refused',
                        refusal='submitted gate policy no longer validates at contained entry')
            ledger.event(task, 'refusal', reason=task['refusal'])
            ledger.save()
            return False
        os.environ.update(zip(names, values[:4]))
    return True


def payload(token):
    with locked() as ledger:
        task = ledger.tasks[token]
        if task['state'] != 'launching' or not alive(task['wrapper']) or task['boot'] != boot():
            return 137
        group = Path('/proc/self/cgroup').read_text().strip().split('::', 1)[1]
        if Path(group).name != task['unit']:
            raise Refusal('payload did not enter its owned cgroup')
        if not populated(CGROOT / group.lstrip('/')):
            raise Refusal('payload cgroup population cannot be proved')
        task['cgroup'] = group
        task['state'] = 'active'
        if not check_gate_candidate(ledger, task):
            return 2
        ledger.event(task, 'scope_enter', cgroup=group)
        ledger.save()
        args, lane = task['args'], task['lane']
    if lane == 'gate':
        if not prepare_payload_policy(token):
            return 2
        phase(token, 'settle_begin')
        settle = int(os.environ.get('HEAVY_SETTLE', '120'))
        start = time.monotonic()
        while time.monotonic() - start < settle and os.getloadavg()[0] >= len(os.sched_getaffinity(0)):
            time.sleep(min(5, max(0, settle - (time.monotonic() - start))))
        phase(token, 'settle_end')
        fault('settled', token)
    with locked() as ledger:
        task = ledger.tasks[token]
        if task['state'] != 'active':
            return 137
        if not check_gate_candidate(ledger, task):
            return 2
        task['payload_started'] = time.time()
        ledger.event(task, 'payload_begin')
        ledger.save()
    child = subprocess.Popen(args, stdin=subprocess.DEVNULL)
    next_gc = time.monotonic() + 120
    serial = 0
    while child.poll() is None:
        if lane == 'gate' and time.monotonic() >= next_gc:
            gc(token, serial)
            serial += 1
            next_gc = time.monotonic() + 120
        time.sleep(0.05)
    code = child.returncode if child.returncode >= 0 else 128 - child.returncode
    if lane == 'gate':
        with locked() as ledger:
            elapsed = int(time.time() - ledger.tasks[token]['payload_started'])
        print(f'heavy: ran alone for {elapsed}s', flush=True)
    with locked() as ledger:
        if not check_gate_candidate(ledger, ledger.tasks[token]):
            return 2
    phase(token, 'payload_end', code=code)
    gc(token, serial)
    with locked() as ledger:
        task = ledger.tasks[token]
        own_cgroup(task)  # Sample accounting while the completed payload's unit still exists.
        task['code'], task['state'] = code, 'finishing'
        ledger.save()
    fault('payload_finished', token)
    # ExitType=cgroup holds any setsid children until the outside reconciler
    # kills this service and proves it empty. No child can outlive release.
    return code


def main(args):
    if len(args) == 2 and args[0] == '--group-held':
        with locked() as ledger:
            return 0 if args[1] in ledger.data['maintenance'] or (
                ledger.data['gate'] and args[1] != 'gate') else 1
    if len(args) in (2, 3) and args[0] == '--record-refusal':
        if args[1] not in ('policy', 'disk-floor'):
            raise Refusal('unknown refusal category')
        with locked() as ledger:
            task = ledger.tasks.get(args[2]) if len(args) == 3 else None
            ledger.event(task, 'refusal:' + uuid.uuid4().hex, reason=args[1])
            ledger.save()
        return 0
    if args == ['--check-recovery']:
        with locked() as ledger:
            ensure_reconciler(ledger)
        return 0
    if len(args) == 2 and args[0] == '--repair-group':
        with locked() as ledger:
            reconcile(ledger)
            group = args[1]
            if group not in json.loads(queue('group', '--json')):
                raise Refusal('cannot repair an unknown group')
            if group not in ledger.data['maintenance'] and not (ledger.data['gate'] and group != 'gate'):
                queue('start', '--group', group)
        return 0
    if args and args[0] == '--install-reconciler':
        install(args[1] if len(args) > 1 else '5s')
        return 0
    if args == ['--remove-reconciler']:
        with locked() as ledger:
            reconcile(ledger)
            if any(task['state'] != 'released' for task in ledger.tasks.values()):
                raise Refusal('cannot remove reconciler with unfinished tasks')
            installed = ledger.data['installation']
            for task in ledger.tasks.values():
                run(['systemctl', '--user', 'stop', observer_unit(task)])
            run(['systemctl', '--user', 'disable', '--now', PREFIX + '-reconcile.timer'], check=True)
            run(['systemctl', '--user', 'stop', PREFIX + '-reconcile.service'])
            for suffix in ('.service', '.timer'):
                (Path(installed['units']) / (PREFIX + '-reconcile' + suffix)).unlink(missing_ok=True)
            run(['systemctl', '--user', 'daemon-reload'], check=True)
            ledger.data['installation'] = None
            ledger.save()
        return 0
    if args == ['--reconcile']:
        with locked() as ledger:
            reconcile(ledger)
            refresh_probe(ledger)
        return 0
    if len(args) == 2 and args[0] in ('--maintenance-pause', '--maintenance-resume'):
        maintenance(args[1], args[0] == '--maintenance-pause')
        return 0
    if len(args) == 2 and args[0] in ('--task', '--payload', '--observe'):
        return {'--task': wrapper, '--payload': payload, '--observe': observe}[args[0]](args[1])
    lane, tail_lines = 'build', int(os.environ.get('HEAVY_TAIL', '200'))
    while args:
        if args[0] == '--gate':
            lane, args = 'gate', args[1:]
        elif args[0] == '--tail' and len(args) > 1:
            tail_lines, args = int(args[1]), args[2:]
        elif args[0] == '--':
            args = args[1:]
            break
        elif args[0].startswith('-'):
            raise Refusal('usage: heavy [--gate] [--tail N] [--] command...')
        else:
            break
    if not args or tail_lines < 0:
        raise Refusal('usage: heavy [--gate] [--tail N] [--] command...')
    return submit(args, lane, tail_lines)


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (Refusal, subprocess.CalledProcessError, ValueError, OSError) as error:
        # Record fixed categories, never arbitrary subprocess text or caller
        # arguments. Wrong-queue/corrupt-state refusals cannot use that ledger.
        try:
            with locked() as refused:
                refused.event(None, 'refusal:' + uuid.uuid4().hex, reason=type(error).__name__)
                refused.save()
        except (Refusal, ValueError, OSError):
            pass
        print('heavy: ' + str(error), file=sys.stderr)
        sys.exit(2)
