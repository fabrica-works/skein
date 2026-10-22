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


def expectation():
    return dict(id="font", width="phone", region="main", property="font-size", value="12px", cites="design 1")


def entry_expectation():
    return dict(id="links", width="phone", entry='main > link "Inbox"', count=0, cites="design 2")


def departure():
    return dict(id="accepted", width="phone", signature="style/main/color", cites="design 3")


def words_entry():
    return dict(id="main-words", width="phone", region="main", data=[], cites="design 5")


class SceneWords(unittest.TestCase):
    def test_optional_words_and_distinct_regions_at_each_width(self):
        value = valid()
        for entries in (None, [], [dict(words_entry(), data=[r"\d+", r"\d\d:\d\d"])]):
            with self.subTest(words=entries):
                if entries is not None:
                    value["words"] = entries
                self.assertEqual(scene.read(canonical.record_bytes(value)), value)
        value["widths"].append(dict(value["widths"][0], name="desktop"))
        value["regions"].append(dict(value["regions"][0], name="other"))
        value["words"] = [words_entry(), dict(words_entry(), id="desktop-words", width="desktop"),
                          dict(words_entry(), id="other-words", region="other")]
        self.assertEqual(scene.read(canonical.record_bytes(value)), value)

    def test_refuses_each_invalid_member(self):
        rows = [("id", x) for x in ("", "Bad", 1)] + [
            (key, x) for key in ("width", "region") for x in ("absent", [], None)] + [
            ("cites", x) for x in ("", 0, None)] + [("data", x) for x in ({}, "x", None)]
        for key, bad in rows:
            with self.subTest(key=key, bad=bad):
                value = dict(valid(), words=[dict(words_entry(), **{key: bad})])
                self.assertIn("words.0." + key, {r.member for r in scene.read(canonical.record_bytes(value))})
        for key in words_entry():
            with self.subTest(missing=key):
                item = words_entry()
                del item[key]
                value = dict(valid(), words=[item])
                self.assertIn("words.0." + key, {r.member for r in scene.read(canonical.record_bytes(value))})
        value = dict(valid(), words=[dict(words_entry(), extra=True)])
        self.assertIn("words.0.extra", {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_refuses_list_and_entry_types_and_patterns(self):
        for entries, member in ((None, "words"), ({}, "words"), ([None], "words.0"),
                                ([[]], "words.0"), (["x"], "words.0")):
            with self.subTest(entries=entries):
                value = dict(valid(), words=entries)
                self.assertIn(member, {r.member for r in scene.read(canonical.record_bytes(value))})
        for patterns, member in (([1], "words.0.data.0"), ([None], "words.0.data.0"),
                                 (["["], "words.0.data.0"), ([""], "words.0.data.0"),
                                 ([r"\d*"], "words.0.data.0"), ([r"\d+", r"\d+"], "words.0.data.1")):
            with self.subTest(patterns=patterns):
                value = dict(valid(), words=[dict(words_entry(), data=patterns)])
                self.assertIn(member, {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_requires_mockup_and_region_once_per_width(self):
        value = dict(valid(), words=[words_entry()])
        value["mockup"] = value["regions"][0]["mockup"] = None
        self.assertIn("words.0", {r.member for r in scene.read(canonical.record_bytes(value))})
        value = dict(valid(), words=[words_entry(), dict(words_entry(), id="again")])
        self.assertEqual({r.member for r in scene.read(canonical.record_bytes(value))}, {"words.1.region"})

    def test_ids_unique_across_all_three_lists(self):
        for kind, item in (("expectations", expectation()), ("departures", departure()), ("words", words_entry())):
            with self.subTest(kind=kind):
                value = dict(valid(), words=[words_entry()])
                if kind == "words":
                    value["words"].append(dict(words_entry(), width="absent"))
                    member = "words.1.id"
                else:
                    value[kind] = [dict(item, id="main-words")]
                    member = "words.0.id"
                self.assertIn(member, {r.member for r in scene.read(canonical.record_bytes(value))})


class SceneChecks(unittest.TestCase):
    def test_accepts_both_expectation_and_departure_forms(self):
        value = valid()
        value["expectations"] = [expectation(), entry_expectation()]
        value["departures"] = [departure(), dict(id="entry", width="phone", entry="main > button", cites="design 4")]
        self.assertEqual(scene.read(canonical.record_bytes(value)), value)

    def test_accepts_each_property_and_signature_punctuation(self):
        from look.style import PROPERTIES
        value = valid()
        value["expectations"] = [dict(expectation(), id="prop-" + prop, property=prop)
                                 for prop in PROPERTIES + ["faces", "x", "y", "width", "height", "shown"]]
        value["departures"] = [dict(departure(), signature="missing/link/Inbox%20%22~._-:80")]
        self.assertEqual(scene.read(canonical.record_bytes(value)), value)

    def test_refuses_each_invalid_check_member(self):
        rows = [("expectations", expectation(), "id", x) for x in ("", "Bad", 1)] + [
            ("expectations", expectation(), "width", x) for x in ("absent", [], None)] + [
            ("expectations", expectation(), "region", x) for x in ("absent", [], None)] + [
            ("expectations", expectation(), "property", x) for x in ("unknown", [], None)] + [
            ("expectations", expectation(), "value", x) for x in (1, None)] + [
            ("expectations", entry_expectation(), "count", x) for x in (-1, True, "1", None)] + [
            ("expectations", entry_expectation(), "entry", x) for x in ("", 1)] + [
            ("departures", departure(), "signature", x) for x in ("", "/style/main/color", "a b", "é", "a\n", "\x7f", 1)] + [
            ("departures", departure(), "id", x) for x in ("", "Bad", 1)] + [
            ("departures", departure(), "width", x) for x in ("absent", [], None)] + [
            ("departures", dict(id="entry", width="phone", entry="main", cites="spec"), "entry", x) for x in ("", 1)]
        for kind, item, key, bad in rows:
            with self.subTest(kind=kind, key=key, bad=bad):
                value = valid()
                value[kind] = [dict(item, **{key: bad})]
                self.assertIn(f"{kind}.0.{key}", {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_refuses_missing_unknown_and_mixed_form_members(self):
        rows = [("expectations", expectation(), {}), ("expectations", entry_expectation(), {}),
                ("departures", departure(), {}),
                ("departures", dict(id="entry", width="phone", entry="main", cites="spec"), {"entry": "signature"})]
        for kind, item, missing_member in rows:
            for key in item:
                with self.subTest(kind=kind, missing=key):
                    value = valid()
                    value[kind] = [{k: v for k, v in item.items() if k != key}]
                    self.assertIn(f"{kind}.0.{missing_member.get(key, key)}",
                                  {r.member for r in scene.read(canonical.record_bytes(value))})
            with self.subTest(kind=kind, unknown=True):
                value = valid()
                value[kind] = [dict(item, extra=True)]
                self.assertIn(f"{kind}.0.extra", {r.member for r in scene.read(canonical.record_bytes(value))})
        value = valid()
        value["expectations"] = [dict(expectation(), entry="main", count=1)]
        value["departures"] = [dict(departure(), entry="main")]
        members = {r.member for r in scene.read(canonical.record_bytes(value))}
        self.assertTrue({"expectations.0.region", "expectations.0.property", "expectations.0.value", "departures.0.signature"} <= members)

    def test_refuses_empty_cites_and_nonobject_items_in_both_lists(self):
        for kind, item in (("expectations", expectation()), ("departures", departure())):
            for cites in ("", 0, None):
                with self.subTest(kind=kind, cites=cites):
                    value = valid()
                    value[kind] = [dict(item, cites=cites)]
                    self.assertIn(f"{kind}.0.cites", {r.member for r in scene.read(canonical.record_bytes(value))})
            for item in (None, [], "x"):
                with self.subTest(kind=kind, item=item):
                    value = valid()
                    value[kind] = [item]
                    self.assertIn(f"{kind}.0", {r.member for r in scene.read(canonical.record_bytes(value))})

    def test_ids_are_unique_within_and_across_both_lists(self):
        for kind, item in (("expectations", expectation()), ("departures", departure())):
            with self.subTest(kind=kind):
                value = valid()
                value[kind] = [item, item.copy()]
                self.assertIn(f"{kind}.1.id", {r.member for r in scene.read(canonical.record_bytes(value))})
        value = valid()
        value["expectations"] = [expectation()]
        value["departures"] = [dict(departure(), id="font")]
        self.assertIn("departures.0.id", {r.member for r in scene.read(canonical.record_bytes(value))})
