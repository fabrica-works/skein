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
import urllib.request

os.umask(0o077)
script = json.loads(Path(__file__).with_suffix('.json').read_text())
if sys.argv[1:3] == ['sandbox', '-P']:
    # The stand-in records the sandbox contract; native probe stories use Codex itself.
    profile = (Path(os.environ['CODEX_HOME']) / 'config.toml').read_text()
    Path(__file__).with_suffix('.probe').write_text(json.dumps(dict(argv=sys.argv[1:], profile=profile)))
    json.load(sys.stdin)
    checks = ('canary_read', 'home_list', 'source_write', 'network', 'pid_namespace', 'canary_process', 'work_write')
    probe_script = script.get('triage', {}) if tomllib.loads(profile)['default_permissions'] == 'triage' else script
    print(json.dumps({name: name != probe_script.get('probe_failure') for name in checks}), flush=True)
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
responses_url = config.get('model_providers', {}).get('lab-openai', {}).get('base_url')
provisioning = config_path.parent.parent.name == 'provision'
triaging = config.get('default_permissions') == 'triage'
if provisioning:
    script = script.get('provision', {})
elif triaging:
    script = script.get('triage', {})
work = os.getcwd()
thread, turn = 'script-thread', 'script-turn'
previous_usage = {}


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


def usage_event(total):
    """Scripts supply thread totals; last describes only the latest request."""
    global previous_usage
    last = {name: value - previous_usage.get(name, 0)
            if value is not None and previous_usage.get(name, 0) is not None else None
            for name, value in total.items()}
    event('thread/tokenUsage/updated', tokenUsage=dict(last=last, total=total, modelContextWindow=None))
    previous_usage = total


class Tools:
    def __init__(self):
        server = config['mcp_servers']['lab']
        # Codex 0.162.1 refuses an unannotated MCP call under approval policy never otherwise.
        assert server['default_tools_approval_mode'] == 'approve'
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


def play_responses():
    offered = [dict(type='function', name=name, parameters=dict(type='object', properties={}))
               for name in ('exec_command', 'write_stdin', 'list_mcp_resources',
                            'list_mcp_resource_templates', 'read_mcp_resource')]
    offered.append(dict(type='namespace', name='mcp__lab', tools=[dict(type='function', name=t['name'],
                        parameters=t['inputSchema']) for t in tools.tools]))
    offered.extend(dict(type='function', name=name) for name in script.get('offered', []))
    history = []
    for number in range(100):
        body = json.dumps(dict(model=config['model'], tools=offered, input=history)).encode()
        request = urllib.request.Request(responses_url + '/responses', data=body,
                                         headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(request) as response:
            events = [json.loads(line[6:]) for line in response.read().decode().splitlines() if line.startswith('data: ')]
        output = [e['item'] for e in events if e['type'] == 'response.output_item.done']
        history.extend(output)
        usage_event(dict(inputTokens=100 * (number + 1), cachedInputTokens=0, outputTokens=10 * (number + 1)))
        for entry in output:
            if entry['type'] == 'message':
                text = ''.join(c['text'] for c in entry['content'])
                event('item/started', item=dict(type='agentMessage', id=entry['id'], text=''))
                event('item/completed', item=dict(type='agentMessage', id=entry['id'], text=text))
                event('turn/completed', turn=dict(id=turn, status='completed', items=[]))
                return True
            args = json.loads(entry['arguments'])
            if entry.get('namespace') == 'mcp__lab':
                item = dict(id=entry['call_id'], type='mcpToolCall', server='lab', tool=entry['name'], arguments=args)
                tools.start(item)
                completed = tools.complete(item)
                text = '\n'.join(c['text'] for c in completed['result']['content'] if c['type'] == 'text')
            elif entry['name'] == 'exec_command':
                item = dict(id=entry['call_id'], type='commandExecution', command=args['cmd'], cwd=work, status='inProgress')
                event('item/started', item=item)
                result = subprocess.run(['/bin/sh', '-c', args['cmd']], cwd=work, env=dict(PATH='/usr/bin:/bin'),
                                        stdin=subprocess.DEVNULL, capture_output=True)
                text = result.stdout.decode() + result.stderr.decode()
                event('item/completed', item=dict(item, status='completed', exitCode=result.returncode, aggregatedOutput=text))
            else:
                event('rawResponseItem/completed', item=entry)
                event('turn/completed', turn=dict(id=turn, status='failed', items=[]))
                return False
            history.append(dict(type='function_call_output', call_id=entry['call_id'], output=text))
    raise RuntimeError('script did not end')


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
    if script.get('wait_after_actions'):
        return False
    for usage in script.get('usage', []):
        usage_event(usage)
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
        assert params == dict(cwd=work, permissions='triage' if triaging else 'lab', ephemeral=True, experimentalRawEvents=True,
                              approvalPolicy='never', config={})
        tools = None if triaging else Tools()
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
        result = dict(thread=dict(id=thread, turns=[]), activePermissionProfile=dict(id='triage' if triaging else 'lab', extends=None), instructionSources=[])
        if triaging and 'before_inventory' in script:
            event('item/completed', item=dict(type='agentMessage', id='early', text=json.dumps(script['before_inventory'])))
            event('turn/completed', turn=dict(id=turn, status='completed', items=[]))
    elif method == 'mcpServerStatus/list':
        server = dict(name='lab', runtimeStatus='connected', toolsError=None,
                      tools={item['name']: item for item in tools.tools},
                      resources=[], resourceTemplates=[]) if tools is not None else None
        result = dict(data=[] if triaging else [server], nextCursor=None)
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
        if triaging:
            assert params['outputSchema']['required'] == ['failures']
            supplied = json.loads(params['input'][0]['text'].rsplit('\n\n', 1)[1])
            proposals = []
            for failure in supplied['failures']:
                match = script.get('match_use') if failure['kind'] == 'use' else script.get('match_nonuse')
                proposals.append(dict(id=failure['id'], match=match,
                    signature='use/' + failure['id'] if failure['kind'] == 'use' and match is None else None,
                    title='Scripted proposal', reason='Retained evidence',
                    **{'class': None if match is not None and failure['kind'] == 'use' else 'tools'}))
            script['report'] = script.get('output', dict(failures=proposals))
        else:
            assert params['outputSchema']['required'] == ['done', 'retried', 'unseen', 'surprises']
            assert 'charter' in params['input'][0]['text']
        if not provisioning:
            Path(__file__).with_suffix('.turn').write_text(json.dumps(params))
        send(dict(id=request['id'], result=dict(turn=dict(id=turn, status='inProgress', items=[]))))
        event('turn/started', turn=dict(id=turn, status='inProgress', items=[]))
        play_responses() if responses_url else play()
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
