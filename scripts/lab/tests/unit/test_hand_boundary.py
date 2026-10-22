"""Hold the hand's words to pure imports (lab.md, 10.4)."""
import ast
from pathlib import Path
import unittest


class Boundary(unittest.TestCase):
    def test_words_has_only_pure_imports_and_no_host_effects(self):
        tree = ast.parse((Path(__file__).resolve().parents[2] / 'hand/words.py').read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    self.assertIn(alias.name, {'json', 're'})
            elif isinstance(node, ast.ImportFrom):
                self.fail('words has no from imports')
            elif isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name):
                    self.assertNotIn(node.func.id, {'open', '__import__', 'eval', 'exec', 'compile', 'input'})
                elif isinstance(node.func, ast.Attribute):
                    self.assertNotIn(node.func.attr, {'open', 'read_bytes', 'read_text', 'write_bytes', 'write_text'})
