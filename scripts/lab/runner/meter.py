"""Meter readings folded from supplied command results (lab.md, 6.5)."""
import re


def reading(stdout):
    """The final stdout line is a canonical nonnegative decimal, or unknown."""
    lines = stdout.split(b'\n')
    if lines and lines[-1] == b'':
        lines.pop()
    line = lines[-1] if lines else b''
    if not re.fullmatch(rb'0|[1-9][0-9]*', line):
        return None
    # Avoid the interpreter's decimal conversion limit without changing the grammar.
    value = 0
    for digit in line:
        value = value * 10 + digit - 48
    return value


def fold(state, record, limit, name):
    """Return the next spend/failure streak and the first terminal signature, if any."""
    state = dict(state)
    result = record['exit']
    signature = None
    if record.get('timed_out'):
        signature = 'bound/command_seconds/' + name
    elif result.get('signal') is not None:
        signature = 'meter/signal:' + result['signal']
    elif result.get('status') != 0:
        signature = 'meter/exit:' + str(result.get('status'))
    elif record.get('spend') is None:
        signature = 'meter/reading'
    if signature is None:
        state.update(spend=record['spend'], failures=0)
        if record['spend'] > limit:
            signature = 'bound/subject_spend'
    else:
        state['failures'] += 1
        if state['failures'] < 2:
            signature = None
    if state['halted']:
        signature = None
    elif signature is not None:
        state['halted'] = True
    return state, signature
