"""An account-free unit cannot route beyond its loopback namespace."""
import unittest
from unittest.mock import patch
from runner import canonical, transport
from runner.host import CapabilityError
from . import test_runtime


class Network(unittest.TestCase):
    setUpClass = classmethod(test_runtime.Runtime.setUpClass.__func__)
    setUp = test_runtime.Runtime.setUp
    tearDown = test_runtime.Runtime.tearDown
    cli = test_runtime.Runtime.cli
    records = test_runtime.Runtime.records
    assert_empty = test_runtime.Runtime.assert_empty

    def test_no_account_connect_reports_enetunreach_within_one_second(self):
        (self.directory / 'network.py').write_text('''import errno, json, socket, sys, time
from pathlib import Path
path = Path(sys.argv[2])
if sys.argv[1] == 'probe':
    start = time.monotonic()
    with socket.socket() as connection:
        connection.settimeout(1)
        try:
            connection.connect(('192.0.2.1', 443))
        except OSError as error:
            number = error.errno
        else:
            number = 0
    elapsed = time.monotonic() - start
    print(errno.errorcode.get(number, str(number)), flush=True)
    path.write_text(json.dumps(dict(errno=number, elapsed=elapsed)))
else:
    value = json.loads(path.read_text())
    if value['errno'] != errno.ENETUNREACH or value['elapsed'] >= 1:
        print(json.dumps(dict(fail='network', summary=str(value))))
        sys.exit(1)
''')
        argv = [test_runtime.PYTHON, '-I', '-B', '-S', '{experiment}/network.py']
        exp = dict(format='lab-experiment/1', name='native', cost='smoke', dirs=['client'],
                   processes=[], actions=[dict(name='network', argv=argv + ['probe', '{dir.client}/errno'], env={})],
                   driver=None, faults=[], planted=[], collect=[],
                   checks=[dict(name='network', argv=argv + ['check', '{dir.client}/errno'], env={})])
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))
        status, _out, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 0, stderr)
        record = self.records()[2][0]
        self.assertEqual(record['network'], 'loopback')
        self.assertEqual(record['outcome'], 'pass')
        base = self.lab_root / 'keys/native' / record['key'] / 'runs' / record['run']
        action = next(c for c in record['commands'] if c['group'] == 'actions')
        self.assertEqual((base / action['output']['stdout']['path']).read_bytes(), b'ENETUNREACH\n')
        self.assert_empty(record)

    def test_worker_refuses_interfaces_beyond_loopback_before_provisioning(self):
        with patch('runner.transport.os.listdir', return_value=['eth0', 'lo']), self.assertRaisesRegex(CapabilityError, 'interfaces: eth0, lo'):
            transport.check_network(dict(network='loopback'))
        with patch('runner.transport.os.listdir', side_effect=AssertionError('host run inspected interfaces')):
            transport.check_network(dict(network='host'))
