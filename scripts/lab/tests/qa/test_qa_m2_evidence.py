"""Adversarial process QA for lab.md 4.6, 5, 7.1, 8 and 9.2–9.3."""
import json
import shutil
import subprocess
import unittest

from .test_m2_tree import LAB
from .test_real_inputs import RealCase, digest

# The scripted host emits these exact observed counts; cached is part of input.
TOKEN_INPUT = 5
TOKEN_CACHED = 3
TOKEN_OUTPUT = 2
TOKEN_TOTAL = TOKEN_INPUT + TOKEN_OUTPUT
# Three judged seeds are required by lab.md 8, plus one explicit rerun for filing.
JUDGED_SEEDS = (7, 8, 9)


class EvidenceQA(RealCase):
    @classmethod
    def setUpClass(cls):
        for program in ('bwrap', 'systemd-run', 'systemctl', 'git', 'tail'):
            if shutil.which(program) is None:
                raise unittest.SkipTest('missing native program: ' + program)
        manager = subprocess.run(['systemctl', '--user', 'show', '--property=Version', '--value'],
                                 capture_output=True, timeout=5)
        if manager.returncode:
            raise unittest.SkipTest('user manager unavailable')

    def triager(self, script):
        host = self.host({'triage': script}, 'triager')
        self.settings['triager'] = dict(program=dict(path=str(host), sha256=digest(host),
                                                    version='0.162.1'),
                                        model='scripted', effort='low')
        self.settings['costs']['qa'].update(triage_seconds=2, triage_tokens=TOKEN_TOTAL)
        self.save_settings()
        return host

    def triage(self, record):
        return self.command([LAB, 'triage', record['run']], timeout=25)

    def histories(self):
        return {path.name: path.read_bytes()
                for path in (self.store / 'findings').glob('F-*.jsonl')}

    def failure(self):
        self.exp['actions'] = [self.command_spec('bad', '/usr/bin/false')]

    def fault(self, action=1):
        self.exp['processes'] = [self.service()]
        self.exp['faults'] = [dict(kill='service', signal='SIGKILL',
                                   after_actions=[action, action])]

    def verdict(self, run):
        return json.loads((run.parents[1] / 'verdict.json').read_bytes())

    def test_fired_fault_stays_failure_after_stopped_check_passes(self):
        # lab.md 4.6, 5.1, 7.1: even a runner fault is an early stop.
        self.driver(dict(items=[dict(type='commandExecution', argv=['/usr/bin/true'])],
                         wait_after_actions=True))
        self.fault()
        self.exp['planted'] = ['stop/service/signal:SIGKILL']
        self.exp['checks'] = [self.command_spec('at-rest', '/usr/bin/true', cluster='stopped')]
        record, run, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertEqual(self.signatures(record), {'stop/service/signal:SIGKILL'})
        self.assertTrue(record['schedule']['faults'][0]['fired'])
        check = next(c for c in record['commands'] if c['name'] == 'at-rest')
        self.assertEqual(check['exit']['status'], 0)
        timeline = [json.loads(line) for line in (run / 'events.jsonl').read_bytes().splitlines()]
        self.assertEqual(sum(e['event'] == 'fault' for e in timeline), 1)

    def test_unfired_fault_does_not_invent_planted_failure(self):
        # lab.md 4.6, 5.5: planted missing applies only to judged runs.
        self.driver({})
        self.fault()
        self.exp['planted'] = ['stop/service/signal:SIGKILL']
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 3, record)
        self.assertEqual(record['outcome'], 'incomplete')
        self.assertEqual(record['failures'], [])
        self.assertIn(dict(check='fault/0', reason='fault did not fire'), record['undecided'])
        self.assertEqual(self.histories(), {})

    def test_other_failure_keeps_unfired_fault_run_judged(self):
        # lab.md 5.5: fail precedes incomplete; 4.6: planted missing is a failure.
        self.driver({})
        self.fault()
        self.exp['planted'] = ['stop/service/signal:SIGKILL']
        self.exp['checks'] = [self.command_spec('panic', '/usr/bin/python3', '-I', '-B', '-S',
                                                '{experiment}/participant.py', 'panic')]
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertIn('planted-missing/stop/service/signal:SIGKILL', self.signatures(record))
        self.assertTrue(any(f['kind'] == 'panic' for f in record['failures']))
        self.assertIn(dict(check='fault/0', reason='fault did not fire'), record['undecided'])

    def test_requested_signal_stop_is_not_a_clean_stop(self):
        # lab.md 7.1: a requested stop must exit zero, even if TERM was expected.
        (self.experiment / 'ready.txt').write_text('ready retained\n')
        self.exp['processes'] = [dict(name='follower',
            argv=['/usr/bin/tail', '-f', '{proc.follower.dir}/ready'],
            files=[dict(path='ready', **{'from': 'ready.txt'})], env={}, memory_bytes=None,
            ready=[dict(stream='stdout', prefix='ready ', capture='message')])]
        self.exp['checks'] = [self.command_spec('after-stop', '/usr/bin/true', cluster='stopped')]
        record, _, result = self.execute(self.subject())
        self.assertEqual(result.returncode, 1, record)
        self.assertEqual(self.signatures(record), {'stopped/follower/signal:SIGTERM'})
        check = next(c for c in record['commands'] if c['name'] == 'after-stop')
        self.assertEqual(check['exit']['status'], 0)

    def test_planted_missing_survives_a_raised_spend_bound(self):
        # lab.md 4.6, 8: any non-bound failure prevents supersession.
        self.meter('3')
        self.exp['planted'] = ['check/absent/never']
        subject = self.subject()
        first, run, _ = self.execute(subject)
        frozen = (run / 'run.json').read_bytes()
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        second, _, result = self.execute(subject)
        self.assertEqual(result.returncode, 1, second)
        self.assertEqual(self.signatures(first),
                         {'bound/subject_spend', 'planted-missing/check/absent/never'})
        self.assertEqual(self.signatures(second), {'planted-missing/check/absent/never'})
        self.assertEqual(first['key'], second['key'])
        seed = next(s for s in self.verdict(run)['seeds'] if s['seed'] == '7')
        self.assertIn(first['run'], seed['judged'])
        self.assertEqual((run / 'run.json').read_bytes(), frozen)
        findings = [json.loads(line) for data in self.histories().values() for line in data.splitlines()]
        planted = [e for e in findings if e['event'] == 'candidate'
                   and e['signature'] == 'planted-missing/check/absent/never']
        self.assertEqual(len(planted), 1)
        self.assertTrue(any(e['event'] == 'occurred' for e in findings))

    def two_bounds(self):
        self.driver(dict(usage=[dict(inputTokens=TOKEN_INPUT, cachedInputTokens=TOKEN_CACHED,
                                     outputTokens=TOKEN_OUTPUT)]))
        self.meter('3')
        self.settings['costs']['qa']['driver_input_tokens'] = TOKEN_INPUT - 1
        self.save_settings()

    def test_raising_only_one_failed_bound_keeps_run_judged(self):
        # lab.md 8: every failed bound must be strictly higher.
        self.two_bounds()
        subject = self.subject()
        first, run, _ = self.execute(subject)
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        self.execute(subject)
        verdict = self.verdict(run)
        self.assertEqual(self.signatures(first), {'bound/subject_spend', 'bound/driver_input_tokens'})
        seed = next(s for s in verdict['seeds'] if s['seed'] == '7')
        self.assertIn(first['run'], seed['judged'])

    def test_later_evidence_with_all_higher_bounds_supersedes(self):
        # lab.md 8: evidence is additive and superseded runs are not judged.
        self.two_bounds()
        subject = self.subject()
        first, run, _ = self.execute(subject)
        evidence = run.parents[1] / 'evidence.jsonl'
        prefix = evidence.read_bytes()
        self.settings['costs']['qa'].update(subject_spend=4, driver_input_tokens=TOKEN_INPUT)
        self.save_settings()
        second, _, result = self.execute(subject)
        self.assertEqual(result.returncode, 0, second)
        self.assertEqual(second['key'], first['key'])
        self.assertEqual(second['attempt'], 2)
        published = self.verdict(run)
        judged = self.command([LAB, 'judge', self.experiment, '--subject', subject], timeout=25)
        refreshed = self.verdict(run)
        seed = next(s for s in published['seeds'] if s['seed'] == '7')
        self.assertEqual(self.signatures(first), {'bound/subject_spend', 'bound/driver_input_tokens'})
        self.assertNotIn(first['run'], seed['judged'],
                         dict(published=published, refreshed=refreshed,
                              judge_status=judged.returncode, judge_stderr=judged.stderr.decode()))
        self.assertIn(first['run'], json.dumps(seed['not_judged']))
        self.assertTrue(evidence.read_bytes().startswith(prefix))

    def test_settings_alone_cannot_supersede_filing_evidence(self):
        # lab.md 9.3 and README: snapshots need a later judged run with higher bounds.
        self.meter('3')
        subject = self.subject()
        for seed in (*JUDGED_SEEDS, JUDGED_SEEDS[0]):
            self.execute(subject, seed)
        identifier = next(iter(self.histories())).removesuffix('.jsonl')
        self.checked([LAB, 'finding', identifier, 'class', 'experiment', '--by', 'qa-owner'])
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        result = self.command([LAB, 'finding', identifier, 'file', '--by', 'qa-owner'])
        self.assertEqual(result.returncode, 0, result.stderr)
        history = [json.loads(line) for line in self.histories()[identifier + '.jsonl'].splitlines()]
        self.assertEqual(history[-1]['event'], 'filed')

    def test_superseding_evidence_prevents_bound_finding_filing(self):
        # lab.md 8, 9.3: snapshots follow later judged evidence, not old failures alone.
        self.meter('3')
        subject = self.subject()
        for seed in (*JUDGED_SEEDS, JUDGED_SEEDS[0]):
            self.execute(subject, seed)
        identifier = next(iter(self.histories())).removesuffix('.jsonl')
        self.checked([LAB, 'finding', identifier, 'class', 'experiment', '--by', 'qa-owner'])
        self.settings['costs']['qa']['subject_spend'] = 4
        self.save_settings()
        for seed in JUDGED_SEEDS:
            record, run, result = self.execute(subject, seed)
            self.assertEqual(result.returncode, 0, record)
        before = self.histories()
        result = self.command([LAB, 'finding', identifier, 'file', '--by', 'qa-owner'])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(self.histories(), before)

    def test_pass_has_nothing_to_triage(self):
        # lab.md 9.2 and public admission contract: only judged failing runs.
        host = self.triager({})
        record, run, _ = self.execute(self.subject())
        result = self.triage(record)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertFalse((run / 'triage').exists())
        self.assertFalse(host.with_suffix('.turn').exists())

    def test_occurrence_has_nothing_new_to_triage(self):
        # lab.md 9.2: mechanically matched occurrences need no triager.
        host = self.triager({})
        self.failure()
        subject = self.subject()
        self.execute(subject)
        record, run, _ = self.execute(subject)
        before = self.histories()
        result = self.triage(record)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(self.histories(), before)
        self.assertFalse((run / 'triage').exists())
        self.assertFalse(host.with_suffix('.turn').exists())

    def test_malformed_batch_appends_no_partial_proposal(self):
        # lab.md 9.2 and README: the entire answer validates before any append.
        self.failure()
        self.exp['checks'] = [self.command_spec('panic', '/usr/bin/python3', '-I', '-B', '-S',
                                                '{experiment}/participant.py', 'panic')]
        record, run, _ = self.execute(self.subject())
        answers = [dict(id=f['id'], match=None, signature=None, title='Proposed',
                        reason='Retained evidence', **{'class': 'experiment'})
                   for f in record['failures']]
        answers[-1]['id'] = 'not-an-input-failure'
        self.triager(dict(output=dict(failures=answers)))
        before = self.histories()
        frozen = (run / 'run.json').read_bytes()
        result = self.triage(record)
        self.assertEqual(result.returncode, 4, result.stderr)
        self.assertEqual(self.histories(), before)
        self.assertEqual((run / 'run.json').read_bytes(), frozen)
        self.assertFalse((run / 'live.json').exists())

    def token_triage(self, bound):
        self.triager(dict(usage=[dict(inputTokens=TOKEN_INPUT, cachedInputTokens=TOKEN_CACHED,
                                      outputTokens=TOKEN_OUTPUT)]))
        self.settings['costs']['qa']['triage_tokens'] = bound
        self.save_settings()
        self.failure()
        record, run, _ = self.execute(self.subject())
        before = self.histories()
        result = self.triage(record)
        path = run / 'triage/1/triage.json'
        return result, json.loads(path.read_bytes()), before

    def test_triage_at_token_bound_does_not_count_cached_twice(self):
        # lab.md 6.5: cached included in input; total here is 5 + 2, not 5 + 3 + 2.
        result, record, before = self.token_triage(TOKEN_TOTAL)
        self.assertEqual(result.returncode, 0, (result.stderr, record))
        self.assertEqual(record['outcome'], 'proposed')
        self.assertNotEqual(self.histories(), before)
        events = [json.loads(line) for data in self.histories().values() for line in data.splitlines()]
        proposed = [e for e in events if e['event'] == 'proposed']
        self.assertEqual(len(proposed), 1)
        self.assertEqual(proposed[0]['by'], 'triager')
        self.assertFalse(any(e['event'] == 'classed' for e in events))

    def test_triage_one_token_past_bound_appends_nothing(self):
        # lab.md 6.5, 9.2: observed overshoot is retained, with no finding events.
        result, record, before = self.token_triage(TOKEN_TOTAL - 1)
        self.assertEqual(result.returncode, 1, (result.stderr, record))
        self.assertEqual(record['outcome'], 'bound')
        self.assertEqual(self.histories(), before)

    def test_retry_after_token_bound_keeps_prior_evidence_immutable(self):
        # lab.md 6.5, 9.2: tuning permits an additive, separately bounded invocation.
        result, _, _ = self.token_triage(TOKEN_TOTAL - 1)
        self.assertEqual(result.returncode, 1, result.stderr)
        path = next(self.store.glob('keys/*/*/runs/*/run.json'))
        run = path.parent
        record = json.loads(path.read_bytes())
        frozen = (run / 'run.json').read_bytes()
        first = (run / 'triage/1/triage.json').read_bytes()
        self.settings['costs']['qa']['triage_tokens'] = TOKEN_TOTAL
        self.save_settings()
        self.assertEqual(self.triage(record).returncode, 0)
        self.assertTrue((run / 'triage/2/triage.json').is_file())
        self.assertEqual((run / 'triage/1/triage.json').read_bytes(), first)
        self.assertEqual((run / 'run.json').read_bytes(), frozen)
