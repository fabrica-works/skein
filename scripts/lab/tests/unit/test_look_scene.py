"""The scene and canonical boundaries (lab-browser.md, 4)."""
import copy
import unittest
from runner import canonical
from runner.records import Refused
from look import canon, scene


def valid():
    finder = {"role": "main", "name": "", "match": "words"}
    return {"format": "look-scene/1", "name": "chat", "widths": [
        {"name": "phone", "width": 1, "height": 4096, "mobile": False}],
        "mockup": {"path": "/index.html?x=1", "ready": finder.copy()},
        "page": {"path": "/", "ready": finder.copy()},
        "regions": [{"name": "main", "mockup": {"selector": "main"}, "page": finder.copy()}],
        "names": ["Inbox", "Send"], "expectations": [], "departures": []}


class Scene(unittest.TestCase):
    def test_valid_mockup_and_page_only_scenes(self):
        value = valid()
        self.assertEqual(scene.read(canonical.record_bytes(value)), value)
        value["mockup"] = value["regions"][0]["mockup"] = None
        self.assertEqual(scene.read(canonical.record_bytes(value)), value)

    def test_collects_all_members_at_once(self):
        value = valid()
        value.update(name="Bad", extra=True, names=["", "Send", "Send", 0], expectations={}, departures=None)
        value["widths"] = [dict(name="phone", width=0, height=4097, mobile=0),
                           dict(name="phone", width=True, height="480", mobile=False)]
        value["mockup"] = dict(path="//elsewhere", ready={"selector": "main"})
        value["page"] = dict(path="/a b", ready=dict(role="", name=0, match="other"))
        value["regions"] = [dict(name="page", mockup=dict(selector=""), page=dict(selector="main")),
                            dict(name="page", mockup=dict(role="main", name="", match="exact", selector="main"),
                                 page=dict(role="main", name="", match="exact"))]
        members = {r.member for r in scene.read(canonical.record_bytes(value))}
        self.assertEqual(members, {
            "name", "extra", "names.0", "names.2", "names.3", "expectations", "departures",
            "widths.0.width", "widths.0.height", "widths.0.mobile", "widths.1.name", "widths.1.width", "widths.1.height",
            "mockup.path", "mockup.ready.role", "mockup.ready.name", "mockup.ready.match", "mockup.ready.selector",
            "page.path", "page.ready.role", "page.ready.name", "page.ready.match",
            "regions.0.name", "regions.0.mockup.selector", "regions.0.page.role", "regions.0.page.name",
            "regions.0.page.match", "regions.0.page.selector", "regions.1.name",
            "regions.1.mockup.role", "regions.1.mockup.name", "regions.1.mockup.match"})

    def test_missing_unknown_and_mistyped_scene_members(self):
        for key in valid():
            with self.subTest(missing=key):
                value = valid()
                del value[key]
                self.assertIn(key, {r.member for r in scene.read(canonical.record_bytes(value))})
        for key in ("widths", "regions", "names", "expectations", "departures"):
            with self.subTest(mistyped=key):
                value = valid()
                value[key] = {}
                self.assertIn(key, {r.member for r in scene.read(canonical.record_bytes(value))})
        value = valid()
        value["widths"] = []
        self.assertIn("widths", {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_refuses_each_invalid_path_and_size(self):
        for path in ("", "a", "//a", "/a b", "/a\n", "/é", "/\x7f"):
            with self.subTest(path=path):
                value = valid()
                value["page"]["path"] = path
                self.assertIn("page.path", {r.member for r in scene.read(canonical.record_bytes(value))})
        for size in (0, 4097, True, "1", None):
            with self.subTest(size=size):
                value = valid()
                value["widths"][0]["width"] = size
                self.assertIn("widths.0.width", {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_refuses_names_and_wrong_mockup_presence(self):
        for name in ("", "A", "x_y", "../x", 1):
            with self.subTest(name=name):
                value = valid()
                value["regions"][0]["name"] = name
                self.assertIn("regions.0.name", {r.member for r in scene.read(canonical.record_bytes(value))})
        for mockup in (None, valid()["mockup"]):
            with self.subTest(mockup=mockup):
                value = valid()
                value["mockup"] = mockup
                value["regions"][0]["mockup"] = {"selector": "main"} if mockup is None else None
                members = {r.member for r in scene.read(canonical.record_bytes(value))}
                self.assertIn("regions.0.mockup", members)

    def test_refuses_malformed_finders_and_nested_shapes(self):
        for finder in ({}, None, [], {"role": "", "name": "", "match": "words"},
                       {"role": "main", "name": None, "match": "words"},
                       {"role": "main", "name": "", "match": "bad"},
                       {"selector": "main"}, {"role": "main", "name": "", "match": "words", "unknown": 1}):
            with self.subTest(finder=finder):
                value = valid()
                value["regions"][0]["page"] = finder
                self.assertTrue(any(r.member.startswith("regions.0.page") for r in scene.read(canonical.record_bytes(value))))
        for key in ("page", "widths", "regions"):
            value = valid()
            value[key] = None if key == "page" else [None]
            self.assertIsInstance(scene.read(canonical.record_bytes(value)), list)
        value = valid()
        value["regions"].append(copy.deepcopy(value["regions"][0]))
        self.assertIn("regions.1.name", {r.member for r in scene.read(canonical.record_bytes(value))})


class Canonical(unittest.TestCase):
    def test_files_and_answer_lines_use_their_own_envelopes(self):
        self.assertEqual(canon.loads(b'{"ready":{"browser":"Chrome"}}', None), {"ready": {"browser": "Chrome"}})
        self.assertEqual(canon.loads(b'{"format":"look-capture/1"}\n', "look-capture/1"), {"format": "look-capture/1"})

    def test_refuses_noncanonical_or_unsafe_json(self):
        for data in (b'{}\n', b'{"x":1,"x":2}', b'{"x":1.0}', b'{"x":NaN}', b'{"x":Infinity}',
                     b'{"x":-0}', b'{ "x":1}', b'{"x":"\\u0061"}', b'{"x":9007199254740992}',
                     b'{"x":"\\ud800"}', b'{"\xc3\xa9":0}', b'{"x":"\xff"}', b'[]', b'{', "{}"):
            with self.subTest(data=data), self.assertRaises(Refused):
                canon.loads(data, None)
        for data in (b'{"format":"other"}\n', b'{"format":"look-scene/1"}', b'{}\n'):
            with self.subTest(data=data):
                self.assertIsInstance(scene.read(data), list)
