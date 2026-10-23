"""Isolated Python entry point for the host's lab MCP server."""
import os
from pathlib import Path
import sys

os.umask(0o077)
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from runner.cli import main

if __name__ == '__main__':
    root = sys.argv[1]
    raise SystemExit(main(sys.argv[2:], lab_root=root))
