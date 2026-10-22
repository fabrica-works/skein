"""Calls and reader's lines, without host effects (lab-browser.md, 8.3, 8.8)."""
import json
import re


class Usage(ValueError):
    """A command does not name a call."""


class Unreadable(ValueError):
    """An answer does not have the call's shape."""


def address(value):
    found = re.fullmatch(r'(127\.0\.0\.1|\[::1\]):([0-9]{1,5})', value)
    if not found or not 1 <= int(found[2]) <= 65535:
        raise Usage('ADDRESS must be 127.0.0.1:PORT or [::1]:PORT, port 1-65535')
    return ('::1' if found[1] == '[::1]' else found[1], int(found[2]))


def call(argv):
    if not argv:
        raise Usage('missing verb')
    verb, *args = argv
    operands, within = [], []
    exact, gone, seconds = False, False, None
    while args:
        item = args.pop(0)
        if item == '--in':
            if len(args) < 2 or any(a.startswith('--') for a in args[:2]):
                raise Usage('--in needs ROLE NAME')
            within.append(dict(role=args.pop(0), name=args.pop(0), match='words'))
        elif item == '--exact':
            if exact:
                raise Usage('repeated --exact')
            exact = True
        elif item == '--gone':
            if gone:
                raise Usage('repeated --gone')
            gone = True
        elif item == '--timeout':
            if seconds is not None or not args or not re.fullmatch(r'[0-9]+', args[0]):
                raise Usage('--timeout needs an integer from 1 to 300')
            try:
                seconds = int(args.pop(0))
            except ValueError as error:
                raise Usage('--timeout needs an integer from 1 to 300') from error
            if not 1 <= seconds <= 300:
                raise Usage('--timeout needs an integer from 1 to 300')
        elif item.startswith('--'):
            raise Usage('unknown option')
        else:
            operands.append(item)
    counts = {'outline': 0, 'press': 2, 'type': 2, 'key': 1, 'choose': 3,
              'go': 1, 'reload': 0, 'back': 0, 'forward': 0, 'address': 0,
              'wait': 2, 'shot': 0, 'trouble': 0}
    if verb not in counts:
        raise Usage('unknown verb')
    if len(operands) != counts[verb]:
        raise Usage(verb + ' needs ' + str(counts[verb]) + ' operands')
    finder = verb in {'press', 'type', 'choose', 'wait'}
    if (within and not (finder or verb == 'outline')) or (exact and not finder):
        raise Usage('finding options need a part')
    if (gone or seconds is not None) and verb != 'wait':
        raise Usage('--gone and --timeout need wait')
    if verb == 'wait' and seconds is None:
        raise Usage('wait needs --timeout')
    if finder:
        result = dict(call=verb, role=operands[0], name=operands[1],
                      match='exact' if exact else 'words', **{'in': within})
        if verb == 'choose':
            result['option'] = dict(name=operands[2], match=result['match'])
        elif verb == 'wait':
            result.update(gone=gone, seconds=seconds)
        return result
    if verb == 'outline':
        return {'call': verb, 'in': within}
    if verb == 'key':
        key = operands[0]
        if key not in {'Enter', 'Shift+Enter', 'Escape', 'Tab', 'Shift+Tab'}:
            raise Usage('unsupported KEY')
        return dict(call=verb, key=key.removeprefix('Shift+'), shift=key.startswith('Shift+'))
    if verb == 'go':
        path = operands[0]
        if not path.startswith('/') or path.startswith('//') or any(not 0x21 <= ord(c) <= 0x7e for c in path):
            raise Usage('PATH must start with /, not //, and contain printable ASCII without spaces')
        return dict(call=verb, path=path)
    if verb in {'reload', 'back', 'forward'}:
        return dict(call='history', to=verb)
    return {'call': verb}


def shape(value, fields):
    if type(value) is not dict or set(value) != set(fields):
        raise Unreadable('unexpected members')
    for name, kind in fields.items():
        if type(value[name]) is not kind:
            raise Unreadable('unexpected member type')


def nonnegative(value):
    if type(value) is not int or value < 0:
        raise Unreadable('expected nonnegative integer')


def quote(value):
    return json.dumps(value, ensure_ascii=False)


def part(value):
    shape(value, dict(role=str, name=str, value=str, states=list))
    if not value['role'] or any(type(s) is not str or s not in
                              {'focused', 'disabled', 'checked', 'expanded', 'selected'}
                              for s in value['states']):
        raise Unreadable('invalid PART')
    return (value['role'] + (' ' + quote(value['name']) if value['name'] else '')
            + (' = ' + quote(value['value']) if value['value'] else '')
            + ''.join(' [' + state + ']' for state in value['states']))


def text(call, answer):
    if type(answer) is not dict:
        raise Unreadable('expected answer object')
    if 'refused' in answer:
        shape(answer, dict(refused=str, detail=str))
        return 1, ['refused ' + answer['refused'] + ': ' + answer['detail']]
    verb = call['call']
    if verb in {'press', 'type', 'choose'}:
        key = {'press': 'pressed', 'type': 'typed', 'choose': 'chose'}[verb]
        shape(answer, {key: dict})
        return 0, [key + ' ' + part(answer[key])]
    if verb in {'go', 'history', 'address', 'sign-in'}:
        key = {'go': 'went', 'history': 'went', 'address': 'address', 'sign-in': 'opened'}[verb]
        shape(answer, {key: str})
        return 0, [('' if key == 'address' else key + ' ') + answer[key]]
    if verb == 'key':
        shape(answer, dict(keyed=str))
        key = ('Shift+' if call['shift'] else '') + call['key']
        if answer['keyed'] != key:
            raise Unreadable('unexpected key')
        return 0, ['keyed ' + key]
    if verb == 'wait':
        if 'met' in answer:
            shape(answer, dict(met=dict))
            return 0, ['met ' + part(answer['met'])]
        if 'gone' in answer:
            shape(answer, dict(gone=bool))
            if answer['gone'] is not True:
                raise Unreadable('expected gone true')
            return 0, ['gone']
        shape(answer, dict(missed=list))
        if len(answer['missed']) > 8:
            raise Unreadable('too many parts')
        return 0, (['missed'] + ['  ' + part(p) for p in answer['missed']]
                   if answer['missed'] else ['missed: nothing of its role'])
    if verb == 'outline':
        shape(answer, dict(outline=str, left=int))
        nonnegative(answer['left'])
        lines = answer['outline'].split('\n')
        if lines[-1] == '':
            lines.pop()
        if answer['left']:
            lines.append(f"({answer['left']} entries cut)")
        return 0, lines
    if verb == 'shot':
        shape(answer, dict(shot=dict))
        shot = answer['shot']
        shape(shot, dict(file=str, bytes=int, sha256=str, width=int, height=int))
        for key in ('bytes', 'width', 'height'):
            nonnegative(shot[key])
        if not re.fullmatch(r'[0-9a-f]{64}', shot['sha256']) or not shot['width'] or not shot['height']:
            raise Unreadable('invalid shot')
        return 0, [f"shot {shot['file']} {shot['width']}x{shot['height']} {shot['bytes']} bytes sha256 {shot['sha256']}"]
    if verb == 'trouble':
        shape(answer, dict(trouble=list, more=int))
        nonnegative(answer['more'])
        if len(answer['trouble']) > 64:
            raise Unreadable('too many entries')
        lines = []
        for entry in answer['trouble']:
            shape(entry, dict(kind=str, address=str, text=str))
            lines.append(entry['kind'] + ' ' + entry['address'] + ' ' + quote(entry['text']))
        if answer['more']:
            lines.append(f"({answer['more']} more)")
        return 0, lines or ['no trouble']
    raise Unreadable('unknown answer')
