#!/usr/bin/env python3
"""Forward recovery instructions while heavy waits; retain its protocol in memory."""
import os
from pathlib import Path
import subprocess
import sys

from recovery_message import RECOVERY_DIAGNOSTIC

os.umask(0o077)
diagnostic = RECOVERY_DIAGNOSTIC.encode() + b'\n'
capture = bytearray()
pending = bytearray()
child = subprocess.Popen(sys.argv[2:], stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
while True:
    chunk = os.read(child.stdout.fileno(), 65536)
    if not chunk:
        break
    capture.extend(chunk)
    pending.extend(chunk)
    while b'\n' in pending:
        line, _, rest = pending.partition(b'\n')
        pending = bytearray(rest)
        if line + b'\n' == diagnostic:
            # No writable file is needed before the caller sees this warning.
            sys.stderr.buffer.write(diagnostic)
            sys.stderr.buffer.flush()
code = child.wait()
try:
    destination = Path(sys.argv[1])
    faults = os.environ.get('HEAVY_FAULT_DIR')
    if faults and (Path(faults) / 'enospc-land-capture').exists():
        if os.environ.get('HEAVY_RECOVERY_FIXTURE') != '1' or not Path(faults).resolve().is_relative_to(
                Path(os.environ['DEV_TMP']).resolve()):
            raise ValueError('capture faults require an isolated recovery fixture')
        # /dev/full returns ENOSPC from the kernel without filling any disk.
        destination = Path('/dev/full')
    with destination.open('wb') as output:
        output.write(capture)
except (OSError, ValueError, KeyError):
    print('land: heavy capture could not be written; completion evidence is unavailable; rerun land',
          file=sys.stderr, flush=True)
    sys.exit(74)
sys.exit(code if code >= 0 else 128 - code)
