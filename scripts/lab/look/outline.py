"""Structure from document-order accessibility entries (lab-browser.md, 5.3)."""
import difflib
import json

LANDMARKS = {"banner", "navigation", "main", "complementary", "contentinfo", "region", "form", "search"}
CONTROLS = {"button", "link", "textbox", "searchbox", "checkbox", "radio", "combobox", "tab", "menuitem", "switch"}


def outline(tree_entries, names):
    names = set(names)
    landmarks = []
    lines = []
    for index, entry in enumerate(tree_entries):
        depth, role = entry["depth"], entry["role"]
        while landmarks and landmarks[-1] >= depth:
            landmarks.pop()
        label = role
        if role == "heading":
            label += " " + str(entry["level"])
        elif role == "list":
            count = 0
            for child in tree_entries[index + 1:]:
                if child["depth"] <= depth:
                    break
                if child["depth"] == depth + 1 and child["role"] == "listitem":
                    count += 1
            label += f" ({count})"
        elif role not in LANDMARKS and role not in CONTROLS:
            continue
        if entry["name"] in names:
            label += " " + json.dumps(entry["name"], ensure_ascii=False)
        lines.append("  " * len(landmarks) + label + "\n")
        if role in LANDMARKS:
            landmarks.append(depth)
    return "".join(lines)


def diff(mockup_text, page_text):
    return "".join(difflib.unified_diff(mockup_text.splitlines(True), page_text.splitlines(True),
                                        "mockup/outline.txt", "page/outline.txt", n=3))
