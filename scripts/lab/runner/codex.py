"""Codex 0.162.1 decisions over supplied configuration and events (lab.md, 6)."""
import json
from . import canonical, judge, views
from .records import Refusal, Refused, SAFE


DISABLED = ('apps', 'plugins', 'remote_plugin', 'recommended_plugins', 'hooks',
            'browser_use', 'browser_use_external', 'browser_use_full_cdp_access',
            'in_app_browser', 'computer_use', 'image_generation', 'view_image',
            'multi_agent', 'multi_agent_v2', 'skill_mcp_dependency_install',
            'skill_search', 'code_mode', 'code_mode_host', 'sleep_tool', 'goals',
            'tool_suggest', 'shell_snapshot', 'api_key_model_discovery', 'workspace_dependencies')
NON_ACTIONS = ('userMessage', 'agentMessage', 'reasoning', 'plan', 'contextCompaction')
# tool_mode: code mode; multi_agent_version: collaboration tools; experimental_supported_tools:
# clock and asynchronous questions; apply_patch_tool_type: apply_patch; supports_search_tool:
# MCP tools deferred behind a tool search the host does not serve.
CATALOG_FIELDS = (('tool_mode', None), ('multi_agent_version', None),
                  ('experimental_supported_tools', []), ('apply_patch_tool_type', None),
                  ('supports_search_tool', False))
# Codex adds these whenever an MCP server is configured; each maps to its listing's field,
# or None for a read. The inventory holds lab's resources and templates empty (lab.md 6.1).
RESOURCE_TOOLS = {'list_mcp_resources': 'resources', 'list_mcp_resource_templates': 'resourceTemplates',
                  'read_mcp_resource': None}
# Raw response items (experimentalRawEvents): every other kind is a model call that an item must answer.
RAW_MESSAGES = ('message', 'reasoning')
RAW_OUTPUTS = ('function_call_output', 'custom_tool_call_output')
INVENTORY_METHODS = ('mcpServerStatus/list', 'app/installed', 'plugin/list',
                     'hooks/list', 'skills/list', 'config/read')


def _evidence(value):
    try:
        canonical.dumps(value)
        return value
    except Refused:
        return {'json': json.dumps(value, ensure_ascii=True, allow_nan=False)}


def _response(responses, method):
    value = responses.get(method)
    return value if type(value) is dict else {}


def config_toml(python: str, runner: str, run: str, tools: list[str], model: str,
                effort: str, profile: str, lab_root: str = '{lab.root}', catalog: str | None = None) -> str:
    """Permission profile text is supplied by the view builder."""
    # Root keys precede every TOML table, including the profile's tables.
    # No base_url: Codex then picks the endpoint for the login, API key or ChatGPT.
    return ('model_provider = "lab-openai"\n'
            + (f'model_catalog_json = {json.dumps(catalog)}\n' if catalog is not None else '')
            + f'model = {json.dumps(model)}\nmodel_reasoning_effort = {json.dumps(effort)}\n'
            'web_search = "disabled"\nallow_login_shell = false\nproject_doc_max_bytes = 0\n'
            + profile + '\n[shell_environment_policy]\ninherit = "none"\n[features]\n'
            + ''.join(f'{name} = false\n' for name in DISABLED)
            + 'skip_host_skill_discovery = true\n[mcp_servers.lab]\n'
            + f'command = {json.dumps(python)}\n'
            + 'args = ' + json.dumps(['-I', '-B', runner + '/runner/launch.py', lab_root, 'tool', run]) + '\n'
            + 'env_vars = []\nrequired = true\nenabled_tools = ' + json.dumps(sorted(tools)) + '\n'
            # The host's own MCP approval is pre-granted; the runner's broker admits each call (lab.md 6.3).
            + 'default_tools_approval_mode = "approve"\n'
            + '[skills.bundled]\nenabled = false\n[tools.experimental_request_user_input]\nenabled = false\n'
            + '[model_providers.lab-openai]\n'
            + 'name = "OpenAI"\nwire_api = "responses"\n'
            + 'requires_openai_auth = true\nsupports_websockets = false\n')


def configuration(python: str, runner: str, program: str, tools: list[str], model: str,
                  effort: str, source: bool, schema: dict, catalog: str | None = None) -> dict:
    """The key uses template names for every path belonging to a run."""
    profile = views.profile_toml(program, '{source}' if source else None, '{driver.work}')
    return dict(argv=['app-server', '--stdio', '--strict-config'],
                toml=config_toml(python, runner, '{run.id}', tools, model, effort, profile, catalog=catalog),
                inventory=expected_inventory(tools, '{codex.home}/config.toml'),
                schema=schema)


def catalog_refusals(data: bytes, model: str) -> list:
    """The pinned catalog must offer the model its tools directly: code mode makes no items."""
    member = 'settings.driver.catalog'
    try:
        value = json.loads(data)
    except (UnicodeDecodeError, ValueError):
        return [Refusal(member, 'catalog is not JSON')]
    models = value.get('models') if type(value) is dict else None
    if type(models) is not list:
        return [Refusal(member, 'catalog has no models list')]
    entries = [m for m in models if type(m) is dict and m.get('slug') == model]
    if len(entries) != 1:
        return [Refusal(member, f'catalog must hold the driver model {model!r} once')]
    # Each field adds built-in tools outside the audit's admitted actions (lab.md 6.1).
    return [Refusal(member, f'the driver model\'s {field} must be {json.dumps(value)}')
            for field, value in CATALOG_FIELDS
            if field not in entries[0] or entries[0][field] != value or type(entries[0][field]) is not type(value)]


def expected_inventory(tools: list[str], config_path: str) -> dict:
    return dict(permission_profile='lab', mcp_servers_and_tools={'lab': sorted(tools)},
                resources=[], prompts=[], apps=[], plugins=[], hooks=[], skills=[],
                configuration_layers=config_path, instruction_files=[], built_in_tools=['commandExecution'])


def observe(responses: dict, expected: dict | None = None) -> dict:
    """Missing/malformed observers mismatch; unavailable observers stay unobserved."""
    expected = expected if expected is not None else responses['expected']
    result = {}
    def row(name, evidence, ok):
        result[name] = dict(state='observed' if ok else 'mismatch', evidence=_evidence(evidence))
    for name in ('prompts', 'plugins', 'built_in_tools'):
        result[name] = dict(state='unobserved', evidence='no complete live observer in Codex 0.162.1')
    thread = _response(responses, 'thread/start')
    row('permission_profile', thread.get('activePermissionProfile'),
        type(thread.get('activePermissionProfile')) is dict
        and thread['activePermissionProfile'].get('id') == expected['permission_profile'])
    row('instruction_files', thread.get('instructionSources'), thread.get('instructionSources') == [])
    pages = responses.get('mcpServerStatus/list')
    valid = (type(pages) is list and bool(pages)
             and all(type(p) is dict and type(p.get('data')) is list and 'nextCursor' in p for p in pages)
             and all(type(p['nextCursor']) is str and bool(p['nextCursor']) for p in pages[:-1])
             and pages[-1]['nextCursor'] is None) if type(pages) is list else False
    entries = [e for p in pages for e in p['data']] if valid else []
    good = (valid and len(entries) == 1 and type(entries[0]) is dict
            and entries[0].get('name') == 'lab' and entries[0].get('runtimeStatus') == 'connected'
            and 'toolsError' in entries[0] and entries[0]['toolsError'] is None
            and type(entries[0].get('tools')) is dict
            and sorted(entries[0]['tools']) == expected['mcp_servers_and_tools']['lab'])
    row('mcp_servers_and_tools', pages, good)
    row('resources', pages, valid and bool(entries) and all(type(e) is dict
        and e.get('resources') == [] and e.get('resourceTemplates') == [] for e in entries))
    apps, plugins = _response(responses, 'app/installed'), _response(responses, 'plugin/list')
    # An app both disabled and not callable offers the model nothing; the entries stay as evidence.
    row('apps', apps, type(apps.get('apps')) is list and all(
        type(a) is dict and a.get('enabled') is False and a.get('callable') is False for a in apps['apps']))
    # The catalog is evidence of installed plugins, never a model-facing inventory.
    result['plugins']['evidence'] = dict(observer=_evidence(plugins), limitation='model-facing capabilities unobserved')
    if (plugins.get('marketplaces') != [] or plugins.get('marketplaceLoadErrors') != []):
        result['plugins']['state'] = 'mismatch'
    for name in ('hooks', 'skills'):
        response = _response(responses, name + '/list')
        entries = response.get('data')
        good = (type(entries) is list and bool(entries) and all(type(e) is dict
                and e.get(name) == [] and e.get('errors') == []
                and (name != 'hooks' or e.get('warnings') == []) for e in entries))
        row(name, response, good)
    response = _response(responses, 'config/read')
    layers = response.get('layers')
    good, users = type(layers) is list and bool(layers), 0
    for layer in layers if type(layers) is list else []:
        if type(layer) is not dict or type(layer.get('name')) is not dict or type(layer.get('config')) is not dict:
            good = False
            continue
        name = layer['name']
        if name.get('type') == 'user' and name.get('file') == expected['configuration_layers']:
            users += 1
            good = good and bool(layer['config']) and not layer.get('disabledReason')
        elif name.get('type') != 'system' or layer['config'] != {}:
            good = False
    row('configuration_layers', response, good and users == 1)
    return result


def item_kind(item: dict) -> str:
    kind = item.get('type')
    if kind == 'commandExecution' or kind == 'mcpToolCall' and (
            item.get('server') == 'lab' or item.get('server') == 'codex' and item.get('tool') in RESOURCE_TOOLS):
        return 'action'
    if kind in NON_ACTIONS:
        return 'message'
    raise Refused('driver/audit', 'unadmitted item kind: ' + str(kind))


def report(text: str) -> dict:
    """Validate the closed report shape in schemas/report.json (lab.md, 6.4)."""
    try:
        value = json.loads(text, object_pairs_hook=canonical._pairs, parse_constant=canonical._no_number)
    except (ValueError, TypeError) as error:
        raise Refused('driver/report', 'final agentMessage is not a report') from error
    if judge._report_state(value) == 'incomplete':
        raise Refused('driver/report', 'final agentMessage differs from report schema')
    canonical.dumps(value)
    return value


def initial() -> dict:
    return dict(actions=[], turns=[], report=None, report_error=None, final_message=None,
                tokens=dict(input=None, cached=None, output=None), overshoot=None, windows=None,
                calls=[], unhandled=None)


def _unhandled(state: dict, call_id, start, ms: int, why: str, end: dict | None = None) -> None:
    """A model call no item answers is still a counted action, and ends the run (lab.md 6.1)."""
    state['actions'].append(dict(n=len(state['actions']) + 1, kind='unhandled', started_ms=ms,
                                 completed_ms=None if end is None else ms, item_id=call_id,
                                 sources=['raw'], start=start, end=None if end is None else _evidence(end)))
    if state['unhandled'] is None:
        state['unhandled'] = why


def _answered(state: dict, call_id, unhandled: bool = False) -> bool:
    return any(a.get('item_id') == call_id and (unhandled or a['kind'] != 'unhandled') for a in state['actions'])


def raw_item(state: dict, item: dict, ms: int) -> None:
    """Pair each raw model call with the item that answers it; write_stdin answers a running command."""
    kind = item.get('type')
    if kind in RAW_MESSAGES:
        return
    call_id = item.get('call_id')
    calls = {c['call_id']: c for c in state['calls']}
    if kind in RAW_OUTPUTS:
        if call_id in calls and not _answered(state, call_id, True):
            _unhandled(state, call_id, calls[call_id]['item'], ms, 'unhandled call: ' + str(call_id), item)
        return
    if kind != 'function_call' or type(call_id) is not str or not call_id or call_id in calls:
        _unhandled(state, call_id if type(call_id) is str else None, _evidence(item), ms, 'unhandled call kind: ' + str(kind))
        return
    state['calls'].append(dict(call_id=call_id, item=_evidence(item)))
    if item.get('name') == 'write_stdin' and item.get('namespace') is None:
        try:
            session = json.loads(item.get('arguments')).get('session_id')
        except (TypeError, ValueError, AttributeError):
            session = None
        if type(session) is int and any(a['kind'] == 'commandExecution' and type(a['start']) is dict
                                        and a['start'].get('processId') == str(session) for a in state['actions']):
            state['actions'].append(dict(n=len(state['actions']) + 1, kind='write_stdin', started_ms=ms, completed_ms=None,
                                         item_id=call_id, sources=['raw'], start=_evidence(item), end=None))


def resource_refusal(item: dict) -> None:
    """A completed resource call must reach nothing: an empty list, or an error (lab.md 6.1)."""
    if item.get('server') != 'codex' or item['type'] != 'mcpToolCall':
        return
    if item.get('status') == 'failed':
        return
    field = RESOURCE_TOOLS[item['tool']]
    result = item.get('result')
    content = result.get('content') if type(result) is dict else None
    try:
        payload = json.loads(content[0]['text']) if (
            item.get('status') == 'completed' and field is not None and type(content) is list and len(content) == 1
            and type(content[0]) is dict and content[0].get('type') == 'text' and type(content[0].get('text')) is str) else None
    except ValueError:
        payload = None
    if not (type(payload) is dict and payload.get(field) == [] and set(payload) <= {field, 'server', 'nextCursor'}
            and payload.get('nextCursor') is None):
        raise Refused('driver/audit', item['tool'] + ' returned content')


def _call_key(item):
    arguments = item.get('arguments', {})
    if (type(arguments) is not dict or type(arguments.get('args', [])) is not list
            or any(type(a) is not str for a in arguments.get('args', []))
            or arguments.get('stdin') is not None and type(arguments['stdin']) is not str
            or type(item.get('tool')) is not str):
        raise Refused('driver/audit', 'invalid lab tool arguments')
    return dict(tool=item['tool'], args=arguments.get('args', []), stdin=arguments.get('stdin') is not None)


def action_start(state: dict, item: dict, ms: int, source: str = 'host') -> dict:
    """Pair identical calls in FIFO order across host and broker observations."""
    key = _call_key(item) if item['type'] == 'mcpToolCall' and item.get('server') == 'lab' else None
    if source == 'host':
        existing = next((a for a in state['actions'] if a.get('item_id') == item.get('id')
                         and a.get('item_id') is not None), None)
        if existing is not None:
            if existing['kind'] != item['type']:
                raise Refused('driver/audit', 'item id reused for another action kind')
            return existing
    existing = next((a for a in state['actions'] if key is not None and a.get('call') == key
                     and source not in a['sources']), None)
    if existing is None:
        existing = dict(n=len(state['actions']) + 1, kind=item['type'], started_ms=ms,
                        completed_ms=None, item_id=None, sources=[], start=_evidence(item), end=None)
        if key is not None:
            existing['call'] = key
        state['actions'].append(existing)
    existing['sources'].append(source)
    if source == 'host':
        existing['item_id'] = item.get('id')
    return existing


def fold(state: dict, message: dict, ms: int) -> dict:
    """Fold one app-server notification; repeated usage replaces that turn's counts."""
    state = json.loads(canonical.dumps(state))
    method, params = message.get('method'), message.get('params', {})
    if method in ('item/started', 'item/completed'):
        item = params['item']
        category = item_kind(item)
        if category == 'action':
            action = action_start(state, item, ms)
            if method == 'item/completed':
                action.update(completed_ms=ms, end=_evidence(item))
                resource_refusal(item)
        elif method == 'item/completed' and item['type'] == 'agentMessage':
            state['final_message'] = item.get('text')
    elif method == 'rawResponseItem/completed':
        raw_item(state, params['item'], ms)
    elif method in ('turn/started', 'turn/completed', 'thread/tokenUsage/updated'):
        turn_id = params.get('turnId') if method == 'thread/tokenUsage/updated' else params['turn']['id']
        turn = next((t for t in state['turns'] if t['id'] == turn_id), None)
        if turn is None:
            turn = dict(id=turn_id, started_ms=None, completed_ms=None,
                        tokens=dict(input=None, cached=None, output=None))
            state['turns'].append(turn)
        if method == 'turn/started':
            turn['started_ms'] = ms
        elif method == 'thread/tokenUsage/updated':
            usage = params.get('tokenUsage', {}).get('last') or {}
            for name, field in (('input', 'inputTokens'), ('cached', 'cachedInputTokens'), ('output', 'outputTokens')):
                value = usage.get(field)
                if value is not None and (type(value) is not int or value < 0):
                    raise Refused('driver/usage', 'invalid token count')
                turn['tokens'][name] = value
            for name in state['tokens']:
                values = [t['tokens'][name] for t in state['turns']]
                state['tokens'][name] = sum(values) if all(v is not None for v in values) else None
        else:
            turn.update(completed_ms=ms, status=params['turn'].get('status'))
            for call in state['calls']:
                if not _answered(state, call['call_id'], True):
                    _unhandled(state, call['call_id'], call['item'], ms, 'unanswered call: ' + call['call_id'])
            if state['unhandled'] is None and any('raw' not in a['sources'] for a in state['actions']) and not state['calls']:
                state['unhandled'] = 'actions without raw response items'
            if turn['status'] != 'completed':
                raise Refused('driver/turn', 'turn did not complete: ' + str(turn['status']))
            try:
                state['report'] = report(state['final_message'])
            except Refused as error:
                state['report_error'] = str(error)
    elif method == 'account/rateLimits/updated':
        state['windows'] = windows(params['rateLimits'])
    elif method == 'error':
        raise Refused('driver/host', str(params.get('error', params)))
    return state


def windows(snapshot):
    """Normalize the host's last two window slots, with unknown fields kept null."""
    if type(snapshot) is not dict:
        raise Refused('driver/windows', 'expected a rate-limit snapshot')
    result = []
    for slot in ('primary', 'secondary'):
        window = snapshot.get(slot)
        if window is None:
            continue
        if type(window) is not dict:
            raise Refused('driver/windows', 'expected a rate-limit window')
        row = {}
        for name, field in (('minutes', 'windowDurationMins'), ('used_percent', 'usedPercent'), ('resets_at', 'resetsAt')):
            value = window.get(field)
            if value is not None and (type(value) is not int or not 0 <= value <= SAFE):
                raise Refused('driver/windows', 'invalid ' + field)
            row[name] = value
        result.append(row)
    return sorted(result, key=lambda row: (row['minutes'] is None, row['minutes'] or 0))


def brake_seen(snapshot):
    """The account's windows as read, rate-limit data alone, for a refusal's line."""
    def number(value):
        return str(value) if type(value) in (int, float) else '?'
    parts = []
    for field in ('limitId', 'planType'):
        value = snapshot.get(field)
        if type(value) is str and value and len(value) <= 64 and all(c.isalnum() or c in '_.-' for c in value):
            parts.append(field + ' ' + value)
    for slot in ('primary', 'secondary'):
        window = snapshot.get(slot)
        if type(window) is not dict:
            parts.append(slot + ' none')
            continue
        parts.append(slot + ' ' + number(window.get('windowDurationMins')) + ' min '
                     + number(window.get('usedPercent')) + '% resets ' + number(window.get('resetsAt')))
    return ' (seen: ' + ', '.join(parts) + ')'


def brake_fold(response, five_hour, week):
    """Admit only a snapshot whose week, and five-hour window when reported, are below their shares."""
    if 'error' in response:
        error = response['error']
        return 1, 'refused: rateLimits: ' + str(error.get('message', error) if type(error) is dict else error)
    result = response.get('result')
    snapshot = result.get('rateLimits') if type(result) is dict else None
    if type(snapshot) is not dict:
        return 1, 'refused: rateLimits missing'
    seen = brake_seen(snapshot)
    if snapshot.get('rateLimitReachedType') is not None:
        return 1, 'refused: ' + str(snapshot['rateLimitReachedType']) + seen
    try:
        rows = windows(snapshot)
    except (Refused, AttributeError):
        return 1, 'refused: rateLimits malformed' + seen
    if any(r['minutes'] not in (300, 10080) or r['used_percent'] is None for r in rows):
        return 1, 'refused: window of unknown duration or use' + seen
    found = []
    for minutes, name, share in ((300, 'five-hour', five_hour), (10080, 'week', week)):
        row = next((r for r in rows if r['minutes'] == minutes), None)
        # The week is required; a plan that reports no five-hour window has no share to keep.
        if row is None and name == 'five-hour':
            found.append('five-hour none')
            continue
        if row is None:
            return 1, 'refused: ' + name + ' window missing' + seen
        text = name + ' ' + str(row['used_percent']) + '%/' + str(share) + '%'
        if row['used_percent'] > share:
            return 1, 'refused: ' + text + seen
        found.append(text)
    return 0, 'go: ' + ', '.join(found)
