"""An artifact's uncompressed USTAR tree, read from supplied bytes (lab.md, 4.3)."""
import tarfile
from .records import Refused, relative


def members(data):
    """Return ordered (name, kind, exec, data) members, without filesystem effects."""
    if type(data) is not bytes or len(data) % 512:
        raise Refused('tree', 'expected an uncompressed USTAR archive')
    result, seen, directories = [], set(), set()
    offset = 0
    while offset < len(data):
        header = data[offset:offset + 512]
        if header == bytes(512):
            if len(data) - offset < 1024 or any(data[offset:]):
                raise Refused('tree', 'invalid USTAR end blocks')
            return result
        if header[257:265] != b'ustar\x0000':
            raise Refused('tree', 'expected an uncompressed POSIX USTAR header')
        try:
            info = tarfile.TarInfo.frombuf(header, 'utf-8', 'strict')
            name = header[:100].split(b'\0', 1)[0].decode('utf-8', 'strict')
            prefix = header[345:500].split(b'\0', 1)[0].decode('utf-8', 'strict')
            name = prefix + '/' + name if prefix else name
        except (tarfile.HeaderError, UnicodeError, ValueError) as error:
            raise Refused('tree', 'invalid USTAR header or UTF-8 name') from error
        if info.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE):
            raise Refused('tree', 'members must be regular files or directories')
        directory = info.type == tarfile.DIRTYPE
        # USTAR directory headers conventionally end in one slash.
        if directory and name.endswith('/'):
            name = name[:-1]
        if not relative(name):
            raise Refused('tree', 'expected a nonempty relative name without empty, dot or parent segments')
        if name in seen:
            raise Refused('tree', 'repeated member: ' + name)
        parents = name.split('/')[:-1]
        if any('/'.join(parents[:i]) in seen - directories for i in range(1, len(parents) + 1)):
            raise Refused('tree', 'member beneath a regular file: ' + name)
        if not directory and parents and '/'.join(parents) not in directories:
            raise Refused('tree', 'file directory must be listed before it: ' + name)
        if not directory and any(n.startswith(name + '/') for n in seen):
            raise Refused('tree', 'regular file overlaps a directory: ' + name)
        offset += 512
        end = offset + info.size
        padded_end = offset + ((info.size + 511) // 512) * 512
        if info.size < 0 or padded_end > len(data) or directory and info.size:
            raise Refused('tree', 'invalid or truncated member data')
        result.append((name, 'directory' if directory else 'file', bool(info.mode & 0o100), data[offset:end]))
        seen.add(name)
        if directory:
            directories.add(name)
        offset = padded_end
    raise Refused('tree', 'missing USTAR end blocks')
