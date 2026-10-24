"""Pure recovery of interrupted records from durable context and surviving events."""
import json
from . import canonical, codex, draws
from .records import Refused


def events(data):
    """A cut may leave a final fragment; complete malformed lines still refuse."""
    result = []
    lines = data.splitlines(keepends=True)
    for n, line in enumerate(lines):
        if not line.endswith(b'\n'):
            break
        try:
            value = json.loads(line)
        except (ValueError, UnicodeError) as error:
            if n + 1 < len(lines) and b'"event":"interrupted"' in lines[n + 1]:
                continue
            raise Refused('recovery.events', 'malformed complete event') from error
        if type(value) is not dict:
            raise Refused('recovery.events', 'expected event object')
        result.append(value)
    return result


def interrupted(context, data, ended, taken=(), driver_data=None):
    """Retain observed starts/ends and held handoffs; infer no successful check."""
    value = json.loads(canonical.dumps(context))
    observed = events(data)
    ms = max((e.get('ms', e.get('t_ms', 0)) for e in observed), default=0)
    event = dict(format='lab-event/1', event='interrupted', ms=ms, detail='unit ended before record publication')
    value.update(ended=ended, outcome='error')
    value['errors'] = [dict(what='interrupted', detail=event['detail'])]
    if value['format'] == 'lab-run/1':
        processes, commands = {}, []
        held = {}
        for e in observed:
            kind, name = e.get('event'), e.get('name')
            if kind == 'start':
                record = dict(name=name, group=e['group'], argv=e['argv'], env=e['env'], started_ms=e['ms'])
                if e['group'] == 'processes':
                    processes[name] = record
                else:
                    commands.append(record)
            elif kind == 'exit':
                record = processes.get(name) if e['group'] == 'processes' else next(
                    (r for r in reversed(commands) if r['name'] == name and r['group'] == e['group']), None)
                if record is not None:
                    record['exit'] = {k: v for k, v in e.items() if k not in ('format', 'event', 'name', 'group')}
            elif kind == 'ready' and name in processes:
                processes[name].update(ready_ms=e['ms'], captures=e['captures'])
            elif kind == 'held_taken':
                held[name] = dict(name=name, taken_ms=e['ms'], given_ms=None)
            elif kind == 'held_given' and name in held:
                held[name]['given_ms'] = e['ms']
            elif kind == 'fault':
                fault = next((f for f in value['schedule'].get('faults', []) if f['index'] == e['index']), None)
                if fault is None:
                    fault = {k: e[k] for k in ('index', 'kill', 'signal', 'n')}
                    value['schedule'].setdefault('faults', []).append(fault)
                fault.update(fired=True, ms=e['ms'])
                label = 'fault.' + str(e['index'])
                value['schedule'].setdefault('draws', {})[label] = draws.draw(value['seed'], label)
        for name in taken:
            held.setdefault(name, dict(name=name, taken_ms=None, given_ms=None))['given_ms'] = None
        value.update(processes=processes, commands=commands, held=list(held.values()))
        value['schedule']['start_order'] = list(processes)
        for n, _name in enumerate(processes):
            label = 'start-order.' + str(n)
            value['schedule'].setdefault('draws', {})[label] = draws.draw(value['seed'], label)
        value['usage']['elapsed_ms'] = ms
        if driver_data is not None:
            state = host_events(driver_data)
            value['driver'] = dict(invocation=None, expected_inventory=None, inventory=None,
                actions=state['actions'], turns=state['turns'], end=None, files={})
            value['usage']['driver'] = dict(elapsed_ms=None, seconds=None, actions=len(state['actions']),
                turns=len(state['turns']), tokens=state['tokens'], input_tokens_overshoot=state['overshoot'])
            value['usage']['windows'] = state['windows']
    else:
        state = host_events(data, triage=True)
        value['usage'].update(turns=len(state['turns']), tokens=state['tokens'])
    return value, event


def host_events(data, *, triage=False):
    state = codex.initial()
    for message in events(data):
        if 'method' in message and 'id' not in message:
            try:
                state = codex.fold(state, message, message.get('t_ms', 0), triage=triage)
            except Refused:
                break
    return state


def unit_ended(answer):
    fields = dict(line.split('=', 1) for line in answer.splitlines() if '=' in line)
    active, loaded = fields.get('ActiveState'), fields.get('LoadState')
    if loaded not in ('loaded', 'not-found', 'error', 'masked', 'bad-setting') or active not in (
            'active', 'reloading', 'inactive', 'failed', 'activating', 'deactivating', 'refreshing', 'maintenance'):
        raise Refused('recovery', 'manager did not report ActiveState and LoadState')
    return active in ('inactive', 'failed')
