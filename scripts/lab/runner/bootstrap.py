"""Private bootstrap framing; only the host owns this byte state (lab.md, 4.3)."""
from dataclasses import dataclass, field


@dataclass(frozen=True)
class State:
    status: str = 'waiting'
    line: bytes = field(default=b'', repr=False)


def feed(state, data):
    """Consume a chunk or EOF, emitting only receipt or a fixed failure reason."""
    if state.status == 'invalid':
        return state, None
    if state.status in ('received', 'taken'):
        return (State('invalid'), 'extra') if data else (state, None)
    if state.status == 'draining':
        end = data.find(b'\n')
        if end < 0:
            return state, None
        if end + 1 != len(data):
            return State('invalid'), 'extra'
        return State('taken'), None
    if not data:
        return (State('invalid'), 'partial') if state.line else (state, None)
    end = data.find(b'\n')
    length = len(state.line) + (end + 1 if end >= 0 else len(data))
    if length > 4096:
        return State('invalid'), 'long'
    if end >= 0 and end + 1 != len(data):
        return State('invalid'), 'extra'
    line = state.line + data
    if end < 0:
        return State(line=line), None
    if line == b'\n' or b'\r' in line or b'\0' in line:
        return State('invalid'), 'malformed'
    try:
        line.decode('utf-8', 'strict')
    except UnicodeError:
        return State('invalid'), 'malformed'
    return State('received', line), 'received'


def take(state):
    """Move a received framed line to the host exactly once."""
    if state.status == 'received':
        return State('taken'), state.line
    return state, None


def drop(state):
    """Forget bytes while preserving whether the terminating LF was observed."""
    if state.status == 'waiting':
        return State('draining')
    if state.status == 'received':
        return State('taken')
    return State(state.status)
