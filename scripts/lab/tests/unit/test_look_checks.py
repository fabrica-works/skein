"""The expected page and check command's contract (lab-browser.md, 5.4)."""
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from runner import canonical, checks as runner_checks
from look import check, checks, record, style
from .test_look_scene import valid, expectation, entry_expectation, departure, words_entry

REFUSED = dict(refused="protocol", detail="call failed")


def box(x=0, y=0, width=10, height=10):
    return dict(x=x, y=y, width=width, height=height)


def part(**changes):
    return dict(dict(found=True, more=0, box=box(), values={p: "normal" for p in style.PROPERTIES},
                     faces=[dict(family="DejaVu Sans", custom=False, glyphs=1)], shot=None), **changes)


def side(**changes):
    file = record.file("unused", b"")
    return dict(dict(address="http://127.0.0.1:1/", ready="met", trouble=[], tree=file, outline=file, shot=file), **changes)


def fixture(mockup=True):
    scene = valid()
    if not mockup:
        scene["mockup"] = scene["regions"][0]["mockup"] = None
    region = dict(name="main", mockup=part() if mockup else None, page=part(),
                  pair=None, content="unknown", diff=None, psnr=None, differing=None, pixels=None)
    width = dict(**scene["widths"][0], mockup=side() if mockup else None, page=side(),
                 structure=None, style=record.file("unused", b""), pair=None, regions=[region])
    return scene, width


def tails(result):
    return [tail for tail, _ in result.fails]


class Entries(unittest.TestCase):
    def test_counts_entries_under_enclosing_landmarks(self):
        text = 'banner\n  navigation "Menu"\n    link "Inbox"\n    link "Inbox"\n  button\nmain\n  list (3)\n  heading 2\n'
        self.assertEqual(checks.entries(text), {
            "banner": 1, 'banner > navigation "Menu"': 1, 'banner > navigation "Menu" > link "Inbox"': 2,
            "banner > button": 1, "main": 1, "main > list (3)": 1, "main > heading 2": 1})

    def test_signatures_encode_names_and_keep_nameless_roles(self):
        rows = [('banner > link "Adopt a repository"', "missing/link/Adopt%20a%20repository"),
                ('heading 2', 'missing/heading-2'), ('list (3)', 'missing/list-3'),
                ('button "é/\\\"% > ._~-"', 'missing/button/%C3%A9%2F%22%25%20%3E%20._~-'),
                ('navigation "A > B" > link "C > D"', 'missing/link/C%20%3E%20D')]
        for entry, expected in rows:
            with self.subTest(entry=entry):
                self.assertEqual(checks.entry_signature("missing", entry), expected)


class Order(unittest.TestCase):
    def test_vertical_tolerance_from_both_sides_of_pixel(self):
        rows = [(9, ["b"]), (8, [])]
        for top, expected in rows:
            with self.subTest(top=top):
                self.assertEqual(checks.order([box(), box(y=top)], [box(y=20), box()], ["a", "b"]), expected)

    def test_horizontal_tolerance_from_both_sides_of_pixel(self):
        for left, expected in [(9, ["b"]), (8, [])]:
            with self.subTest(left=left):
                self.assertEqual(checks.order([box(), box(x=left)], [box(x=20), box()], ["a", "b"]), expected)

    def test_only_region_moved_past_two_is_named(self):
        self.assertEqual(checks.order([box(y=0), box(y=20), box(y=40)],
                                     [box(y=40), box(y=0), box(y=20)], ["moved", "b", "c"]), ["moved"])

    def test_tie_names_later_region_in_scene(self):
        self.assertEqual(checks.order([box(), box(y=20)], [box(y=20), box()], ["a", "b"]), ["b"])

    def test_nested_crossing_and_undrawn_pairs_have_no_order(self):
        for unordered in ([box(width=30, height=30), box(x=5, y=5)],
                          [box(width=30), box(x=10, y=5, height=30)], [None, box()]):
            with self.subTest(unordered=unordered):
                self.assertEqual(checks.order(unordered, [box(y=20), box()], ["a", "b"]), [])
                self.assertEqual(checks.order([box(), box(y=20)], unordered, ["a", "b"]), [])

    def test_removals_are_returned_in_scene_order(self):
        self.assertEqual(checks.order([box(y=i) for i in (0, 20, 40)],
                                     [box(y=i) for i in (40, 20, 0)], ["a", "b", "c"]), ["b", "c"])


class Structure(unittest.TestCase):
    def test_base_agreement_has_no_failures_or_undecided(self):
        scene, width = fixture()
        self.assertEqual(checks.structure(scene, width, 'main\n  link "Inbox"\n', 'main\n  link "Inbox"\n'), checks.Result([], []))

    def test_missing_extra_and_same_signature_entries_are_grouped(self):
        scene, width = fixture()
        result = checks.structure(scene, width, 'banner\n  button\nmain\n  button\n', 'banner\nmain\n  heading 2\n')
        self.assertEqual(tails(result), ["missing/button", "extra/heading-2"])
        self.assertIn("banner > button", result.fails[0][1])
        self.assertIn("main > button", result.fails[0][1])

    def test_entry_expectation_overrides_mockup_count(self):
        scene, width = fixture()
        scene["expectations"] = [entry_expectation()]
        self.assertEqual(checks.structure(scene, width, 'main\n  link "Inbox"\n', 'main\n'), checks.Result([], []))

    def test_scene_without_mockup_reports_extras(self):
        scene, width = fixture(False)
        self.assertEqual(tails(checks.structure(scene, width, None, "main\n")), ["extra/main"])

    def test_scene_without_mockup_uses_expectations_alone(self):
        scene, width = fixture(False)
        scene["expectations"] = [dict(entry_expectation(), entry="main", count=1)]
        self.assertEqual(checks.structure(scene, width, None, "main\n"), checks.Result([], []))

    def test_missed_and_refused_readiness_have_side_signatures(self):
        scene, width = fixture()
        width["mockup"]["ready"] = "missed"
        width["page"]["ready"] = REFUSED
        result = checks.structure(scene, width, "", "")
        self.assertEqual(tails(result), ["ready/mockup", "ready/page"])
        self.assertEqual(result.undecided, ["chat/phone/page: ready protocol: call failed"])

    def test_trouble_names_each_kind_and_network_host_once(self):
        scene, width = fixture()
        width["mockup"]["trouble"] = [dict(kind="network", text="fonts.example:443")] * 2
        width["page"]["trouble"] = [dict(kind=kind, text="reported") for kind in ("exception", "console", "log", "crash", "dialog")]
        width["page"]["trouble"] += [dict(kind="network", text=host) for host in ("a.example:80", "a.example:80", "b.example:443")]
        self.assertEqual(tails(checks.structure(scene, width, "", "")), [
            "trouble/mockup/network/fonts.example:443", "trouble/page/exception", "trouble/page/console",
            "trouble/page/log", "trouble/page/crash", "trouble/page/dialog", "trouble/page/network/a.example:80",
            "trouble/page/network/b.example:443"])

    def test_missing_page_region_uses_its_finder_name(self):
        scene, width = fixture()
        scene["regions"][0]["page"] = dict(role="textbox", name="A/B", match="exact")
        width["regions"][0]["page"] = dict(found=False)
        self.assertEqual(tails(checks.structure(scene, width, "", "")), ["missing/textbox/A%2FB"])

    def test_expectation_requires_region_without_mockup(self):
        scene, width = fixture(False)
        scene["expectations"] = [expectation()]
        width["regions"][0]["page"] = dict(found=False)
        self.assertEqual(tails(checks.structure(scene, width, None, "")), ["missing/main"])

    def test_no_mockup_or_expectation_does_not_require_region(self):
        scene, width = fixture(False)
        width["regions"][0]["page"] = dict(found=False)
        self.assertEqual(checks.structure(scene, width, None, ""), checks.Result([], []))

    def test_structure_reports_region_order(self):
        scene, width = fixture()
        scene["regions"].append(dict(scene["regions"][0], name="second"))
        width["regions"].append(dict(width["regions"][0], name="second", mockup=part(box=box(y=20)), page=part(box=box())))
        width["regions"][0]["page"]["box"] = box(y=20)
        self.assertEqual(tails(checks.structure(scene, width, "", "")), ["order/second"])

    def test_departure_by_entry_accepts_only_its_landmark(self):
        scene, width = fixture()
        scene["departures"] = [dict(id="accepted", width="phone", entry="banner > button", cites="spec")]
        result = checks.structure(scene, width, "banner\n  button\nmain\n  button\n", "banner\nmain\n")
        self.assertEqual(tails(result), ["missing/button"])
        self.assertEqual(result.fails[0][1], "main > button: page 0, expected 1 (mockup)")

    def test_departure_by_signature_accepts_all_its_entries(self):
        scene, width = fixture()
        scene["departures"] = [dict(departure(), signature="missing/button")]
        self.assertEqual(checks.structure(scene, width, "banner\n  button\nmain\n  button\n", "banner\nmain\n"), checks.Result([], []))

    def test_unused_departure_belongs_to_its_check(self):
        scene, width = fixture()
        scene["departures"] = [departure(), dict(departure(), id="structure", signature="order/main"),
                               dict(id="entry", width="phone", entry="main", cites="spec")]
        self.assertEqual(tails(checks.structure(scene, width, "", "")), ["departure-gone/structure", "departure-gone/entry"])
        self.assertEqual(tails(checks.style(scene, width)), ["departure-gone/accepted"])

    def test_other_widths_expectations_and_departures_are_ignored(self):
        scene, width = fixture()
        scene["expectations"] = [dict(expectation(), width="desktop")]
        scene["departures"] = [dict(departure(), width="desktop")]
        self.assertEqual(checks.structure(scene, width, "", ""), checks.Result([], []))
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_refused_outline_preserves_explicit_expectations(self):
        scene, width = fixture()
        scene["expectations"] = [dict(entry_expectation(), count=1)]
        width["mockup"]["outline"] = REFUSED
        result = checks.structure(scene, width, None, "extra\n")
        self.assertEqual(tails(result), ["missing/link/Inbox"])
        self.assertEqual(result.undecided, ["chat/phone/mockup: outline protocol: call failed"])

    def test_side_refusals_are_undecided_without_fabricated_differences(self):
        for call in ("trouble", "outline"):
            with self.subTest(call=call):
                scene, width = fixture()
                width["page"][call] = REFUSED
                result = checks.structure(scene, width, "", None)
                self.assertEqual(result.fails, [])
                self.assertEqual(result.undecided, [f"chat/phone/page: {call} protocol: call failed"])

    def test_missing_mockup_and_refused_parts_name_region(self):
        for side_name, part_value, why in (("mockup", dict(found=False), "not found"),
                                           ("mockup", REFUSED, "protocol: call failed"),
                                           ("page", REFUSED, "protocol: call failed")):
            for decide in (lambda s, w: checks.structure(s, w, "", ""), checks.style):
                with self.subTest(side=side_name, part=part_value, decide=decide):
                    scene, width = fixture()
                    width["regions"][0][side_name] = part_value
                    self.assertEqual(decide(scene, width), checks.Result([], [f"chat/phone/main: {side_name} {why}"]))


class Style(unittest.TestCase):
    def test_base_values_faces_and_boxes_agree(self):
        scene, width = fixture()
        width["regions"][0]["page"]["faces"][0]["glyphs"] = 100
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_each_expected_property_has_its_own_signature(self):
        rows = [(prop, ("values", prop), "different") for prop in style.PROPERTIES] + [
            ("faces", ("faces", 0, "custom"), True), ("x", ("box", "x"), 2), ("y", ("box", "y"), 2),
            ("width", ("box", "width"), 12), ("height", ("box", "height"), 12)]
        for prop, path, replacement in rows:
            with self.subTest(prop=prop):
                scene, width = fixture()
                target = width["regions"][0]["page"]
                for member in path[:-1]:
                    target = target[member]
                target[path[-1]] = replacement
                self.assertEqual(tails(checks.style(scene, width)), ["style/main/" + prop])

    def test_box_tolerance_holds_at_one_pixel_and_not_two(self):
        for offset, expected in ((1, []), (-1, []), (2, ["style/main/y"]), (-2, ["style/main/y"])):
            with self.subTest(offset=offset):
                scene, width = fixture()
                width["regions"][0]["page"]["box"]["y"] += offset
                self.assertEqual(tails(checks.style(scene, width)), expected)

    def test_expectation_overrides_one_property_with_citation(self):
        scene, width = fixture()
        scene["expectations"] = [expectation()]
        width["regions"][0]["page"]["values"]["font-size"] = "14px"
        result = checks.style(scene, width)
        self.assertEqual(result.fails, [("style/main/font-size", "page 14px, expected 12px (font: design 1)")])

    def test_page_only_style_uses_explicit_expectations(self):
        scene, width = fixture(False)
        scene["expectations"] = [expectation()]
        width["regions"][0]["page"]["values"]["font-size"] = "13px"
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_faces_expectation_uses_sorted_families_and_web_marker(self):
        scene, width = fixture(False)
        scene["expectations"] = [dict(expectation(), property="faces", value="A (web), B")]
        width["regions"][0]["page"]["faces"] = [dict(family="B", custom=False, glyphs=2), dict(family="A", custom=True, glyphs=1)]
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_shown_difference_skips_box_members(self):
        for a, b in ((box(), None), (None, box())):
            with self.subTest(mockup=a, page=b):
                scene, width = fixture()
                width["regions"][0]["mockup"]["box"] = a
                width["regions"][0]["page"]["box"] = b
                self.assertEqual(tails(checks.style(scene, width)), ["style/main/shown"])

    def test_shown_expectation_can_accept_hidden_page(self):
        scene, width = fixture()
        scene["expectations"] = [dict(expectation(), property="shown", value="no")]
        width["regions"][0]["page"]["box"] = None
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_departure_accepts_style_difference(self):
        scene, width = fixture()
        width["regions"][0]["page"]["values"]["color"] = "red"
        scene["departures"] = [departure()]
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_both_hidden_parts_have_no_box_difference(self):
        scene, width = fixture()
        for side_name in ("mockup", "page"):
            width["regions"][0][side_name]["box"] = None
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_missing_mockup_still_uses_property_expectations(self):
        scene, width = fixture()
        scene["expectations"] = [expectation()]
        width["regions"][0]["mockup"] = dict(found=False)
        result = checks.style(scene, width)
        self.assertEqual(tails(result), ["style/main/font-size"])
        self.assertEqual(result.undecided, ["chat/phone/main: mockup not found"])

    def test_missing_page_has_no_style_failure(self):
        scene, width = fixture()
        width["regions"][0]["page"] = dict(found=False)
        self.assertEqual(checks.style(scene, width), checks.Result([], []))


def words_fixture(a, b, data=()):
    scene, width = fixture()
    scene["words"] = [dict(words_entry(), data=list(data))]
    width["regions"][0].update(mockup=part(words=a), page=part(words=b))
    return scene, width


class Words(unittest.TestCase):
    def test_runs_split_differently_agree(self):
        scene, width = words_fixture(["Inbox ", "2"], ["Inbox 2"])
        self.assertEqual(checks.words(scene, width), checks.Result([], []))

    def test_data_cut_on_both_sides_and_across_runs(self):
        for a, b in ((["At 09:16"], ["At 09:18"]), (["At ", "09:16"], ["At 09:18"])):
            with self.subTest(mockup=a, page=b):
                scene, width = words_fixture(a, b, [r"\d\d:\d\d"])
                self.assertEqual(checks.words(scene, width), checks.Result([], []))

    def test_changed_word_beside_data_still_signs(self):
        scene, width = words_fixture(["12 min ago"], ["12 mins ago"], [r"\d+"])
        result = checks.words(scene, width)
        self.assertEqual(tails(result), ["words/main/missing/min", "words/main/extra/mins"])
        self.assertEqual(result.undecided, [])
        self.assertEqual(result.notes, ())
        for _, summary in result.fails:
            self.assertIn("'min'", summary)
            self.assertIn("'mins'", summary)

    def test_data_patterns_apply_in_declared_order_and_replace_every_match(self):
        scene, width = words_fixture(["09:16 09:16"], ["09:18 09:18"], [r"09:16", r"09:18", "#"])
        self.assertEqual(checks.words(scene, width), checks.Result([], []))

    def test_data_match_with_any_white_space_is_undecided(self):
        for text, pattern in (("12 min", r"\d+ min"), ("12\tmin", r"\d+\smin"),
                              ("12\nmin", r"\d+\smin"), ("12\u00a0min", r"\d+\smin")):
            for side_name in ("mockup", "page"):
                with self.subTest(text=text, side=side_name):
                    scene, width = words_fixture(["unchanged"], ["unchanged"], [pattern])
                    width["regions"][0][side_name]["words"] = [text]
                    result = checks.words(scene, width)
                    self.assertEqual(result.fails, [])
                    self.assertEqual(result.undecided, [f"chat/phone/main-words: {side_name} data {pattern} matched white space in {text!r}"])

    def test_data_match_of_empty_text_is_undecided(self):
        # scene.py refuses a pattern matching the empty text whole; these match empty inside a text.
        for pattern in (r"\b", r"(?=x)"):
            with self.subTest(pattern=pattern):
                scene, width = words_fixture(["max"], ["max"], [pattern])
                result = checks.words(scene, width)
                self.assertEqual(result.fails, [])
                self.assertEqual(result.undecided[0], f"chat/phone/main-words: mockup data {pattern} matched empty text at "
                                 + ("0" if pattern == r"\b" else "2"))

    def test_stale_pattern_is_only_a_note(self):
        scene, width = words_fixture(["Inbox"], ["Inbox"], [r"\d+"])
        self.assertEqual(checks.words(scene, width), checks.Result([], [], (r"chat/phone/main-words: data \d+ matched nothing",)))

    def test_delete_insert_replace_and_encoded_signatures(self):
        rows = [("Inbox Send", "Inbox", ["words/main/missing/Send"]),
                ("Inbox", "Inbox Send", ["words/main/extra/Send"]),
                ("Inbox Send", "Inbox Post", ["words/main/missing/Send", "words/main/extra/Post"]),
                ("é/\"% > ._~-", "", ["words/main/missing/%C3%A9%2F%22%25%20%3E%20._~-"])]
        for a, b, expected in rows:
            with self.subTest(mockup=a, page=b):
                scene, width = words_fixture([a], [b])
                result = checks.words(scene, width)
                self.assertEqual(tails(result), expected)
                self.assertEqual(result.undecided, [])

    def test_moved_word_signs_both_where_it_left_and_arrived(self):
        scene, width = words_fixture(["one two three"], ["two three one"])
        self.assertEqual(tails(checks.words(scene, width)), ["words/main/missing/one", "words/main/extra/one"])

    def test_repeated_words_are_compared_without_junk_heuristic(self):
        scene, width = words_fixture(["start " + "word " * 210], ["word " * 210 + "end"])
        self.assertEqual(tails(checks.words(scene, width)), ["words/main/missing/start", "words/main/extra/end"])

    def test_long_span_bound_and_complete_summary(self):
        for text, expected in (("one two three four five six seven eight", "one%20two%20three%20four%20five%20six%20seven%20eight"),
                               ("one two three four five six seven eight nine", "one%20two%20three%20four%20five%20six%20seven%20eight/9w-5d8c5119")):
            for a, b, kind in (([text], [], "missing"), ([], [text], "extra")):
                with self.subTest(text=text, kind=kind):
                    scene, width = words_fixture(a, b)
                    result = checks.words(scene, width)
                    self.assertEqual(tails(result), ["words/main/" + kind + "/" + expected])
                    self.assertIn(text, result.fails[0][1])

    def test_departure_accepts_words_and_is_gone_only_in_words(self):
        scene, width = words_fixture(["Inbox"], [])
        scene["departures"] = [dict(departure(), signature="words/main/missing/Inbox")]
        self.assertEqual(checks.words(scene, width), checks.Result([], []))
        width["regions"][0]["page"]["words"] = ["Inbox"]
        self.assertEqual(tails(checks.words(scene, width)), ["departure-gone/accepted"])
        self.assertEqual(checks.structure(scene, width, "", ""), checks.Result([], []))
        self.assertEqual(checks.style(scene, width), checks.Result([], []))

    def test_absent_words_list_and_other_widths_do_not_judge_regions(self):
        scene, width = fixture()
        width["regions"][0]["mockup"] = REFUSED
        self.assertEqual(checks.words(scene, width), checks.Result([], []))
        scene["words"] = [dict(words_entry(), width="desktop")]
        scene["departures"] = [dict(departure(), width="desktop", signature="words/main/missing/Inbox")]
        self.assertEqual(checks.words(scene, width), checks.Result([], []))

    def test_missing_refused_and_old_parts_are_undecided_per_entry(self):
        rows = [("mockup", dict(found=False), "not found"),
                ("mockup", REFUSED, "protocol: call failed"), ("page", REFUSED, "protocol: call failed")]
        rows += [(side, part(**changes), "words absent or null")
                 for side in ("mockup", "page") for changes in ({}, dict(words=None))]
        for side_name, value, why in rows:
            with self.subTest(side=side_name, part=value):
                scene, width = words_fixture(["Inbox"], ["Inbox"])
                scene["regions"].append(dict(scene["regions"][0], name="other"))
                scene["words"].append(dict(words_entry(), id="other-words", region="other"))
                width["regions"].append(dict(width["regions"][0], name="other", mockup=part(words=["Send"]), page=part(words=[])))
                width["regions"][0][side_name] = value
                result = checks.words(scene, width)
                self.assertEqual(tails(result), ["words/other/missing/Send"])
                self.assertEqual(result.undecided, [f"chat/phone/main-words: {side_name} {why}"])

    def test_missing_page_signs_nothing(self):
        scene, width = words_fixture(["Inbox"], ["Inbox"])
        width["regions"][0]["page"] = dict(found=False)
        self.assertEqual(checks.words(scene, width), checks.Result([], []))


class Command(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.scene, self.width = fixture()
        self.scene_path = self.root / "scene.json"
        self.scene_data = canonical.record_bytes(self.scene)
        self.scene_path.write_bytes(self.scene_data)
        for side_name in ("mockup", "page"):
            data = b"main\n"
            path = side_name + ".txt"
            (self.root / path).write_bytes(data)
            self.width[side_name]["outline"] = record.file(path, data)
        self.capture = record.build("Chrome", [dict(name="chat", scene=record.file("scene.json", self.scene_data)["sha256"], widths=[self.width])])

    def run_check(self, kind="structure", arguments=None):
        (self.root / "capture.json").write_bytes(canonical.record_bytes(self.capture))
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            status = check.main(arguments if arguments is not None else [kind, "--capture", str(self.root), str(self.scene_path)])
        data = stdout.getvalue().encode()
        lines = [json.loads(line) for line in data.splitlines()]
        self.assertEqual(runner_checks.command("look-" + kind, status, data)[2], [])
        self.assertEqual(data, b"".join(canonical.dumps(line) + b"\n" for line in lines))
        return status, lines

    def test_pass_has_status_zero_and_no_lines(self):
        self.assertEqual(self.run_check(), (0, []))
        self.assertEqual(self.run_check("style"), (0, []))

    def test_failure_has_status_one_and_prefixed_signature(self):
        self.width["page"]["ready"] = "missed"
        self.assertEqual(self.run_check(), (1, [dict(fail="chat/phone/ready/page", summary="page readiness missed")]))

    def test_style_failure_has_status_one_and_property_signature(self):
        self.width["regions"][0]["page"]["values"]["font-size"] = "14px"
        self.assertEqual(self.run_check("style"), (1, [dict(fail="chat/phone/style/main/font-size",
                                                          summary="page 14px, expected normal (mockup)")]))

    def words_scene(self):
        self.scene["words"] = [words_entry()]
        data = canonical.record_bytes(self.scene)
        self.scene_path.write_bytes(data)
        self.capture["scenes"][0]["scene"] = record.file("scene.json", data)["sha256"]
        self.width["regions"][0].update(mockup=part(words=["Inbox"]), page=part(words=["Inbox"]))

    def test_words_statuses_and_lines_without_reading_outlines(self):
        self.words_scene()
        (self.root / "mockup.txt").unlink()
        (self.root / "page.txt").unlink()
        self.assertEqual(self.run_check("words"), (0, []))
        self.width["regions"][0]["page"]["words"] = []
        self.assertEqual(self.run_check("words"), (1, [dict(fail="chat/phone/words/main/missing/Inbox",
                                                          summary="region main: mockup 'Inbox', page ''")]))
        del self.width["regions"][0]["page"]["words"]
        self.assertEqual(self.run_check("words"), (2, [dict(undecided="chat/phone/main-words: page words absent or null")]))

    def test_words_stale_data_note_is_printed_only_on_stderr(self):
        self.words_scene()
        self.scene["words"][0]["data"] = [r"\d+"]
        data = canonical.record_bytes(self.scene)
        self.scene_path.write_bytes(data)
        self.capture["scenes"][0]["scene"] = record.file("scene.json", data)["sha256"]
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(self.run_check("words"), (0, []))
        self.assertEqual(stderr.getvalue(), "chat/phone/main-words: data \\d+ matched nothing\n")

    def test_undecided_only_has_status_two(self):
        self.width["regions"][0]["mockup"] = dict(found=False)
        self.assertEqual(self.run_check(), (2, [dict(undecided="chat/phone/main: mockup not found")]))

    def test_failure_with_undecided_has_status_one(self):
        self.width["page"]["ready"] = "missed"
        self.width["regions"][0]["mockup"] = dict(found=False)
        self.assertEqual(self.run_check(), (1, [dict(fail="chat/phone/ready/page", summary="page readiness missed"),
                                               dict(undecided="chat/phone/main: mockup not found")]))

    def test_scene_digest_mismatch_is_undecided(self):
        self.scene_path.write_bytes(canonical.record_bytes(dict(self.scene, names=[])))
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertIn("scene digest differs", lines[0]["undecided"])

    def test_outline_size_or_digest_mismatch_is_undecided(self):
        for data in (b"main", b"form\n"):
            with self.subTest(data=data):
                (self.root / "page.txt").write_bytes(data)
                status, lines = self.run_check()
                self.assertEqual(status, 2)
                self.assertIn("outline size or digest differs", lines[0]["undecided"])

    def test_unreadable_outline_is_undecided(self):
        (self.root / "page.txt").unlink()
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertIn("page.txt", lines[0]["undecided"])

    def test_style_does_not_read_outline_files(self):
        (self.root / "page.txt").unlink()
        (self.root / "mockup.txt").unlink()
        self.assertEqual(self.run_check("style"), (0, []))

    def test_absent_scene_is_undecided(self):
        self.capture["scenes"] = []
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertIn("expected scene once in capture", lines[0]["undecided"])

    def test_other_scenes_in_record_are_ignored(self):
        other = copy.deepcopy(self.capture["scenes"][0])
        other["name"] = "other"
        other["widths"][0]["page"]["ready"] = "missed"
        self.capture["scenes"].append(other)
        self.assertEqual(self.run_check(), (0, []))

    def test_invalid_scene_members_are_undecided(self):
        self.scene_path.write_bytes(canonical.record_bytes(dict(self.scene, expectations=[{}])))
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertTrue(all("expectations.0." in line["undecided"] for line in lines))

    def test_missing_scene_file_is_undecided(self):
        self.scene_path.unlink()
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertIn("scene.json", lines[0]["undecided"])

    def test_bad_arguments_are_undecided_json_lines(self):
        for args in ([], ["other"], ["style", "--capture"], ["structure", "--capture", str(self.root)]):
            with self.subTest(args=args):
                status, lines = self.run_check(arguments=args)
                self.assertEqual(status, 2)
                self.assertIn("arguments", lines[0]["undecided"])

    def test_malformed_outline_is_undecided(self):
        data = b'link "unterminated\n'
        (self.root / "page.txt").write_bytes(data)
        self.width["page"]["outline"] = record.file("page.txt", data)
        status, lines = self.run_check()
        self.assertEqual(status, 2)
        self.assertIn("chat/phone", lines[0]["undecided"])

    def test_missing_or_invalid_record_is_undecided(self):
        for data in (None, b"{", b'{}\n', b'{"format":"other"}\n'):
            with self.subTest(data=data):
                path = self.root / "capture.json"
                if data is not None:
                    path.write_bytes(data)
                elif path.exists():
                    path.unlink()
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    status = check.main(["structure", "--capture", str(self.root), str(self.scene_path)])
                self.assertEqual(status, 2)
                self.assertIn("undecided", json.loads(stdout.getvalue()))

    def test_incomplete_or_mismatched_capture_is_undecided(self):
        for field, replacement in (("widths", []), ("regions", []), ("width", 2), ("mockup", None)):
            with self.subTest(field=field):
                target = self.capture["scenes"][0] if field == "widths" else self.width
                saved = target[field]
                target[field] = replacement
                status, lines = self.run_check()
                self.assertEqual(status, 2)
                self.assertIn("differ", lines[0]["undecided"])
                target[field] = saved
