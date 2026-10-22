"""Keep the look's decisions free of host effects (lab.md, 10.4)."""
import ast
from pathlib import Path
import unittest


class Boundary(unittest.TestCase):
    def test_pure_modules_have_only_approved_imports_and_no_file_effects(self):
        pure = {"canon", "scene", "outline", "style", "png", "picture", "record"}
        allowed = {"json", "hashlib", "re", "dataclasses", "enum", "typing", "difflib", "math", "struct", "zlib"}
        root = Path(__file__).resolve().parents[2] / "look"
        for module in sorted(pure):
            tree = ast.parse((root / (module + ".py")).read_text())
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    for alias in node.names:
                        self.assertIn(alias.name, allowed, module)
                elif isinstance(node, ast.ImportFrom):
                    if node.level:
                        if node.module:
                            self.assertIn(node.module, pure, module)
                        else:
                            for alias in node.names:
                                self.assertIn(alias.name, pure, module)
                    elif node.module == "runner":
                        for alias in node.names:
                            self.assertIn(alias.name, {"canonical", "records"}, module)
                    else:
                        self.assertIn(node.module, allowed | {"runner.canonical", "runner.records"}, module)
                elif isinstance(node, ast.Call):
                    if isinstance(node.func, ast.Name):
                        self.assertNotIn(node.func.id, {"open", "__import__", "eval", "exec", "compile", "input"}, module)
                    elif isinstance(node.func, ast.Attribute):
                        self.assertNotIn(node.func.attr, {"open", "read_bytes", "read_text", "write_bytes", "write_text"}, module)
