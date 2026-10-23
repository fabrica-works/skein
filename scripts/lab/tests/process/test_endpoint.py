"""The endpoint retains wire evidence for Responses and for refused HTTP routes."""
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request
from runner import canonical
from .test_runtime import PACKAGE, PYTHON


class Endpoint(unittest.TestCase):
    def test_responses_and_other_routes_are_private_and_term_exits_zero(self):
        started = time.monotonic()
        with tempfile.TemporaryDirectory(prefix='endpoint-', dir=PACKAGE.parents[1]) as temporary:
            root = Path(temporary)
            path = root / 'script.json'
            path.write_bytes(canonical.record_bytes(dict(format='lab-script/1', turns=[dict(output=[dict(text='done')])])))
            with (root / 'stderr').open('wb') as errors, subprocess.Popen([PYTHON, '-I', '-B', '-S',
                    str(PACKAGE / 'runner/launch.py'), str(root), 'endpoint', 'RUN', str(path), str(root / 'evidence')],
                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=errors) as endpoint:
                try:
                    self.assertTrue(select.select([endpoint.stdout], [], [], 2)[0], 'endpoint readiness')
                    address = endpoint.stdout.readline().decode().strip().removeprefix('listening ')
                    request = urllib.request.Request('http://' + address + '/v1/responses', data=b'{"input":[]}',
                                                     headers={'Content-Type': 'application/json'})
                    with urllib.request.urlopen(request, timeout=2) as answer:
                        data = answer.read()
                        self.assertIn(b'event: response.completed', data)
                    self.assertEqual((root / 'evidence/responses/0.sse').read_bytes(), data)
                    for method, route in [('POST', '/other'), ('GET', '/v1/responses'), ('HEAD', '/v1/responses'), ('PATCH', '/v1/responses')]:
                        with self.subTest(method=method), self.assertRaises(urllib.error.HTTPError) as refused:
                            urllib.request.urlopen(urllib.request.Request('http://' + address + route, method=method), timeout=2)
                        self.assertEqual(refused.exception.code, 404)
                        refused.exception.close()
                    self.assertEqual(len(list((root / 'evidence/responses').glob('*.sse'))), 5)
                    self.assertEqual(json.loads((root / 'evidence/requests/1.http.json').read_bytes()),
                                     dict(method='POST', path='/other', status=404))
                    for directory in ('requests', 'responses'):
                        for path in (root / 'evidence' / directory).iterdir():
                            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                finally:
                    endpoint.terminate()
                    endpoint.wait(timeout=2)
                self.assertEqual(endpoint.returncode, 0)
        print(f'endpoint HTTP evidence: {time.monotonic() - started:.3f} s', flush=True)
