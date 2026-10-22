"""Structure, style and content decisions (lab-browser.md, 5.3)."""
from pathlib import Path
import unittest
from unittest.mock import Mock
from look import capture, outline, picture, record, style
from .test_look_scene import valid


def entry(node, depth, role, name="", level=0):
    return dict(node=node, depth=depth, role=role, name=name, level=level)


class Structure(unittest.TestCase):
    def test_nested_landmarks_headings_list_counts_and_names(self):
        tree = [entry(1, 0, "RootWebArea"), entry(2, 1, "banner"), entry(3, 2, "navigation", "Menu"),
                entry(4, 3, "link", "Inbox"), entry(5, 1, "main"), entry(6, 2, "heading", "Content", 2),
                entry(7, 2, "list"), entry(8, 3, "listitem"), entry(9, 4, "listitem"), entry(10, 3, "listitem"),
                entry(11, 2, "button", "Send \"now\""), entry(12, 1, "contentinfo"), entry(13, 2, "form"),
                entry(14, 3, "textbox", "é"), entry(15, 1, "button", "Outside")]
        self.assertEqual(outline.outline(tree, ["Menu", "Inbox", 'Send "now"', "é"]),
                         'banner\n  navigation "Menu"\n    link "Inbox"\nmain\n  heading 2\n  list (2)\n'
                         '  button "Send \\"now\\""\ncontentinfo\n  form\n    textbox "é"\nbutton\n')

    def test_all_landmarks_and_controls_are_shown(self):
        roles = ["banner", "navigation", "main", "complementary", "contentinfo", "region", "form", "search",
                 "button", "link", "textbox", "searchbox", "checkbox", "radio", "combobox", "tab", "menuitem", "switch"]
        self.assertEqual(outline.outline([entry(i, 0, role) for i, role in enumerate(roles)], []),
                         "\n".join(roles) + "\n")

    def test_diff_has_named_files_and_equal_is_empty(self):
        self.assertEqual(outline.diff("main\n", "main\n"), "")
        self.assertEqual(outline.diff("main\n", "form\n"),
                         "--- mockup/outline.txt\n+++ page/outline.txt\n@@ -1 +1 @@\n-main\n+form\n")

    def test_content_found_changed_or_unknown(self):
        tree = [entry(1, 0, "main", "Top"), entry(2, 1, "heading", "Hello"), entry(3, 0, "button", "Send")]
        self.assertEqual(picture.content(tree, 1), ["Top", "Hello"])
        self.assertEqual(picture.content(tree, 3), ["Send"])
        self.assertIsNone(picture.content(tree, 4))
        self.assertIsNone(picture.content(tree, None))
        self.assertIsNone(picture.content(None, 1))

    def test_words_are_static_text_descendants_in_tree_order(self):
        tree = [entry(1, 0, "main", "Top"), entry(2, 1, "heading", "Inbox"),
                entry(3, 2, "StaticText", "Inbox "), entry(4, 1, "button", "2"),
                entry(5, 2, "StaticText", "2"), entry(6, 3, "InlineTextBox", "2"),
                entry(7, 1, "StaticText", "Hint"), entry(8, 0, "StaticText", "Outside")]
        self.assertEqual(picture.words(tree, 1), ["Inbox ", "2", "Hint"])
        self.assertEqual(picture.words(tree, 2), ["Inbox "])
        self.assertEqual(picture.words(tree, 4), ["2"])
        self.assertEqual(picture.words(tree, 7), [])
        self.assertEqual(picture.words(tree, 8), [])
        for source, node in ((tree, 9), (tree, None), (None, 1), ([], 1)):
            with self.subTest(tree=source, node=node):
                self.assertIsNone(picture.words(source, node))


class CaptureWords(unittest.TestCase):
    def test_served_and_live_parts_take_words_from_the_existing_tree(self):
        tree = [entry(1, 0, "main", "Inbox"), entry(2, 1, "StaticText", "Inbox "),
                entry(3, 1, "StaticText", "2")]
        refused = dict(refused="protocol", detail="tree failed")
        for live in (False, True):
            for answer, expected in ((dict(tree=tree), ["Inbox ", "2"]),
                                     (dict(tree=[]), None), (refused, None)):
                with self.subTest(live=live, tree=answer):
                    scene = valid()
                    if live:
                        scene["page"] = dict(live="/", ready=scene["page"]["ready"], focus=None)
                    program, hand = Mock(), Mock()
                    program.call.return_value = dict(done=True, trouble=[])
                    hand.call.side_effect = lambda call, **kw: {
                        "look-viewport": {}, "address": dict(address="http://127.0.0.1:1/"),
                        "look-focus": dict(focus=None), "trouble": dict(trouble=[], more=0)}[call]
                    engine = capture.Capture(program, Path("unused"), 1, dict(page="http://127.0.0.1:1"), hand)
                    engine.write = Mock(return_value=record.file("unused", b""))
                    engine.shot = Mock(return_value=refused)
                    answers = {"await": dict(met=[]), "tree": answer,
                               "find": dict(found=[dict(node=1)], more=0),
                               "style": dict(style=dict(box=None, values={}, faces=[]))}
                    engine.read = Mock(side_effect=lambda call, *args, **kw: answers[call])
                    _, parts, _, _ = engine.side(scene, scene["widths"][0], "page", "unused")
                    self.assertEqual(parts[0]["words"], expected)
                    self.assertEqual([c.args[0] for c in engine.read.call_args_list], ["await", "tree", "find", "style"])


class Style(unittest.TestCase):
    def test_px_tolerance_on_both_sides_and_exact_tokens(self):
        cases = [("10px", "10.9px", True), ("10px", "9.1px", True), ("10px", "11px", True),
                 ("1.2px", "2.2px", True), ("100000000000000000000px", "100000000000000000002px", False),
                 ("10px", "11.1px", False), ("10px", "8.9px", False), ("10px", "10", False),
                 ("-10px", "-10.9px", True), ("normal", "normal", True), ("normal", "bold", False),
                 ("rgb(1, 2, 3)", "rgb(1, 2, 3)", True), ("rgb(1, 2, 3)", "rgb(1, 2, 4)", False),
                 ("0px 2px 3px rgb(0,0,0)", "0.9px 1.1px 4px rgb(0, 0, 0)", True),
                 ("0px 2px 3px rgb(0,0,0)", "0px 2px 4.1px rgb(0,0,0)", False),
                 ("1px 2px", "1px", False), ("none", "", False)]
        for a, b, expected in cases:
            with self.subTest(a=a, b=b):
                self.assertEqual(style.agree(a, b), expected)
                self.assertEqual(style.agree(b, a), expected)

    def test_faces_ignore_order_and_glyphs_but_keep_custom(self):
        a = [dict(family="A", custom=False, glyphs=1), dict(family="B", custom=True, glyphs=2)]
        b = [dict(family="B", custom=True, glyphs=200), dict(family="A", custom=False, glyphs=10)]
        self.assertTrue(style.faces_agree(a, b))
        b[0]["custom"] = False
        self.assertFalse(style.faces_agree(a, b))
        self.assertFalse(style.faces_agree(a, a[:1]))

    def test_table_marks_values_faces_and_boxes_and_escapes_pipes(self):
        def part(y, colour, custom):
            return dict(found=True, box=dict(x=0, y=y, width=10, height=20),
                        values={p: colour if p == "color" else "A|B" for p in style.PROPERTIES},
                        faces=[dict(family="Font", custom=custom, glyphs=1)])
        table = style.table([dict(name="main", mockup=part(1, "red", False), page=part(3, "blue", True)),
                             dict(name="missing", mockup=None, page=part(1, "red", False))])
        self.assertIn("| property | mockup | page | |\n", table)
        self.assertIn("| font-family | A\\|B | A\\|B |  |", table)
        self.assertIn("| color | red | blue | ≠ |", table)
        self.assertIn("| faces | Font | Font (web) | ≠ |", table)
        self.assertIn("| y | 1 | 3 | ≠ |", table)
        self.assertIn("| x | 0 | 0 |  |", table)
        self.assertIn("| color | — | red |  |", table)
        self.assertNotIn("≠", table.split("## missing")[1])
        a, b = part(1, "red", False), part(2, "red", False)
        self.assertIn("| y | 1 | 2 |  |", style.table([dict(name="bound", mockup=a, page=b)]))
        b["box"] = None
        self.assertIn("| y | 1 | — |  |", style.table([dict(name="hidden", mockup=a, page=b)]))
