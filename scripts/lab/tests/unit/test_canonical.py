import hashlib
import json
from pathlib import Path
import unittest
from runner import canonical as c
from runner.records import Refused


class Canonical(unittest.TestCase):
    def test_fixed_vectors(self):
        vectors = json.loads((Path(__file__).parent / 'vectors/canonical.json').read_text())
        for vector in vectors:
            with self.subTest(vector=vector['bytes']):
                data = vector['bytes'].encode()
                self.assertEqual(c.record_bytes(c.loads(data)), data)
                self.assertEqual(c.digest(c.loads(data)), vector['digest'])

    def test_refuses_noncanonical_forms(self):
        bad = [b'{"format":"lab-tree/1","format":"lab-tree/1"}\n',
               b'{"format":"lab-tree/1","x":1.0}\n', b'{"format":"lab-tree/1","x":1e3}\n',
               b'{"format":"lab-tree/1","x":NaN}\n', b'{"format":"lab-tree/1","x":Infinity}\n',
               b'{"format":"lab-tree/1","x":9007199254740992}\n',
               '{"format":"lab-tree/1","é":0}\n'.encode(),
               b'{"format":"lab-tree/1","x":"\\ud800"}\n',
               b'{"format":"lab-tree/1","x":-0}\n',
               b'{"format":"lab-tree/1","x":"\\u0041"}\n',
               b'{ "format":"lab-tree/1"}\n', b'{"format":"lab-tree/1"}',
               b'{"format":"lab-tree/1"}\n\n', b'{"format":"lab-future/1"}\n',
               b'{"format":[]}\n', b'[]\n']
        for data in bad:
            with self.subTest(data=data), self.assertRaises(Refused):
                c.loads(data)

    def test_exact_types_and_provenance(self):
        value = {'format': 'lab-tree/1', 'nested': [True, 1, False, 0, None, 'é😀\u000f']}
        self.assertEqual(c.loads(c.record_bytes(value)), value)
        self.assertIs(type(c.loads(c.record_bytes(value))['nested'][0]), bool)
        self.assertIs(type(c.loads(c.record_bytes(value))['nested'][1]), int)
        self.assertEqual(c.digest(value), c.digest({**value, 'about': {'origin': 'other'}}))
        self.assertNotEqual(c.digest(value), c.digest({**value, 'nested': [1, 1, False, 0]}))
        for value in [1.0, {'x': float('nan')}, {1: 'x'}, '\udfff', 1 << 53]:
            with self.subTest(value=repr(value)), self.assertRaises(Refused):
                c.dumps(value)

    def test_manifest_metadata(self):
        a = {'path': 'a', 'bytes': 0, 'sha256': 'sha256:' + hashlib.sha256(b'').hexdigest(), 'exec': False}
        b = {**a, 'path': 'é'}
        self.assertEqual(c.manifest([b, a])['entries'], [a, b])
        self.assertEqual(c.tree_digest([b, a]), c.tree_digest([a, b]))
        self.assertNotEqual(c.tree_digest([a]), c.tree_digest([{**a, 'exec': True}]))
        for update in [{'path': '../a'}, {'path': '/a'}, {'path': 'a/../b'}, {'path': 'a//b'},
                       {'path': '\ud800'}, {'kind': 'link'}, {'kind': 'fifo'}, {'exec': 1}, {'bytes': True}]:
            with self.subTest(update=repr(update)), self.assertRaises(Refused):
                c.manifest([{**a, **update}])
        with self.assertRaises(Refused):
            c.manifest([a, a])
        with self.assertRaises(Refused):
            c.manifest([a, {**a, 'path': 'a/b'}])

    def test_u64_bounds(self):
        for n in [0, 1, (1 << 64) - 1]:
            self.assertEqual(c.u64(c.u64_text(n)), n)
        for text in ['+1', '-1', '01', ' 1', '1 ', '', '18446744073709551616', '١', 1, True]:
            with self.subTest(text=text), self.assertRaises(Refused):
                c.u64(text)
        for n in [-1, 1 << 64, True, 1.0]:
            with self.assertRaises(Refused):
                c.u64_text(n)
