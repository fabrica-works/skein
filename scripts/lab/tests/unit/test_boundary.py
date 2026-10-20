import ast
from pathlib import Path
import unittest


class Boundary(unittest.TestCase):
    def test_pure_modules_have_only_approved_imports_and_no_file_effects(self):
        pure = {'canonical', 'records', 'experiment', 'draws', 'judge', 'life', 'checks'}
        allowed = {'json', 'hashlib', 're', 'dataclasses', 'enum', 'typing'}
        root = Path(__file__).resolve().parents[2] / 'runner'
        for module in sorted(pure):
            tree = ast.parse((root / (module + '.py')).read_text())
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
                    else:
                        self.assertIn(node.module, allowed, module)
                elif isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                    self.assertNotIn(node.func.id, {'open', '__import__', 'eval', 'exec', 'compile', 'input'}, module)
