#!/usr/bin/python3
"""App-server stand-in: scripted inventory and events, never a model transport."""
import ctypes
import json
import os
from pathlib import Path
import select
import socket
import signal
import subprocess
import sys
import tomllib

os.umask(0o077)
script = json.loads(Path(__file__).with_suffix('.json').read_text())
if sys.argv[1:3] == ['sandbox', '-P']:
    # The stand-in records the sandbox contract; native probe stories use Codex itself.
    profile = (Path(os.environ['CODEX_HOME']) / 'config.toml').read_text()
    Path(__file__).with_suffix('.probe').write_text(json.dumps(dict(argv=sys.argv[1:], profile=profile)))
    json.load(sys.stdin)
    checks = ('canary_read', 'home_list', 'source_write', 'network', 'pid_namespace', 'canary_process', 'work_write')
    print(json.dumps({name: name != script.get('probe_failure') for name in checks}), flush=True)
    raise SystemExit(0)
assert sys.argv[1:] == ['app-server', '--stdio', '--strict-config']
signal.signal(signal.SIGTERM, lambda _sig, _frame: None)
if script.get('rate_read'):
    assert set(os.environ) == {'CODEX_HOME', 'HOME', 'PATH', 'LANG'}
    assert os.environ['PATH'] == '/usr/bin:/bin' and os.environ['LANG'] == 'C.UTF-8'
    assert not list(Path(os.environ['HOME']).iterdir())
    assert Path(os.environ['HOME']).stat().st_mode & 0o777 == 0o700
    Path(__file__).with_suffix('.brake-home').write_text(os.environ['HOME'])
config_path = Path(os.environ['CODEX_HOME']) / 'config.toml'
config = {} if script.get('rate_read') else tomllib.loads(config_path.read_text())
work = os.getcwd()
thread, turn = 'script-thread', 'script-turn'


def send(value):
    print(json.dumps(value, separators=(',', ':')), flush=True)


def event(method, **params):
    item = params.get('item', {})
    # Like the host, each call is a raw function_call before its item and a raw output after it.
    raw = item.get('type') in ('commandExecution', 'mcpToolCall') and not script.get('no_raw')
    if raw and method == 'item/started':
        event('rawResponseItem/completed', item=dict(type='function_call', call_id=item['id'], arguments='{}',
                                                     name=item.get('tool', 'exec_command')))
    send(dict(method=method, params=dict(threadId=thread, turnId=turn, **params)))
    if raw and method == 'item/completed':
        event('rawResponseItem/completed', item=dict(type='function_call_output', call_id=item['id'], output=''))


class Tools:
    def __init__(self):
        server = config['mcp_servers']['lab']
        self.process = subprocess.Popen([server['command'], *server['args']], env={},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr)
        self.serial = 0
        self.request('initialize', dict(protocolVersion='2024-11-05', capabilities={}, clientInfo=dict(name='script', version='1')))
        self.receive()
        self.process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        self.process.stdin.flush()
        self.request('tools/list', {})
        self.tools = self.receive()['result']['tools']

    def request(self, method, params):
        self.serial += 1
        self.process.stdin.write((json.dumps(dict(jsonrpc='2.0', id=self.serial, method=method, params=params)) + '\n').encode())
        self.process.stdin.flush()

    def receive(self):
        response = self.process.stdout.readline()
        if not response:
            raise RuntimeError('lab tool ended before its MCP response')
        return json.loads(response)

    def start(self, item):
        event('item/started', item=item)
        self.request('tools/call', dict(name=item['tool'], arguments=item['arguments']))

    def complete(self, item):
        response = self.receive()
        assert 'error' not in response, response
        item = dict(item, status='completed', result=response['result'])
        event('item/completed', item=item)
        return item

    def close(self):
        self.process.stdin.close()
        self.process.wait()
        self.process.stdout.close()


def wait_record(path, predicate):
    """Observe a runner record without a timer or polling its files repeatedly."""
    libc = ctypes.CDLL(None, use_errno=True)
    fd = libc.inotify_init1(os.O_CLOEXEC)
    if fd < 0:
        raise OSError(ctypes.get_errno(), 'inotify_init1')
    try:
        if libc.inotify_add_watch(fd, os.fsencode(path), 2) < 0:  # IN_MODIFY
            raise OSError(ctypes.get_errno(), 'inotify_add_watch')
        while True:
            for line in path.read_text().splitlines():
                if predicate(json.loads(line)):
                    return
            if not select.select([fd], [], [], 3)[0]:
                raise AssertionError('runner did not record the call')
            os.read(fd, 65536)
    finally:
        os.close(fd)


def call_item(entry, number):
    return dict(id='item-' + str(number), **entry)


def play():
    for snapshot in script.get('windows', []):
        event('account/rateLimits/updated', rateLimits=snapshot)
    pending = []
    for number, entry in enumerate(script.get('items', [])):
        if entry.get('early'):
            item = dict(entry)
            del item['early']
            tools.complete(call_item(item, number))
    for number, entry in enumerate(script.get('items', [])):
        item = dict(entry)
        argv, wait = item.pop('argv', None), item.pop('wait', False)
        early, trigger = item.pop('early', False), item.pop('trigger', False)
        item.setdefault('id', 'item-' + str(number))
        if early:
            continue
        if item['type'] == 'raw':
            event('rawResponseItem/completed', item=item['item'])
            continue
        if item['type'] == 'commandExecution':
            item.update(command=' '.join(argv or []), cwd=work, status='inProgress')
        if item['type'] == 'mcpToolCall' and item.get('server') == 'lab':
            tools.start(item)
            if wait:
                pending.append(item)
                if not trigger:
                    continue
            else:
                tools.complete(item)
        else:
            event('item/started', item=item)
            if argv is not None:
                result = subprocess.run(argv, cwd=work, stdin=subprocess.DEVNULL, capture_output=True, check=False)
                item.update(status='completed', exitCode=result.returncode,
                            aggregatedOutput=result.stdout.decode('utf-8') + result.stderr.decode('utf-8'))
            if wait:
                return False
            event('item/completed', item=item)
        if trigger:
            # The call's view signals readiness, including its detached descendant,
            # before another action crosses the driver's bound.
            connections = [listener.accept()[0], listener.accept()[0]]
            assert {c.recv(32) for c in connections} == {b'parent', b'child'}
            if not script.get('command_timeout'):
                event('item/started', item=dict(id='past-bound', type='commandExecution', command='true', cwd=work))
            assert all(c.recv(1) == b'' for c in connections)
            for connection in connections:
                connection.close()
            Path(__file__).with_suffix('.ended').write_text('view and detached descendant ended')
            return False
    for item in pending:
        tools.complete(item)
    for usage in script.get('usage', []):
        event('thread/tokenUsage/updated', tokenUsage=dict(last=usage, total=usage, modelContextWindow=None))
    report = script.get('report', dict(done=True, retried=[], unseen=[], surprises=[]))
    if report is not None:
        event('item/started', item=dict(type='agentMessage', id='final', text=''))
        event('item/completed', item=dict(type='agentMessage', id='final', text=report if type(report) is str else json.dumps(report)))
    event('turn/completed', turn=dict(id=turn, status='completed', items=[]))
    return True


tools = None
listener = None
if script.get('endpoint'):
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(script['endpoint'])
    listener.listen(2)
    listener.settimeout(3)


for line in sys.stdin:
    request = json.loads(line)
    method, params = request.get('method'), request.get('params', {})
    if script.get('rate_read'):
        assert method in ('initialize', 'initialized', 'account/rateLimits/read'), method
        with Path(__file__).with_suffix('.requests').open('a') as log:
            log.write(json.dumps(request) + '\n')
    if 'id' not in request:
        assert method == 'initialized'
        continue
    if script.get('rpc_error') == method:
        send(dict(id=request['id'], error=dict(code=-32603, message='scripted host error')))
        continue
    if method == 'initialize':
        assert params['capabilities']['experimentalApi']
        result = dict(userAgent='scripted/0.162.1')
    elif method == 'account/rateLimits/read':
        if script.get('rate_exit'):
            raise SystemExit(3)
        if script.get('rate_hang'):
            select.select([], [], [])
        assert script.get('rate_read') and params == {}
        result = dict(rateLimits=script['rate_read'])
    elif method == 'thread/start':
        assert params == dict(cwd=work, permissions='lab', ephemeral=True, experimentalRawEvents=True,
                              approvalPolicy='never', config={})
        tools = Tools()
        for number, item in enumerate(script.get('items', [])):
            if item.get('early'):
                early = dict(item)
                del early['early']
                early = call_item(early, number)
                tools.start(early)
                wait_record(Path(work).parents[1] / 'events.jsonl', lambda r: r['event'] == 'driver_call')
                assert not list((Path(work).parent / 'calls').iterdir())
                assert not select.select([tools.process.stdout], [], [], 0)[0]
                Path(__file__).with_suffix('.held').write_text('call held before inventory')
        result = dict(thread=dict(id=thread, turns=[]), activePermissionProfile=dict(id='lab', extends=None), instructionSources=[])
    elif method == 'mcpServerStatus/list':
        server = dict(name='lab', runtimeStatus='connected', toolsError=None,
                      tools={item['name']: item for item in tools.tools},
                      resources=[], resourceTemplates=[])
        result = dict(data=[server], nextCursor=None)
        if script.get('pages') and 'cursor' not in params:
            result = dict(data=[], nextCursor='second')
    elif method == 'app/installed':
        result = dict(apps=[])
    elif method == 'plugin/list':
        result = dict(marketplaces=[], marketplaceLoadErrors=[], featuredPluginIds=[])
    elif method in ('hooks/list', 'skills/list'):
        row = dict(cwd=work, errors=[])
        row['hooks' if method == 'hooks/list' else 'skills'] = []
        if method == 'hooks/list':
            row['warnings'] = []
        result = dict(data=[row])
    elif method == 'config/read':
        assert params['includeLayers']
        result = dict(layers=[dict(name=dict(type='user', file=str(config_path)), config=config),
                             dict(name=dict(type='system', file='/etc/codex/config.toml'), config={})])
    elif method == 'turn/start':
        assert params['threadId'] == thread
        assert params['outputSchema']['required'] == ['done', 'retried', 'unseen', 'surprises']
        assert 'charter' in params['input'][0]['text']
        Path(__file__).with_suffix('.turn').write_text(json.dumps(params))
        send(dict(id=request['id'], result=dict(turn=dict(id=turn, status='inProgress', items=[]))))
        event('turn/started', turn=dict(id=turn, status='inProgress', items=[]))
        play()
        continue
    else:
        raise AssertionError(method)
    override = script.get('inventory', {}).get(method, {})
    result.update(override)
    send(dict(id=request['id'], result=result))

if tools is not None:
    tools.close()
if listener is not None:
    listener.close()
