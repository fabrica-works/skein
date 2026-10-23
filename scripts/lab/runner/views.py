"""Tool views, permission profiles and wire decisions (lab.md, 6.2–6.3)."""
import json
import re
from . import canonical


SYSTEM_ROOTS = ('/usr', '/bin', '/sbin', '/lib', '/lib64', '/etc')


def profile_toml(codex_executable: str, source: str | None, work: str) -> str:
    roots = {':minimal': 'read', codex_executable: 'read'}
    if source is not None:
        roots[source] = 'read'
    roots[work] = 'write'
    return ('default_permissions = "lab"\napproval_policy = "never"\n'
            '[permissions.lab.filesystem]\n'
            + ''.join(json.dumps(path, ensure_ascii=False) + ' = ' + json.dumps(access) + '\n'
                      for path, access in roots.items())
            + '[permissions.lab.network]\nenabled = false\n')


def tool_argv(bwrap, tool, work, args, system_roots):
    """Mount parents before children so narrower grants take precedence."""
    mounts = {path: '--ro-bind' for path in system_roots}
    mounts[tool['argv'][0]] = '--ro-bind'
    mounts.update({path: '--ro-bind' for path in tool['reads']})
    mounts[work] = '--ro-bind'
    mounts.update({path: '--bind' for path in tool['writes']})
    argv = [bwrap, '--unshare-user', '--unshare-pid', '--share-net', '--die-with-parent',
            '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp']
    for path in sorted(mounts, key=lambda p: (p.count('/'), p)):
        argv.extend([mounts[path], path, path])
    argv.extend(['--chdir', work, '--clearenv'])
    for name, value in sorted(tool['env'].items()):
        argv.extend(['--setenv', name, value])
    # Bubblewrap supplies PWD itself; env removes that implicit grant before exec.
    environment = [name + '=' + value for name, value in sorted(tool['env'].items())]
    return argv + ['--', '/usr/bin/env', '-i', '--', *environment, *tool['argv'], *args]


def _scalar_text(value):
    return type(value) is str and not any(0xd800 <= ord(c) <= 0xdfff for c in value)


def _text(value):
    return _scalar_text(value) and '\0' not in value


def _absolute(value):
    return (_text(value) and value.startswith('/') and value != '/'
            and all(part not in ('', '.', '..') for part in value[1:].split('/')))


def tools_record(data, run):
    value = json.loads(data)
    if type(data) is not bytes or canonical.record_bytes(value) != data:
        raise ValueError('expected canonical driver/tools.json')
    if (type(value) is not dict
            or set(value) != {'format', 'run', 'work', 'source', 'socket', 'tools'}
            or value.get('format') != 'lab-driver-tools/1'
            or value.get('run') != run or not _absolute(value.get('work'))
            or not _absolute(value.get('socket'))
            or value.get('source') is not None and not _absolute(value['source'])
            or type(value.get('tools')) is not list):
        raise ValueError('invalid driver/tools.json')
    names = set()
    for tool in value['tools']:
        if (type(tool) is not dict or set(tool) != {'name', 'argv', 'env', 'writes', 'reads'}
                or not _text(tool['name']) or not re.fullmatch(r'[A-Za-z0-9_-]+', tool['name'])
                or tool['name'] in names
                or type(tool['argv']) is not list or not tool['argv']
                or not all(_text(arg) for arg in tool['argv']) or not _absolute(tool['argv'][0])
                or type(tool['env']) is not dict
                or not all(_text(k) and k and '=' not in k and _text(v) for k, v in tool['env'].items())
                or any(type(tool[key]) is not list or not all(_absolute(p) for p in tool[key])
                       for key in ('reads', 'writes'))):
            raise ValueError('invalid tool entry')
        names.add(tool['name'])
    return value


def rpc_error(identity, code, message):
    return dict(jsonrpc='2.0', id=identity, error=dict(code=code, message=message))


def mcp_request(line, tools):
    """Return (response, call); a call awaits broker admission before its response."""
    try:
        message = json.loads(line)
    except (ValueError, UnicodeError):
        return rpc_error(None, -32700, 'invalid JSON'), None
    if (type(message) is not dict or message.get('jsonrpc') != '2.0'
            or type(message.get('method')) is not str
            or 'id' in message and type(message['id']) not in (str, int, type(None))):
        return rpc_error(None, -32600, 'invalid request'), None
    identity = message.get('id')
    if 'id' not in message:
        return None, None
    method, params = message['method'], message.get('params', {})
    if type(params) is not dict:
        return rpc_error(identity, -32602, 'expected object params'), None
    if method == 'initialize':
        result = dict(protocolVersion='2024-11-05', capabilities=dict(tools={}),
                      serverInfo=dict(name='lab', version='1'))
    elif method == 'tools/list':
        result = dict(tools=[dict(name=t['name'], inputSchema=dict(type='object',
            properties=dict(args=dict(type='array', items=dict(type='string')),
                            stdin=dict(type=['string', 'null'])), required=['args'], additionalProperties=False))
                            for t in tools])
    elif method == 'tools/call':
        arguments = params.get('arguments', {})
        if (not any(t['name'] == params.get('name') for t in tools)
                or type(arguments) is not dict or set(arguments) - {'args', 'stdin'}
                or type(arguments.get('args')) is not list
                or not all(_text(a) for a in arguments['args'])
                or arguments.get('stdin') is not None and not _scalar_text(arguments['stdin'])):
            return rpc_error(identity, -32602, 'unknown tool or invalid arguments'), None
        return None, dict(id=identity, tool=params['name'], args=arguments['args'], stdin=arguments.get('stdin'))
    else:
        return rpc_error(identity, -32601, 'unknown method'), None
    return dict(jsonrpc='2.0', id=identity, result=result), None


def broker_start(call):
    return dict(start=dict(tool=call['tool'], args=call['args'], stdin=call['stdin'] is not None))


def broker_reply(line):
    value = json.loads(line)
    if type(value) is dict:
        if set(value) == {'go'} and type(value['go']) is int and value['go'] >= 0:
            return value
        if set(value) == {'refuse'} and _text(value['refuse']):
            return value
    raise ValueError('invalid broker admission')


def call_result(identity, result, *, failed=False):
    return dict(jsonrpc='2.0', id=identity,
                result=dict(content=[dict(type='text', text=json.dumps(result, ensure_ascii=False))], isError=failed))
