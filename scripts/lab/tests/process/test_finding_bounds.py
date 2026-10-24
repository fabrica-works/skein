"""Finding rechecks derive superseded bounds from contained runs' retained facts."""
from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
import io
import json
import unittest
from runner import canonical, cli, evidence, finding_store, findings, store
from . import test_driver, test_runtime as native


class FindingBounds(unittest.TestCase):
    setUpClass = classmethod(native.Runtime.setUpClass.__func__)
    tearDown = native.Runtime.tearDown
    cli = native.Runtime.cli
    assert_empty = native.Runtime.assert_empty

    def setUp(self):
        test_driver.Driver.setUp(self)
        self.settings['costs']['temper-m1'] = self.settings['costs'].pop('smoke')
        self.settings['costs']['temper-m1'].update(driver_seconds=10, driver_input_tokens=283000)
        self.exp['cost'] = 'temper-m1'
        self.flag = self.root / 'fail-check'
        self.flag.touch()
        (self.directory / 'check.py').write_text(
            'from pathlib import Path\nimport sys\n'
            'if Path(sys.argv[1]).exists():\n'
            '    print(\'{"fail":"new","summary":"new failure"}\')\n'
            '    sys.exit(1)\n')
        self.exp['checks'] = [dict(name='new', argv=[native.PYTHON, '-I', '-B', '-S',
            '{experiment}/check.py', str(self.flag)], env={})]

    def inputs(self, exp, script):
        self.script.write_text(json.dumps(script))
        settings = self.lab_root / 'settings.json'
        settings.chmod(0o600)
        settings.write_bytes(canonical.record_bytes(self.settings))
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(exp))

    def owner(self, identity, *args, status=0):
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            result = cli.main(['finding', identity, *args, '--by', 'owner'], lab_root=self.lab_root)
        self.assertEqual(result, status, stderr.getvalue())
        return stderr.getvalue()

    def story(self, extra_failure):
        original = deepcopy(self.exp)
        original['driver'] = None
        self.inputs(original, {})
        for seed in ('1', '2', '3', '1'):
            status, _, stderr = self.cli('run', seed=seed)
            self.assertEqual(status, 1, stderr)
        with store.open_lab(self.lab_root) as lab:
            index = canonical.loads(lab.read('findings/index.json'))
            identity = next(f['id'] for f in index['findings'] if f['signature'] == 'check/new/new')
        self.owner(identity, 'class', 'experiment')
        self.owner(identity, 'file')
        self.owner(identity, 'take')

        if not extra_failure:
            self.flag.unlink()
        script = dict(usage=[dict(inputTokens=300000, outputTokens=1)])
        self.inputs(self.exp, script)
        status, failed_run, stderr = self.cli('run', seed='1')
        self.assertEqual(status, 1, stderr)
        with store.open_lab(self.lab_root) as lab:
            records, paths = finding_store.sources(lab)
            corrected = next(key for key, runs in records.items() if runs[0]['run'] == failed_run)
            key_path = paths[corrected]
            failed = records[corrected][0]
            self.assertEqual(failed['bounds']['driver_input_tokens'], 283000)
            self.assertEqual({f['signature'] for f in failed['failures']},
                             {'bound/driver_input_tokens', 'check/new/new'} if extra_failure else
                             {'bound/driver_input_tokens'})
            retained = lab.read(f"{key_path}/runs/{failed_run}/run.json")
            corrected_exp = canonical.loads(lab.read(key_path + '/key.json'))['experiment']
        self.flag.unlink(missing_ok=True)
        self.owner(identity, 'fix', '--experiment', corrected_exp)
        self.settings['costs']['temper-m1']['driver_input_tokens'] = 450000
        self.inputs(self.exp, script)
        for seed in ('1', '2', '3'):
            status, _, stderr = self.cli('run', seed=seed)
            self.assertEqual(status, 0, stderr)
        status, output, stderr = self.cli('judge')
        self.assertEqual((status, output), (1, 'flaky') if extra_failure else (0, 'pass'), stderr)
        with store.open_lab(self.lab_root) as lab:
            runs = finding_store.sources(lab)[0][corrected]
            self.assertEqual([r['seed'] for r in runs], ['1', '1', '2', '3'])
            self.assertEqual([r['bounds']['driver_input_tokens'] for r in runs], [283000, 450000, 450000, 450000])
            self.assertEqual(evidence.read(lab, key_path)[0]['run'], failed_run)
            verdict = canonical.loads(lab.read(key_path + '/verdict.json'))
            self.assertEqual(findings.snapshot(corrected, runs), verdict)
            self.assertEqual(verdict['seeds'][0]['not_judged'], [] if extra_failure else [failed_run])
            self.assertEqual(lab.read(f"{key_path}/runs/{failed_run}/run.json"), retained)
            for run in runs:
                self.assert_empty(run)
            before = lab.read('findings/' + identity + '.jsonl')
        # Finding verification depends only on retained evidence, including its bounds.
        (self.lab_root / 'settings.json').unlink()
        derived_path = self.lab_root / key_path / 'verdict.json'
        derived_path.chmod(0o600)
        derived_path.write_bytes(b'corrupt derived bytes\n')
        error = self.owner(identity, 'close', '--recheck', corrected, status=2 if extra_failure else 0)
        with store.open_lab(self.lab_root) as lab:
            events = finding_store.histories(lab)[identity]
            if extra_failure:
                self.assertIn('finding.recheck', error)
                self.assertEqual(lab.read('findings/' + identity + '.jsonl'), before)
                self.assertEqual(events[-1]['event'], 'fixed')
            else:
                self.assertEqual([e['event'] for e in events[-2:]], ['rechecked', 'closed'])
                self.assertEqual(events[-2]['verdict'], verdict)
                self.assertEqual(finding_store.sync(lab, events[-1]['t'])['findings'][0]['state'], 'closed')

    def test_bound_only_failure_then_three_passes_closes(self):
        self.story(False)

    def test_bound_and_other_failure_then_three_passes_refuses(self):
        self.story(True)
