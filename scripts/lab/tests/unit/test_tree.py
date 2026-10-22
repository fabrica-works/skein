import io
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
from runner import store, tree
from runner.records import Refused


def archive(entries, *, format=tarfile.USTAR_FORMAT, mode='w'):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode=mode, format=format) as output:
        for name, kind, executable, contents in entries:
            info = tarfile.TarInfo(name)
            info.type = kind
            info.mode = 0o777 if executable else 0o666
            info.size = len(contents)
            if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                info.linkname = 'outside'
            output.addfile(info, io.BytesIO(contents))
    return data.getvalue()


GOOD = [('assets', tarfile.DIRTYPE, False, b''),
        ('assets/nested', tarfile.DIRTYPE, False, b''),
        ('assets/nested/index', tarfile.REGTYPE, False, b'{unrendered}\n'),
        ('run', tarfile.REGTYPE, True, b'#!/bin/sh\n'),
        ('empty', tarfile.DIRTYPE, False, b'')]


def bad_archives():
    file = lambda name: (name, tarfile.REGTYPE, False, b'x')
    yield 'gzip', archive(GOOD, mode='w:gz')
    yield 'pax header', archive([file('x' * 120)], format=tarfile.PAX_FORMAT)
    yield 'gnu header', archive(GOOD, format=tarfile.GNU_FORMAT)
    for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE, tarfile.BLKTYPE):
        yield repr(kind), archive([('special', kind, False, b'')])
    for name in ('', '/absolute', '.', '..', '../escape', 'x/../y', './x', 'x//y', 'x/./y', 'x/'):
        yield repr(name), archive([file(name)])
    yield 'directory empty segment', archive([('x//', tarfile.DIRTYPE, False, b'')])
    yield 'repeated', archive([file('x'), file('x')])
    yield 'repeated directory', archive([('x', tarfile.DIRTYPE, False, b''), ('x/', tarfile.DIRTYPE, False, b'')])
    yield 'file before directory', archive([file('x/y'), ('x', tarfile.DIRTYPE, False, b'')])
    yield 'file as parent', archive([file('x'), ('x/y', tarfile.DIRTYPE, False, b'')])
    yield 'file above directory', archive([('x/y', tarfile.DIRTYPE, False, b''), file('x')])
    yield 'truncated', archive(GOOD)[:600]
    yield 'missing terminator', archive([file('x')])[:1024]
    yield 'trailing archive', archive([])[:1024] + archive(GOOD)
    damaged = bytearray(archive(GOOD))
    damaged[0] ^= 1
    yield 'checksum', bytes(damaged)
    yield 'bad bytes', b'not a tar archive'


class Tree(unittest.TestCase):
    def test_nested_files_executable_and_empty_directory_in_order(self):
        self.assertEqual(tree.members(archive(GOOD)),
                         [(name, 'directory' if kind == tarfile.DIRTYPE else 'file', executable, data)
                          for name, kind, executable, data in GOOD])
        self.assertEqual(tree.members(archive([])), [])

    def test_every_archive_refusal(self):
        for why, data in bad_archives():
            with self.subTest(why=why), self.assertRaises(Refused):
                tree.members(data)

    def test_directory_slash_and_ustar_prefix(self):
        prefix = 'a' * 90
        name = prefix + '/' + 'b' * 90
        entries = [(prefix + '/', tarfile.DIRTYPE, False, b''), (name, tarfile.REGTYPE, False, b'x')]
        self.assertEqual(tree.members(archive(entries))[1], (name, 'file', False, b'x'))

    def test_owner_execute_is_the_only_mode_read(self):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode='w', format=tarfile.USTAR_FORMAT) as output:
            info = tarfile.TarInfo('other-execute')
            info.mode = 0o6077
            output.addfile(info)
        self.assertFalse(tree.members(data.getvalue())[0][2])

    def test_unpack_paths_modes_and_remove_read_only_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            with store.open_lab(Path(temporary) / 'lab') as lab:
                lab.unpack_tree('scratch/server/bundle', archive(GOOD))
                root = lab.root / 'scratch/server/bundle'
                self.assertEqual(stat.S_IMODE(root.stat().st_mode), 0o500)
                for name, kind, executable, data in GOOD:
                    path = root / name
                    expected = 0o500 if kind == tarfile.DIRTYPE or executable else 0o400
                    self.assertEqual(stat.S_IMODE(path.stat().st_mode), expected)
                    if kind == tarfile.REGTYPE:
                        self.assertEqual(path.read_bytes(), data)
                parent = lab.directory('scratch/server')
                try:
                    lab._remove_stage(parent, 'bundle')
                finally:
                    os.close(parent)
                self.assertFalse(root.exists())

    def test_unpack_never_follows_or_replaces_destination_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            outside = root / 'outside'
            outside.mkdir()
            with store.open_lab(root / 'lab') as lab:
                (lab.root / 'link').symlink_to(outside, target_is_directory=True)
                for destination in ('link/bundle', 'link'):
                    with self.subTest(destination=destination), self.assertRaises(OSError):
                        lab.unpack_tree(destination, archive(GOOD))
                self.assertEqual(list(outside.iterdir()), [])
