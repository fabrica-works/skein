"""Exclusive held-file handoffs; only metadata is inspected during a run (lab.md, 4.8)."""
import os
import re
import stat
from . import canonical
from .records import NAME, Refused


def valid_file(info):
    return (info is not None and stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
            and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1)


def directory(lab, path):
    fd = lab.directory(path, create=True)
    try:
        if stat.S_IMODE(os.fstat(fd).st_mode) != 0o700:
            raise Refused(path, 'held directory must be 0700')
    finally:
        os.close(fd)


def taken_record(lab, name):
    try:
        record = canonical.loads(lab.read('held/' + name + '.taken'))
    except FileNotFoundError:
        return None
    if (set(record) != {'format', 'name', 'run', 'taken'} or record['format'] != 'lab-held/1'
            or record['name'] != name or type(record['run']) is not str
            or not re.fullmatch(r'[a-zA-Z0-9-]+', record['run'])
            or type(record['taken']) is not str
            or not re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z', record['taken'])):
        raise Refused('held.' + name, 'invalid taken record')
    return record


def availability(lab, names):
    result = {}
    for name in names if type(names) is list else ():
        if type(name) is not str or not NAME.fullmatch(name):
            continue
        try:
            parent = lab.metadata('held')
            if parent is not None and (not stat.S_ISDIR(parent.st_mode)
                                       or stat.S_IMODE(parent.st_mode) != 0o700):
                raise Refused('held', 'held directory must be 0700')
            info = lab.metadata('held/' + name)
            if info is not None:
                result[name] = 'kept' if valid_file(info) else 'expected an owned regular 0600 file with one link'
            else:
                record = taken_record(lab, name)
                result[name] = 'taken by run ' + record['run'] if record else 'not held'
        except (OSError, Refused) as error:
            result[name] = str(error)
    return result


def listing(lab):
    try:
        fd = lab.directory('held')
    except FileNotFoundError:
        return []
    try:
        names = {n.removesuffix('.taken') for n in os.listdir(fd)}
    finally:
        os.close(fd)
    lines = []
    for name in sorted(names):
        if not NAME.fullmatch(name):
            raise Refused('held', 'invalid name in held store')
        status = availability(lab, [name])[name]
        if status == 'kept':
            lines.append(name + ' kept')
        elif status.startswith('taken by run '):
            record = taken_record(lab, name)
            lines.append(f"{name} taken {record['run']} {record['taken']}")
        else:
            raise Refused('held.' + name, status)
    return lines


def put(lab, name, source):
    if type(name) is not str or not NAME.fullmatch(name):
        raise Refused('held.name', 'invalid name')
    directory(lab, 'held')
    if lab.metadata('held/' + name) is not None:
        raise Refused('held.' + name, 'already kept')
    lab.move_in(source, 'held/' + name)
    if lab.metadata('held/' + name + '.taken') is not None:
        lab.remove('held/' + name + '.taken')


def take(lab, base, name, run, taken):
    directory(lab, 'held')
    directory(lab, base + '/held')
    status = availability(lab, [name])[name]
    if status != 'kept':
        raise Refused('held.' + name, status)
    lab.write_once('held/' + name + '.taken', canonical.record_bytes(
        dict(format='lab-held/1', name=name, run=run, taken=taken)))
    lab.move('held/' + name, base + '/held/' + name)


def give_back(lab, base, name):
    if not valid_file(lab.metadata(base + '/held/' + name)):
        return False
    lab.move(base + '/held/' + name, 'held/' + name)
    lab.remove('held/' + name + '.taken')
    return True
