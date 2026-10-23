"""Pure lab-script/1 decisions and the Codex Responses wire format (lab.md, 6.6)."""
import json
import re
from . import canonical
from .records import NAME, Refusal, Refused, shape


TOOL = re.compile(r'[A-Za-z0-9_.-]+\Z')
CAPTURE = re.compile(r'\{capture\.([a-z][a-z0-9-]*)\}')


def parse(data: bytes) -> dict:
    value = canonical.loads(data)
    errors = []
    obj = shape(value, ('format', 'turns'), (), 'script', errors)
    if obj.get('format') != 'lab-script/1':
        raise Refused('script.format', 'expected lab-script/1')
    if type(obj.get('turns')) is not list:
        raise Refused('script.turns', 'expected a list')
    for n, turn in enumerate(obj['turns']):
        member = f'script.turns.{n}'
        turn = shape(turn, ('output',), ('capture',), member, errors)
        captures = turn.get('capture', {})
        if type(captures) is not dict:
            errors.append(Refusal(member + '.capture', 'expected an object'))
        else:
            for name, regex in captures.items():
                m = member + '.capture.' + name
                if not NAME.fullmatch(name):
                    raise Refused(m, 'invalid capture name')
                try:
                    pattern = re.compile(regex) if type(regex) is str else None
                except re.error as error:
                    raise Refused(m, 'invalid regex: ' + str(error)) from error
                if pattern is None or pattern.groups < 1:
                    raise Refused(m, 'expected a regex with a capture group')
        if type(turn.get('output')) is not list:
            raise Refused(member + '.output', 'expected a list')
        for i, item in enumerate(turn['output']):
            m = f'{member}.output.{i}'
            if type(item) is dict and 'text' in item:
                item = shape(item, ('text',), (), m, errors)
                if type(item.get('text')) is not str:
                    raise Refused(m + '.text', 'expected text')
            else:
                item = shape(item, ('call', 'namespace', 'arguments'), (), m, errors)
                for field in ('call', 'namespace'):
                    text = item.get(field)
                    if field == 'namespace' and text is None:
                        continue
                    if type(text) is not str or not TOOL.fullmatch(text):
                        raise Refused(m + '.' + field, 'expected a tool name')
                if type(item.get('arguments')) is not dict:
                    raise Refused(m + '.arguments', 'expected an object')
    if errors:
        first = errors[0]
        raise Refused(first.member, first.why)
    return value


def _latest_outputs(request):
    items = request.get('input', [])
    last = -1
    for n, item in enumerate(items):
        if item.get('role') == 'assistant' or item.get('type') in ('function_call', 'custom_tool_call'):
            last = n
    return '\n'.join(item['output'] if type(item.get('output')) is str else json.dumps(item.get('output'))
                     for item in items[last + 1:] if item.get('type') in ('function_call_output', 'custom_tool_call_output'))


def respond(script, turn: int, request: dict, captured: dict):
    captured = dict(captured)
    if turn >= len(script['turns']) or turn < 0:
        return [dict(text=f'script: no turn {turn}')], captured
    entry = script['turns'][turn]
    text = _latest_outputs(request)
    for name, regex in entry.get('capture', {}).items():
        match = re.search(regex, text)
        if match is None or match.group(1) is None:
            return [dict(text=f'script: capture {name} missed at turn {turn}')], captured
        captured[name] = match.group(1)

    def substitute(value):
        if type(value) is str:
            return CAPTURE.sub(lambda m: captured[m.group(1)], value)
        if type(value) is list:
            return [substitute(v) for v in value]
        if type(value) is dict:
            return {substitute(k): substitute(v) for k, v in value.items()}
        return value
    try:
        return substitute(entry['output']), captured
    except KeyError as error:
        return [dict(text=f'script: capture {error.args[0]} missed at turn {turn}')], captured


def provisioning(tools: list[dict]) -> dict:
    turns = [dict(output=[dict(call=t['name'], namespace='mcp__lab',
                              arguments=dict(args=t['provision']['args'], stdin=None))]) for t in tools]
    turns.append(dict(output=[dict(text=json.dumps(dict(done=True, retried=[], unseen=[], surprises=[])))]))
    return dict(format='lab-script/1', turns=turns)


def offered(request: dict) -> list[str]:
    def names(tools, namespace=None):
        result = []
        for tool in tools:
            if tool.get('type') == 'namespace':
                inner = None if tool['name'] == 'functions' else tool['name']
                result.extend(names(tool.get('tools', []), inner))
            else:
                name = tool.get('name', tool.get('type'))
                if type(name) is str:
                    result.append((namespace + '.' if namespace else '') + name)
        return result
    # A pinned catalog's model is offered its tools as `additional_tools` input
    # items, the plain functions under the `functions` namespace.
    tools = list(request.get('tools', []))
    for item in request.get('input', []):
        if type(item) is dict and item.get('type') == 'additional_tools':
            tools.extend(item.get('tools', []))
    return sorted(set(names(tools)))


def admitted(offered, tools) -> list[str]:
    # Observed in the Codex 0.162.1 request fixture; lab.md 6.1 admits this surface.
    allowed = {'exec_command', 'write_stdin', 'list_mcp_resources',
               'list_mcp_resource_templates', 'read_mcp_resource'}
    allowed.update('mcp__lab.' + tool['name'] for tool in tools)
    return sorted(set(offered) - allowed)


def stream(items, response_id, usage) -> bytes:
    output = []
    for n, item in enumerate(items):
        identity = response_id + '_' + str(n)
        if 'call' in item:
            value = dict(type='function_call', id='fc_' + identity, call_id='call_' + identity,
                         name=item['call'], arguments=json.dumps(item['arguments']))
            if item['namespace'] is not None:
                value['namespace'] = item['namespace']
        else:
            value = dict(type='message', role='assistant', id='msg_' + identity,
                         content=[dict(type='output_text', text=item['text'], annotations=[])])
        output.append(value)
    events = [dict(type='response.created', response=dict(id=response_id))]
    events.extend(dict(type='response.output_item.done', item=item, output_index=n) for n, item in enumerate(output))
    events.append(dict(type='response.completed', response=dict(id=response_id, usage=usage)))
    return b''.join(b'event: ' + e['type'].encode() + b'\ndata: ' + json.dumps(e).encode() + b'\n\n' for e in events)
