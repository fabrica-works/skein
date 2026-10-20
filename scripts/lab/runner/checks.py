"""Checks over supplied bytes and process records (lab.md, 7)."""
import json
import re
from . import canonical
from .records import Refused, signature_value

PANIC = re.compile(r"thread '[^\r\n]*' panicked at (.+):([0-9]+):([0-9]+):(?:\r?$)", re.MULTILINE)


def failure(signature, kind, summary, evidence=()):
    return dict(signature=signature, kind=kind, summary=summary, evidence=list(evidence))


def signature(text):
    return signature_value(text)


def panics(who, text, evidence):
    result = []
    for match in PANIC.finditer(text):
        place = ':'.join(match.groups())
        message = text[match.end():].lstrip('\r\n').split('\n', 1)[0]
        result.append(failure(f'panic/{who}/{place}', 'panic', message, evidence))
    return result


def command(name, status, stdout, *, timed_out=False):
    """Return failures, undecided and errors. Check output need not be canonical."""
    failures, undecided, errors = [], [], []
    if timed_out:
        return [failure(f'bound/command_seconds/{name}', 'bound', 'command exceeded its cost class')], [], []
    lines = []
    try:
        text = stdout.decode('utf-8', 'strict')
        for line in text.splitlines():
            value = json.loads(line, object_pairs_hook=canonical._pairs,
                               parse_constant=canonical._no_number, parse_float=canonical._no_number)
            if (type(value) is dict and set(value) == {'fail', 'summary'}
                    and signature(value['fail']) and type(value['summary']) is str):
                canonical.dumps(value)
                lines.append(value)
            elif type(value) is dict and set(value) == {'undecided'} and type(value['undecided']) is str and value['undecided']:
                canonical.dumps(value)
                lines.append(value)
            else:
                raise ValueError('invalid signature line')
        failed = [v for v in lines if 'fail' in v]
        unknown = [v for v in lines if 'undecided' in v]
        valid = type(status) is int and ((status == 0 and not lines) or (status == 1 and bool(failed))
                 or (status == 2 and bool(unknown) and not failed))
        if not valid:
            raise ValueError('exit status contradicts check lines')
        failures = [failure(f"check/{name}/{v['fail']}", 'check', v['summary']) for v in failed]
        undecided = [dict(check=name, reason=v['undecided']) for v in unknown]
    except (UnicodeError, ValueError, RecursionError, Refused) as error:
        errors.append(dict(what='check/' + name, detail=str(error)))
    return failures, undecided, errors


def process(name, record, panic_failures):
    failures, undecided = list(panic_failures), []
    if 'spawn_error' in record:
        return failures, undecided
    end = record.get('exit')
    if end is not None and not end['stopped'] and not panic_failures:
        reason = 'exit:' + str(end['status']) if 'status' in end else 'signal:' + end['signal']
        failures.append(failure(f'stop/{name}/{reason}', 'stop', 'process ended before stop', [f'proc/{name}/stderr']))
    memory = record['memory']
    if memory['bound_bytes'] is not None:
        if memory['peak_bytes'] is None:
            undecided.append(dict(check='memory/' + name, reason='own process peak unavailable'))
        elif memory['peak_bytes'] > memory['bound_bytes']:
            failures.append(failure('memory/' + name, 'memory', 'own process peak exceeded declared bound'))
    return failures, undecided


def survivors(who, counts):
    return ([failure('survivor/' + who, 'survivor', 'tree outlived its first process')]
            if counts['populated'] else [])


def action(name, record):
    end = record['exit']
    if end['stopped'] or 'spawn_error' in record or end.get('status') == 0:
        return []
    suffix = 'exit:' + str(end['status']) if 'status' in end else 'signal:' + end['signal']
    return [failure(f'action/{name}/{suffix}', 'action', 'seed action did not exit zero',
                    [record['output']['stderr']['path']])]


def planted(required, failures, outcome):
    if outcome not in ('pass', 'fail'):
        return []
    observed = {f['signature'] for f in failures}
    return [failure('planted-missing/' + s, 'planted', 'planted signature not observed')
            for s in required if s not in observed]
