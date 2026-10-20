"""The null-driver life: supplied events in, ordered effects out (lab.md, 5)."""
from dataclasses import dataclass
import json
from . import canonical, draws, experiment
from .records import Refused


@dataclass
class State:
    data: dict


def initial(exp, seed, cost, known, texts):
    if exp['driver'] is not None:
        raise Refused('driver', 'nonnull drivers are unsupported by this runtime')
    canonical.u64(seed)
    return State(dict(exp=exp, seed=seed, cost=cost, known=known, texts=texts,
                      phase='validate', order=[], ready=[], closed=[], processes={}, commands=[],
                      current=None, index=0, deadline=None, stop_queue=[], cut=False,
                      interrupted=None, errors=[], bounds=[], schedule=dict(draws={}, start_order=[], faults=[])))


def _phase(d, phase, effects):
    d.update(phase=phase, index=0, current=None, deadline=None)
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
                files=[dict(path=f['path'], text=text(d['texts'][f['from']])) for f in item.get('files', [])])


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
    if phase in ('seed', 'collect', 'check'):
        group = dict(seed='actions', collect='collect', check='checks')[phase]
        items = exp[group]
        if d['index'] < len(items):
            item = items[d['index']]
            name = item['name']
            extra = {'collect.dir': d['known']['collected.' + name]} if phase == 'collect' else None
            try:
                rendered = _render(d, item, extra)
            except Refused as error:
                # Early exit can leave a capture unavailable. Retain this as unjudged.
                d['commands'].append(dict(group=group, name=name, skipped=str(error)))
                effects.append(dict(op='unavailable', group=group, name=name, reason=str(error)))
                d['index'] += 1
                _advance(d, effects, ms)
                return
            d['current'] = name
            effects.append(dict(op='spawn', group=group, name=name, **rendered))
            return
        following = dict(seed='drive', collect='check', check='stop')[phase]
        _phase(d, following, effects)
        if following == 'drive':
            _phase(d, 'collect', effects)
        if following == 'stop':
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
        _phase(d, 'judge', effects)
        effects.append(dict(op='judge'))
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
    if d['phase'] in ('start', 'seed', 'drive'):
        if d['phase'] == 'seed' and d['current'] is not None:
            effects.append(dict(op='cancel_command', name=d['current']))
            d['current'] = None
        _phase(d, 'collect', effects)
        _advance(d, effects, ms)
    elif d['phase'] in ('collect', 'check') and d['current'] is not None:
        effects.append(dict(op='cancel_command', name=d['current']))


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
    elif kind == 'spawned':
        name, group = event['name'], event['group']
        if group == 'processes':
            d['order'].append(name)
            d['schedule']['start_order'].append(name)
            d['processes'][name] = dict(started_ms=ms, ready_ms=None, seen=[], captures={}, stopping=False, exit=None)
            d['deadline'] = ms + d['cost']['ready_seconds'] * 1000
        else:
            d['deadline'] = ms + d['cost']['command_seconds'] * 1000
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
            if len(record['seen']) == len(item['ready']):
                record['ready_ms'] = ms
                for capture, value in record['captures'].items():
                    d['known'][f'proc.{name}.ready.{capture}'] = value
                d['ready'].append(name)
                effects.append(dict(op='ready', name=name, captures=record['captures']))
                d['deadline'] = None
                _advance(d, effects, ms)
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
        if name not in d['closed']:
            d['closed'].append(name)
        if d['phase'] == 'stop' and d['current'] == name:
            d['stop_queue'] = [n for n in d['stop_queue'] if n != name]
            _advance(d, effects, ms)
    elif kind == 'command_done':
        if (d['current'] == event['name'] and d['phase'] in ('seed', 'collect', 'check')
                and event['record']['group'] == dict(seed='actions', collect='collect', check='checks')[d['phase']]):
            d['commands'].append(event['record'])
            d['index'] += 1
            d['current'] = None
            d['deadline'] = None
            _advance(d, effects, ms)
    elif kind == 'tick' and d['deadline'] is not None and ms >= d['deadline']:
        name = d['current']
        bound = dict(start='ready_seconds', seed='command_seconds', collect='command_seconds',
                     check='command_seconds', stop='stop_seconds')[d['phase']]
        d['bounds'].append(dict(name=bound, who=name, ms=ms))
        effects.append(dict(op='bound', name=bound, who=name))
        d['deadline'] = None
        if d['phase'] == 'start':
            _cut(d, effects, ms)
        else:
            effects.append(dict(op='kill_tree' if d['phase'] == 'stop' else 'cancel_command', name=name))
    elif kind in ('interrupt', 'error'):
        if kind == 'interrupt':
            d['interrupted'] = event['signal']
        d['errors'].append(dict(what='interrupted' if kind == 'interrupt' else event['what'], detail=event.get('detail', kind)))
        if d['phase'] in ('seed', 'collect', 'check') and d['current'] is not None:
            effects.append(dict(op='cancel_command', name=d['current']))
        if d['phase'] != 'stop':
            _phase(d, 'stop', effects)
            _stop(d, effects, ms)
    return State(d), effects
