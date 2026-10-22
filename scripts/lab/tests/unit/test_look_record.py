"""The capture's canonical form and nested shapes (lab-browser.md, 5.2)."""
import copy
import unittest
from runner import canonical
from runner.records import Refused
from look import canon, record, style


def sample():
    file = record.file("chat/phone/page/tree.json", b"{}\n")
    refused = dict(refused="limit", detail="too large")
    side = dict(address="http://127.0.0.1:1/", ready="met", trouble=[], tree=file, outline=file, shot=refused)
    part = dict(found=True, more=0, box=dict(x=0, y=0, width=1, height=2),
                values={p: "normal" for p in style.PROPERTIES}, faces=[dict(family="DejaVu Sans", custom=False, glyphs=1)], shot=file)
    region = dict(name="main", mockup=None, page=part, pair=None, content="unknown", diff=None,
                  psnr=None, differing=None, pixels=None)
    width = dict(name="phone", width=360, height=480, mobile=False, mockup=None, page=side,
                 structure=None, style=file, pair=None, regions=[region])
    return record.build("Chrome", [dict(name="chat", scene="sha256:" + "a" * 64, widths=[width])])


class Record(unittest.TestCase):
    def test_file_entry_has_prefixed_digest_and_exact_byte_count(self):
        self.assertEqual(record.file("empty.txt", b""), {
            "path": "empty.txt", "bytes": 0,
            "sha256": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"})

    def test_capture_canonical_round_trip_and_order(self):
        value = sample()
        data = canonical.record_bytes(value)
        self.assertTrue(data.startswith(b'{"browser":"Chrome","format":"look-capture/1","scenes":['))
        self.assertTrue(data.endswith(b"\n"))
        self.assertEqual(record.check(canon.loads(data, "look-capture/1")), value)
        self.assertEqual(canonical.record_bytes(record.build("Chrome", [])),
                         b'{"browser":"Chrome","format":"look-capture/1","scenes":[]}\n')

    def test_refuses_wrong_nested_shapes_and_file_metadata(self):
        mutations = [((), "extra", 1), ((), "format", "other"), ((), "browser", 1),
                     (("scenes", 0), "scene", "a" * 64),
                     (("scenes", 0, "widths", 0), "mobile", 0),
                     (("scenes", 0, "widths", 0, "style"), "path", "../outside"),
                     (("scenes", 0, "widths", 0, "style"), "bytes", True),
                     (("scenes", 0, "widths", 0, "style"), "sha256", "a" * 64),
                     (("scenes", 0, "widths", 0, "page"), "ready", "done"),
                     (("scenes", 0, "widths", 0, "regions", 0), "content", "equal"),
                     (("scenes", 0, "widths", 0, "regions", 0), "psnr", 1),
                     (("scenes", 0, "widths", 0, "regions", 0, "page"), "more", -1),
                     (("scenes", 0, "widths", 0, "regions", 0, "page"), "values", {})]
        for path, key, item in mutations:
            with self.subTest(path=path, key=key):
                value = copy.deepcopy(sample())
                target = value
                for component in path:
                    target = target[component]
                target[key] = item
                with self.assertRaises(Refused):
                    record.check(value)

    def test_missing_parts_and_refusals_stay_in_their_place(self):
        value = sample()
        width = value["scenes"][0]["widths"][0]
        width["regions"][0]["page"] = {"found": False}
        width["page"]["ready"] = dict(refused="timeout", detail="wait")
        self.assertEqual(record.check(value), value)
        width["regions"][0]["page"] = dict(refused="gone", detail="node")
        self.assertEqual(record.check(value), value)
