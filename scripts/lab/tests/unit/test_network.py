"""No-account network admission and the unit command are pure decisions."""
import unittest
from runner import experiment, transport


class Network(unittest.TestCase):
    def test_only_rehearsals_and_runs_without_an_account_have_loopback(self):
        for driver in (None, {}):
            for held in ([], ['account']):
                exp = dict(driver=driver, held=held)
                for triage in (False, True):
                    for rehearsal in (False, True):
                        with self.subTest(driver=driver, held=held, triage=triage, rehearsal=rehearsal):
                            self.assertEqual(experiment.loopback_only(exp, rehearsal=rehearsal, triage=triage),
                                rehearsal or not triage and driver is None and not held)

    def test_loopback_units_request_a_private_network(self):
        cost = dict(command_seconds=7, stop_seconds=3)
        for network in ('loopback', 'host'):
            command = transport.unit_command('/systemd-run', '/python', 'unit', cost, 'code', '/runner', '/socket', 'token', network=network)
            self.assertEqual('--property=PrivateNetwork=yes' in command, network == 'loopback')

    def test_network_member_extends_run_records_without_changing_the_family(self):
        from runner.records import run_refusals
        from .test_runtime_store import run_record
        record = run_record(1, '1')
        self.assertEqual(run_refusals(record), [])
        for network in ('host', 'loopback'):
            self.assertEqual(run_refusals(dict(record, network=network)), [])
        for network in (None, '', 'private', []):
            self.assertIn('run.network', [r.member for r in run_refusals(dict(record, network=network))])
