#!/usr/bin/env python3
"""Verify an installed, read-only recovery kit before setup changes anything."""
import hashlib
import json
from pathlib import Path
import re
import sys


def verify():
    kit = Path(__file__).resolve().parent
    release = json.loads((kit / 'release.json').read_text())
    commit = release['commit']
    if release['schema'] != 'skein-recovery-release-v1' or not re.fullmatch('[0-9a-f]{40,64}', commit):
        raise ValueError('invalid release identity')
    expected = Path.home() / '.local/share/skein-kits' / ('recovery-v1-' + commit)
    if kit != expected or expected.is_symlink():
        raise ValueError('release is outside its stable installation path')
    for directory in [kit, *kit.parents]:
        if (directory / '.git').exists():
            raise ValueError('release is inside a mutable checkout')
    files = release['files']
    required = {'heavy', 'land', 'setup', 'heavy-recovery.py', 'gate-policy.sh',
                'kit.sh', 'after-landing', 'kache-setup', 'stable-release.py',
                'land-capture.py', 'recovery_message.py'}
    if not required.issubset(files):
        raise ValueError('release is missing runtime assets')
    for name, digest in files.items():
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('invalid release asset path')
        path = kit / relative
        if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o222:
            raise ValueError('release asset is mutable')
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError('release asset differs from its manifest')
    if kit.stat().st_mode & 0o222:
        raise ValueError('release directory is mutable')
    for path in kit.rglob('*'):
        if path.is_symlink() or (path.is_dir() and path.stat().st_mode & 0o222):
            raise ValueError('release contains mutable directories or symlinks')


try:
    verify()
except (OSError, ValueError, KeyError, TypeError):
    print('setup: refusing mutable/unverified kit; commit and run recovery-v1/install-release, '
          'then use the copied stable setup after the director-owned quiet drain '
          '(build-infrastructure.md, 7)', file=sys.stderr)
    sys.exit(2)
