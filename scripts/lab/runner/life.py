"""The run's life: supplied events in, ordered effects out (lab.md, 5)."""
from dataclasses import dataclass
import json
from . import canonical, codex, draws, experiment, meter
from .records import Refused


@dataclass
class State:
    data: dict


COMMAND_GROUP = {'seed': 'actions', 'collect': 'collect', 'check': 'checks',
                 'release': 'release',
                 'stopped-collect': 'collect', 'stopped-check': 'checks'}


def _items(d, phase):
    items = d['exp'].get(COMMAND_GROUP[phase], [])
    if phase in ('seed', 'release'):
        return items
    cluster = 'stopped' if phase.startswith('stopped-') else 'up'
    return [item for item in items if item.get('cluster', 'up') == cluster]


def _unavailable(d, effects, group, item, reason):
    extra = {'cluster': 'stopped'} if item.get('cluster') == 'stopped' else {}
    record = dict(group=group, name=item['name'], skipped=reason, **extra)
    if group == 'actions' and 'await' in item:
        record['awaited'] = None
    d['commands'].append(record)
    effects.append(dict(op='unavailable', group=group, name=item['name'], reason=reason, **extra))


def _skip_stopped(d, effects):
    for group in ('collect', 'checks'):
        for item in d['exp'][group]:
            if (item.get('cluster') == 'stopped'
                    and not any(c['group'] == group and c['name'] == item['name']
                                for c in d['commands'] + d['started_commands'])):
                _unavailable(d, effects, group, item,
                             'stopped command unavailable after interruption or machinery error')


def _collection_inputs(item):
    texts = item['argv'] + list(item['env'].values()) + ([item['stdin']] if 'stdin' in item else [])
    return {name[10:] for text in texts for placeholder, name in experiment._parts(text)
            if placeholder and name.startswith('collected.')}


def initial(exp, seed, cost, known, texts):
    if exp['faults']:
        raise Refused('faults', 'unsupported by this runtime')
    canonical.u64(seed)
    return State(dict(exp=exp, seed=seed, cost=cost, known=known, texts=texts,
                      phase='validate', order=[], ready=[], closed=[], processes={}, commands=[],
                      current=None, index=0, deadline=None, awaiting=None, stop_queue=[], cut=False,
                      started_commands=[], collections={}, driver=None, tool_provision=None, meter=None,
                      interrupted=None, machinery=False, errors=[], bounds=[], schedule=dict(draws={}, start_order=[], faults=[])))


def _phase(d, phase, effects):
    d.update(phase=phase, index=0, current=None, deadline=None, awaiting=None)
    effects.append(dict(op='phase', phase=phase))


def _render(d, item, extra=None):
    known = {**d['known'], **(extra or {})}
    def text(value):
        for placeholder, name in experiment._parts(value):
            if placeholder and name.startswith('seed.') and name not in known:
                value_draw = draws.draw(d['seed'], name[5:])
                d['schedule']['draws'][name[5:]] = value_draw
                known[name] = value_draw
        return experiment.render(value, known)
    return dict(argv=[text(v) for v in item['argv']], env={k: text(v) for k, v in item['env'].items()},
                stdin=text(item['stdin']) if 'stdin' in item else None,
                files=[dict(path=f['path'], text=text(d['texts'][f['from']])) if 'from' in f else dict(f)
                       for f in item.get('files', [])])


def _ready(d, name, effects, ms):
    if d['phase'] != 'start' or d['current'] != name or name in d['ready']:
        return
    record = d['processes'][name]
    item = next(p for p in d['exp']['processes'] if p['name'] == name)
    if 'bootstrap' in record and d['deadline'] is not None and ms >= d['deadline']:
        d['bounds'].append(dict(name='ready_seconds', who=name, ms=ms))
        effects.append(dict(op='bound', name='ready_seconds', who=name))
        d['deadline'] = None
        _cut(d, effects, ms)
        return
    if (len(record['seen']) != len(item['ready'])
            or 'bootstrap' in record and record['bootstrap']['received_ms'] is None):
        return
    record['ready_ms'] = ms
    for capture, value in record['captures'].items():
        d['known'][f'proc.{name}.ready.{capture}'] = value
    d['ready'].append(name)
    effects.append(dict(op='ready', name=name, captures=record['captures']))
    d['deadline'] = None
    _advance(d, effects, ms)


def _command_advance(d, effects, ms):
    d['index'] += 1
    d.update(current=None, deadline=None, awaiting=None)
    _advance(d, effects, ms)


def _release_unavailable(d):
    if d['machinery']:
        return 'release unavailable after machinery error'
    if not any(p['exit'] is None for p in d['processes'].values()):
        return 'release unavailable: no started process is alive'
    return None


def _release(d, effects, ms):
    _phase(d, 'release', effects)
    _advance(d, effects, ms)


def _released(d, effects, ms):
    if d['exp'].get('held', []):
        effects.append(dict(op='give_held'))
    _phase(d, 'stop', effects)
    _stop(d, effects, ms)


def _advance(d, effects, ms):
    phase = d['phase']
    exp = d['exp']
    if phase == 'start':
        remaining = [p for p in exp['processes'] if p['name'] not in d['order']]
        if remaining and not d['cut']:
            candidates = sorted((p for p in remaining if all(n in d['ready'] for n in p.get('after', []))), key=lambda p: p['name'])
            label = f"start-order.{len(d['order'])}"
            value = draws.draw(d['seed'], label)
            d['schedule']['draws'][label] = value
            item = candidates[canonical.u64(value) % len(candidates)]
            name = item['name']
            rendered = _render(d, item)
            d['current'] = name
            effects.append(dict(op='spawn', group='processes', name=name, **rendered))
            return
        _phase(d, 'seed', effects)
        phase = 'seed'
    if phase == 'seed' and d['cut']:
        _phase(d, 'collect', effects)
        phase = 'collect'
    if phase in COMMAND_GROUP:
        group = COMMAND_GROUP[phase]
        items = _items(d, phase)
        if d['index'] < len(items):
            item = items[d['index']]
            name = item['name']
            reason = _release_unavailable(d) if phase == 'release' else None
            if reason is not None:
                _unavailable(d, effects, group, item, reason)
                d['index'] += 1
                _advance(d, effects, ms)
                return
            extra = {'collect.dir': d['known']['collected.' + name]} if group == 'collect' else None
            if phase == 'stopped-check' and any(not d['collections'].get(n, False) for n in _collection_inputs(item)):
                _unavailable(d, effects, group, item, 'collection input did not complete successfully')
                d['index'] += 1
                _advance(d, effects, ms)
                return
            try:
                rendered = _render(d, item, extra)
            except Refused as error:
                # Early exit can leave a capture unavailable. Retain this as unjudged.
                _unavailable(d, effects, group, item, str(error))
                d['index'] += 1
                _advance(d, effects, ms)
                return
            d['current'] = name
            extra = {'cluster': 'stopped'} if phase.startswith('stopped-') else {}
            if group == 'actions':
                source = next((p['name'] for p in exp['processes'] if p.get('bootstrap', {}).get('action') == name), None)
                if source is not None:
                    extra['bootstrap'] = dict(process=source)
            effects.append(dict(op='spawn', group=group, name=name, **extra, **rendered))
            return
        if phase == 'release':
            _released(d, effects, ms)
            return
        if phase == 'check':
            _release(d, effects, ms)
            return
        following = {'seed': 'drive', 'collect': 'check',
                     'stopped-collect': 'stopped-check', 'stopped-check': 'judge'}[phase]
        _phase(d, following, effects)
        if following == 'drive':
            if exp['driver'] is not None and not d['cut']:
                d['driver'] = dict(observed=codex.initial(), inventory=None, started_ms=None,
                                   ended_ms=None, ending=False)
                d['tool_provision'] = dict(observed=codex.initial(), inventory=None, started_ms=ms,
                                           ended_ms=None, ending=False)
                d['deadline'] = ms + d['cost']['command_seconds'] * 1000
                effects.append(dict(op='provision_tools'))
                return
            _meter_begin(d, ms)
            _drive_ended(d, effects, ms)
            return
        if following == 'judge':
            effects.append(dict(op='judge'))
        elif following == 'stop':
            _stop(d, effects, ms)
        else:
            _advance(d, effects, ms)
    elif phase == 'stop':
        _stop(d, effects, ms)


def _stop(d, effects, ms):
    d['stop_queue'] = [n for n in d['stop_queue'] if n not in d['closed']]
    if not d['stop_queue']:
        d['stop_queue'] = [n for n in reversed(d['order']) if n not in d['closed']]
    if not d['stop_queue']:
        d.update(current=None, deadline=None)
        if d['errors'] or d['interrupted'] is not None:
            _skip_stopped(d, effects)
        if d['errors'] or d['interrupted'] is not None or not any(
                item.get('cluster') == 'stopped' for group in ('collect', 'checks') for item in d['exp'][group]):
            _phase(d, 'judge', effects)
            effects.append(dict(op='judge'))
        # The host supplies closed only after all command trees and streams drain too.
        return
    name = d['stop_queue'][0]
    d['current'] = name
    record = d['processes'][name]
    if record.get('exit') is None and not record.get('stop_requested'):
        record['stop_requested'] = True
        effects.append(dict(op='signal_first', name=name, signal='SIGTERM'))
    elif record.get('exit') is not None:
        d['deadline'] = None
        effects.append(dict(op='reap_tree', name=name))


def _cut(d, effects, ms):
    d['cut'] = True
    effects.append(dict(op='cut', phase=d['phase']))
    if d['phase'] == 'drive' and d['driver'] is not None:
        _end_driver(d, effects)
        return
    if d['phase'] in ('start', 'seed', 'drive'):
        if d['phase'] == 'seed' and d['current'] is not None:
            effects.append(dict(op='cancel_command', name=d['current']))
            d['current'] = None
        if d['phase'] == 'drive':
            _drive_ended(d, effects, ms)
        else:
            _phase(d, 'collect', effects)
            _advance(d, effects, ms)
    elif d['phase'] in ('collect', 'check') and d['current'] is not None:
        effects.append(dict(op='cancel_command', name=d['current']))


def _meter_begin(d, ms):
    if d['exp'].get('meter') is not None:
        d['meter'] = dict(n=0, active=False, due=ms + d['cost']['meter_seconds'] * 1000,
                          deadline=None, drive_done=False, final=False, timed_out=False,
                          spend=None, failures=0, halted=False)


def _meter_spawn(d, effects, ms, *, final=False):
    state = d['meter']
    state.update(n=state['n'] + 1, active=True,
                 due=None if final else ms + d['cost']['meter_seconds'] * 1000, deadline=None,
                 final=final, timed_out=False)
    item = d['exp']['meter']
    effects.append(dict(op='spawn', group='meter', name=item['name'], n=state['n'], **_render(d, item)))


def _drive_ended(d, effects, ms):
    state = d['meter']
    d['deadline'] = None
    if state is not None:
        state.update(drive_done=True, due=None)
        if not state['active']:
            _meter_spawn(d, effects, ms, final=True)
        return
    _phase(d, 'collect', effects)
    _advance(d, effects, ms)


def _meter_due(d, effects, ms):
    state = d['meter']
    if state is None or d['phase'] != 'drive':
        return
    if state['active']:
        if state['deadline'] is not None and ms >= state['deadline']:
            state.update(deadline=None, timed_out=True)
            effects.append(dict(op='cancel_command', group='meter', name=d['exp']['meter']['name'], timed_out=True))
    elif not state['halted'] and not state['drive_done'] and state['due'] is not None and ms >= state['due']:
        _meter_spawn(d, effects, ms)


def _meter_done(d, event, effects, ms):
    state = d['meter']
    if state is None or not state['active'] or event['n'] != state['n']:
        return
    record = dict(event['record'])
    record['timed_out'] = (record.get('timed_out', False) or state['timed_out']
                           or state['deadline'] is not None and ms >= state['deadline'])
    state, signature = meter.fold(state, record, d['cost']['subject_spend'], d['exp']['meter']['name'])
    d['meter'] = state
    state.update(active=False, deadline=None)
    if signature is not None:
        state['due'] = None
        if signature == 'bound/subject_spend':
            d['bounds'].append(dict(name='subject_spend', ms=ms))
            effects.append(dict(op='bound', name='subject_spend'))
        else:
            effects.append(dict(op='meter_failure', signature=signature))
        _end_driver(d, effects)
    if d['phase'] != 'drive':
        return
    if state['final']:
        _phase(d, 'collect', effects)
        _advance(d, effects, ms)
    elif state['drive_done']:
        _meter_spawn(d, effects, ms, final=True)
    elif not state['halted'] and state['due'] is not None and ms >= state['due']:
        _meter_spawn(d, effects, ms)


def _end_driver(d, effects):
    provision = d['tool_provision']
    if provision is not None and provision['ended_ms'] is None:
        if not provision['ending']:
            provision['ending'] = True
            d['deadline'] = None
            effects.append(dict(op='stop_driver'))
        return
    if d['driver'] is not None and not d['driver']['ending']:
        d['driver']['ending'] = True
        d['deadline'] = None
        effects.append(dict(op='stop_driver'))


def _driver_limits(d, effects, ms):
    driver = d['driver']
    if driver is None:
        return
    observed = driver['observed']
    bound = None
    if not driver['ending'] and driver['started_ms'] is not None and ms - driver['started_ms'] >= d['cost']['driver_seconds'] * 1000:
        bound = 'driver_seconds'
    elif not driver['ending'] and len(observed['actions']) > d['cost']['driver_actions']:
        bound = 'driver_actions'
    if observed['tokens']['input'] is not None and observed['tokens']['input'] > d['cost']['driver_input_tokens']:
        observed['overshoot'] = observed['tokens']['input'] - d['cost']['driver_input_tokens']
        if not any(b['name'] == 'driver_input_tokens' for b in d['bounds']):
            # Late usage is retained even after another bound has stopped the host.
            if bound is not None:
                d['bounds'].append(dict(name=bound, ms=ms))
                effects.append(dict(op='bound', name=bound))
            bound = 'driver_input_tokens'
    if bound is not None:
        d['bounds'].append(dict(name=bound, ms=ms))
        effects.append(dict(op='bound', name=bound))
        _end_driver(d, effects)


def _driver_event(d, event, effects, ms):
    driver = d['driver']
    if driver is None:
        return
    try:
        _driver_limits(d, effects, ms)
        if event['kind'] == 'driver_inventory':
            driver['inventory'] = event['inventory']
            if any(i['state'] == 'mismatch' for i in driver['inventory'].values()):
                raise Refused('driver/inventory', 'incomplete or mismatched inventory')
            if not driver['ending']:
                effects.append(dict(op='driver_turn'))
        elif event['kind'] == 'driver_call':
            action = codex.action_start(driver['observed'], event['item'], ms, 'broker')
            driver['call_number'] = action['n']
        else:
            driver['observed'] = codex.fold(driver['observed'], event['message'], ms)
            if driver['observed']['unhandled'] is not None:
                raise Refused('driver/audit', driver['observed']['unhandled'])
        _driver_limits(d, effects, ms)
        report = driver['observed']['report']
        if report is not None and not driver['ending']:
            if not report['done'] or report['unseen']:
                effects.append(dict(op='driver_use', report=report))
            _end_driver(d, effects)
        elif any(t['completed_ms'] is not None for t in driver['observed']['turns']):
            _end_driver(d, effects)
    except (Refused, KeyError, TypeError, ValueError) as error:
        d['errors'].append(dict(what='driver', detail=str(error)))
        _end_driver(d, effects)


def step(state, event):
    # Value copy keeps scripted worlds' previous snapshots usable.
    d = json.loads(canonical.dumps(state.data))
    effects = []
    kind, ms = event['kind'], event.get('ms', 0)
    if kind == 'begin':
        effects.append(dict(op='validate'))
    elif kind == 'validated':
        _phase(d, 'provision', effects)
        effects.append(dict(op='provision'))
    elif kind == 'provisioned':
        _phase(d, 'start', effects)
        _advance(d, effects, ms)
    elif kind == 'meter_due':
        _meter_due(d, effects, ms)
    elif kind == 'meter_done':
        _meter_done(d, event, effects, ms)
    elif kind == 'tools_provisioned':
        d['tool_provision']['ended_ms'] = ms
        if d['phase'] == 'drive':
            d['deadline'] = None
        d['errors'].extend(event['errors'])
        if (event['errors'] or any(e['what'].startswith('provision/') for e in d['errors'])
                or d['cut'] or d['machinery'] or d['interrupted'] is not None):
            d['driver']['ending'] = True
            if d['phase'] == 'drive':
                if event['errors'] or any(e['what'].startswith('provision/') for e in d['errors']):
                    d['machinery'] = True
                    _skip_stopped(d, effects)
                    _release(d, effects, ms)
                else:
                    _drive_ended(d, effects, ms)
        elif d['phase'] == 'drive':
            _meter_begin(d, ms)
            effects.append(dict(op='start_driver'))
    elif kind in ('provision_inventory', 'provision_event', 'provision_call', 'provision_error'):
        provision = d['tool_provision']
        try:
            if kind == 'provision_inventory':
                provision['inventory'] = event['inventory']
                if any(v['state'] == 'mismatch' for v in event['inventory'].values()):
                    raise Refused('provision/inventory', 'incomplete or mismatched inventory')
                if not provision['ending']:
                    effects.append(dict(op='driver_turn'))
            elif kind == 'provision_call':
                action = codex.action_start(provision['observed'], event['item'], ms, 'broker')
                provision['call_number'] = action['n']
            elif kind == 'provision_event':
                provision['observed'] = codex.fold(provision['observed'], event['message'], ms)
                if provision['observed']['unhandled'] is not None:
                    raise Refused('provision/host', provision['observed']['unhandled'])
                if any(t['completed_ms'] is not None for t in provision['observed']['turns']):
                    _end_driver(d, effects)
            else:
                raise Refused(event['what'], event['detail'])
        except Refused as error:
            d['errors'].append(dict(what=error.refusal.member, detail=error.refusal.why))
            _end_driver(d, effects)
    elif kind == 'driver_spawned':
        d['driver']['started_ms'] = ms
        d['deadline'] = ms + d['cost']['driver_seconds'] * 1000
    elif kind in ('driver_inventory', 'driver_event', 'driver_call'):
        _driver_event(d, event, effects, ms)
    elif kind == 'driver_command_bound':
        if not d['driver']['ending']:
            d['bounds'].append(dict(name='command_seconds', ms=ms))
            effects.append(dict(op='bound', name='command_seconds'))
            _end_driver(d, effects)
    elif kind == 'driver_closed':
        driver = d['driver']
        _driver_limits(d, effects, ms)
        driver['ended_ms'] = ms
        if (driver['inventory'] is None
                or event['result'].get('status') != 0 and not event.get('stopped')):
            d['errors'].append(dict(what='driver/host', detail='host ended without completing its drive'))
        if d['phase'] == 'drive':
            _drive_ended(d, effects, ms)
    elif kind == 'spawned':
        name, group = event['name'], event['group']
        if group == 'meter':
            d['meter']['deadline'] = ms + d['cost']['command_seconds'] * 1000
        elif group == 'processes':
            d['order'].append(name)
            d['schedule']['start_order'].append(name)
            d['processes'][name] = dict(started_ms=ms, ready_ms=None, seen=[], captures={}, stopping=False, exit=None)
            item = next(p for p in d['exp']['processes'] if p['name'] == name)
            if 'bootstrap' in item:
                d['processes'][name]['bootstrap'] = dict(**item['bootstrap'], received_ms=None, delivered_ms=None)
            d['deadline'] = ms + d['cost']['ready_seconds'] * 1000
        else:
            d['started_commands'].append(dict(group=group, name=name))
            d['deadline'] = ms + d['cost']['command_seconds'] * 1000
            if group == 'actions':
                item = d['exp']['actions'][d['index']]
                if 'await' in item:
                    d['awaiting'] = dict(rule=item['await'], started_ms=ms, seen=None, done=False)
    elif kind == 'line' and event['name'] in d['processes']:
        name = event['name']
        record = d['processes'][name]
        item = next(p for p in d['exp']['processes'] if p['name'] == name)
        if d['phase'] == 'start' and d['current'] == name and name not in d['ready']:
            for i, rule in enumerate(item['ready']):
                if i not in record['seen'] and event['stream'] == rule['stream'] and event['text'].startswith(rule['prefix']):
                    record['seen'].append(i)
                    if rule['capture'] is not None:
                        capture = event['text'][len(rule['prefix']):]
                        record['captures'][rule['capture']] = capture
            _ready(d, name, effects, ms)
        elif d['phase'] == 'seed' and d['awaiting'] is not None:
            pending = d['awaiting']
            rule = pending['rule']
            if (pending['seen'] is None and ms >= pending['started_ms']
                    and d['deadline'] is not None and ms < d['deadline']
                    and name == rule['process'] and event['stream'] == rule['stream']
                    and event['text'].startswith(rule['prefix'])):
                captures = {rule['capture']: event['text'][len(rule['prefix']):]}
                pending['seen'] = dict(ms=ms, captures=captures)
                action = d['current']
                d['known'][f"action.{action}.{rule['capture']}"] = captures[rule['capture']]
                effects.append(dict(op='awaited', name=action, ms=ms, captures=captures))
                if pending['done']:
                    d['commands'][-1]['awaited'] = pending['seen']
                    _command_advance(d, effects, ms)
    elif kind in ('bootstrap_received', 'bootstrap_delivered'):
        name = event['name']
        record = d['processes'][name]
        channel = record.get('bootstrap')
        field = 'received_ms' if kind == 'bootstrap_received' else 'delivered_ms'
        if channel is not None and channel[field] is None:
            channel[field] = ms
            effects.append(dict(op=kind, process=name, action=channel['action'], ms=ms))
            if kind == 'bootstrap_received':
                _ready(d, name, effects, ms)
    elif kind == 'bootstrap_failed':
        _cut(d, effects, ms)
    elif kind == 'signalled':
        d['processes'][event['name']]['stopping'] = True
        d['deadline'] = ms + d['cost']['stop_seconds'] * 1000
    elif kind == 'exit':
        name = event['name']
        record = d['processes'][name]
        record['exit'] = dict(ms=ms, phase=d['phase'], stopped=record['stopping'],
                              killed=event.get('killed', False), **event['result'])
        effects.append(dict(op='reap_tree', name=name))
        if not record['stopping'] and d['phase'] not in ('stop', 'judge'):
            _cut(d, effects, ms)
    elif kind == 'tree_empty':
        name = event['name']
        if d['processes'][name]['exit'] is None:
            return State(d), effects
        if name not in d['closed']:
            d['closed'].append(name)
        if d['phase'] == 'stop' and d['current'] == name:
            d['stop_queue'] = [n for n in d['stop_queue'] if n != name]
            _advance(d, effects, ms)
    elif kind == 'closed' and d['phase'] == 'stop':
        if (all(n in d['closed'] and d['processes'][n]['exit'] is not None for n in d['order'])
                and not d['errors'] and d['interrupted'] is None):
            _phase(d, 'stopped-collect', effects)
            _advance(d, effects, ms)
        else:
            _stop(d, effects, ms)
    elif kind == 'command_done':
        if (d['current'] == event['name'] and d['phase'] in COMMAND_GROUP
                and event['record']['group'] == COMMAND_GROUP[d['phase']]):
            record = event['record']
            if (d['phase'].startswith('stopped-') or d['phase'] == 'release') and d['deadline'] is not None and ms >= d['deadline']:
                d['bounds'].append(dict(name='command_seconds', who=d['current'], ms=ms))
                effects.append(dict(op='bound', name='command_seconds', who=d['current']))
                d['deadline'] = None
            if record['group'] == 'collect':
                end = record.get('exit') or {}
                d['collections'][event['name']] = (end.get('status') == 0 and not end.get('stopped')
                    and not end.get('killed') and d['deadline'] is not None and ms < d['deadline']
                    and not record.get('timed_out') and not record.get('cancelled'))
            pending = d['awaiting']
            if pending is not None:
                if pending['done']:
                    return State(d), effects
                if d['deadline'] is not None and ms >= d['deadline']:
                    d['bounds'].append(dict(name='command_seconds', who=d['current'], ms=ms))
                    effects.append(dict(op='bound', name='command_seconds', who=d['current']))
                    d['deadline'] = None
                success = (record.get('exit', {}).get('status') == 0
                           and not record.get('exit', {}).get('stopped')
                           and not record.get('exit', {}).get('killed')
                           and d['deadline'] is not None)
                record = {**record, 'awaited': pending['seen'] if success else None}
                if not success:
                    d['known'].pop(f"action.{d['current']}.{pending['rule']['capture']}", None)
                d['commands'].append(record)
                if success and pending['seen'] is None and d['deadline'] is not None:
                    pending['done'] = True
                    return State(d), effects
            else:
                d['commands'].append(record)
            _command_advance(d, effects, ms)
    elif kind == 'tick' and d['phase'] == 'drive':
        provision = d['tool_provision']
        if provision is not None and provision['ended_ms'] is None:
            if not provision['ending'] and d['deadline'] is not None and ms >= d['deadline']:
                d['errors'].append(dict(what='provision/timeout', detail='provisioning exceeded command_seconds'))
                _end_driver(d, effects)
        else:
            _driver_limits(d, effects, ms)
    elif kind == 'tick' and d['deadline'] is not None and ms >= d['deadline']:
        name = d['current']
        bound = dict(start='ready_seconds', seed='command_seconds', collect='command_seconds',
                     check='command_seconds', release='command_seconds', **{'stopped-collect': 'command_seconds',
                                               'stopped-check': 'command_seconds'}, stop='stop_seconds')[d['phase']]
        d['bounds'].append(dict(name=bound, who=name, ms=ms))
        effects.append(dict(op='bound', name=bound, who=name))
        d['deadline'] = None
        if d['phase'] == 'start':
            _cut(d, effects, ms)
        elif d['phase'] == 'seed' and d['awaiting'] is not None and d['awaiting']['done']:
            _command_advance(d, effects, ms)
        else:
            effects.append(dict(op='kill_tree' if d['phase'] == 'stop' else 'cancel_command', name=name))
    elif kind in ('interrupt', 'error'):
        if d['meter'] is not None:
            d['meter']['due'] = None
            if d['meter']['active']:
                effects.append(dict(op='cancel_command', group='meter', name=d['exp']['meter']['name']))
        _end_driver(d, effects)
        if kind == 'interrupt':
            d['interrupted'] = event['signal']
        else:
            d['machinery'] = True
        d['errors'].append(dict(what='interrupted' if kind == 'interrupt' else event['what'], detail=event.get('detail', kind)))
        if kind == 'interrupt' and d['phase'] == 'release':
            return State(d), effects
        if d['phase'] in COMMAND_GROUP and d['current'] is not None:
            effects.append(dict(op='cancel_command', name=d['current']))
        _skip_stopped(d, effects)
        if d['phase'] not in ('stop', 'stopped-collect', 'stopped-check', 'judge', 'closed'):
            if d['phase'] == 'release':
                # A spawn or runner failure leaves this command unavailable too.
                d.update(current=None, deadline=None, awaiting=None)
                _advance(d, effects, ms)
            else:
                _release(d, effects, ms)
        elif d['phase'] != 'stop':
            _phase(d, 'stop', effects)
            _stop(d, effects, ms)
    return State(d), effects
