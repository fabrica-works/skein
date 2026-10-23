"""Supplied meter times, account windows and admission settings; no host effects."""
from copy import deepcopy
import unittest
from runner import canonical, codex, experiment, judge, life, meter, records
from .fixtures import fixture
from .test_life_checks import event
from . import test_browser_settings as browser_tests


def snapshot(five=12, week=3, swapped=False):
    rows = [dict(windowDurationMins=300, usedPercent=five, resetsAt=100),
            dict(windowDurationMins=10080, usedPercent=week, resetsAt=None)]
    if swapped:
        rows.reverse()
    return dict(primary=rows[0], secondary=rows[1])


def drive(driver=True):
    exp, _, settings, _, _ = fixture()
    exp.update(processes=[], actions=[], collect=[dict(name='obs', argv=['/collect'], env={}, scan=[])], checks=[])
    exp['driver'] = dict(charter='charter', tools=[]) if driver else None
    exp['meter'] = dict(name='spend', argv=['/meter', '{run.seed}'], env={})
    cost = dict(settings['costs']['smoke'], meter_seconds=1, subject_spend=10, command_seconds=5)
    state = life.initial(exp, '1', cost, {'run.seed': '1', 'collected.obs': '/collected'}, {})
    state, _ = event(state, 'validated')
    return event(state, 'provisioned', 100)


def done(state, ms, *, spend=None, status=0, signal=None, timed_out=False):
    result = dict(status=status) if signal is None else dict(signal=signal)
    return event(state, 'meter_done', ms, n=state.data['meter']['n'],
                 record=dict(exit=result, spend=spend, timed_out=timed_out))


class Settings(unittest.TestCase):
    def test_meter_costs_optional_positive_safe_and_superseded(self):
        cost = fixture()[2]['costs']['smoke']
        self.assertEqual(records.cost_refusals(cost, 'cost'), [])
        for field in records.METER_COST_FIELDS:
            for value in (None, 0, -1, True, '1', 1.5, records.SAFE + 1):
                with self.subTest(field=field, value=value):
                    self.assertTrue(records.cost_refusals(dict(cost, **{field: value}), 'cost'))
        self.assertEqual(records.cost_refusals(dict(cost, subject_spend=1, meter_seconds=records.SAFE), 'cost'), [])
        self.assertTrue(judge.superseded(['bound/subject_spend'], {'subject_spend': 10}, {'subject_spend': 11}))
        self.assertFalse(judge.superseded(['bound/subject_spend'], {'subject_spend': 10}, {'subject_spend': 10}))

    def test_meter_needs_both_bounds_and_closed_command_shape(self):
        exp, subject, settings, facts, _ = fixture()
        before = canonical.dumps(exp)
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(canonical.dumps(exp), before)
        exp['meter'] = None
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        exp['meter'] = dict(name='spend', argv=['/usr/bin/helper', '{action.seed.capture}'], env={})
        problems = experiment.validate(exp, subject, settings, facts)
        self.assertEqual(sum(r.member == 'meter' for r in problems), 2)
        exp['meter']['argv'].pop()
        settings['costs']['smoke'].update(subject_spend=100, meter_seconds=1)
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        for field, value in (('name', 'BAD'), ('argv', []), ('env', []), ('stdin', 'no'), ('cluster', 'up')):
            changed = deepcopy(exp)
            changed['meter'][field] = value
            with self.subTest(field=field):
                self.assertTrue(experiment.validate(changed, subject, settings, facts))
        for value in ('meter', [], 1):
            exp['meter'] = value
            self.assertTrue(experiment.validate(exp, subject, settings, facts))

    def test_meter_templates_have_live_checks_scope(self):
        exp, subject, settings, facts, _ = fixture()
        settings['costs']['smoke'].update(subject_spend=10, meter_seconds=1)
        exp['actions'][0]['await'] = dict(process='app', stream='stdout', prefix='seed ', capture='token')
        exp['meter'] = dict(name='spend', argv=['/usr/bin/helper', '{proc.app.ready.address}',
                                               '{action.seed.token}'], env={})
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        # A collection is known in checks alone (lab.md 4.2); the meter reads before it.
        for cluster in ('up', 'stopped'):
            with self.subTest(cluster=cluster):
                exp['collect'][0]['cluster'] = cluster
                exp['meter']['argv'][3:] = ['{collected.obs}']
                self.assertIn('meter.0.argv.3', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_ports_names_values_and_unknown_port(self):
        for value in (None, [], {'BAD': 3}, {'bad_name': 3}, {'web': True}, {'web': 0}, {'web': 65536}, {'web': '80'}):
            with self.subTest(value=value):
                self.assertTrue(experiment.ports_settings_refusals(value))
        self.assertEqual(experiment.ports_settings_refusals({'web': 1, 'web-2': 65535}), [])
        exp, subject, settings, facts, _ = fixture()
        settings['ports'] = {'web': 30300}
        for group in ('processes', 'actions', 'collect', 'checks'):
            exp[group][0]['argv'].append('{port.web}')
            exp[group][0]['env']['PORT'] = '{port.web}'
        facts['experiment_files']['app.conf'] += ' port={port.web}'
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(experiment.render('{port.web}', experiment.port_values(settings['ports'])), '30300')
        settings['ports'] = {}
        self.assertTrue(any('port web not named' in r.why for r in experiment.validate(exp, subject, settings, facts)))
        settings['ports'] = None
        self.assertIn('settings.ports', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_brake_closed_shape_runner_only_and_absolute(self):
        valid = dict(argv=['{runner}/brake/codex', 'arg'], env={'ROOT': '{runner}', 'EMPTY': ''})
        self.assertEqual(experiment.brake_settings_refusals(valid, '/runner'), [])
        for value in (None, [], {}, dict(argv=[], env={}), dict(argv=['relative'], env={}),
                      dict(argv=['/bin/true', ''], env={}), dict(argv=['/bin/true', 1], env={}),
                      dict(argv=['{experiment}/check'], env={}), dict(argv=['/bin/true'], env={'X': '{port.web}'}),
                      dict(argv=['/bin/true'], env={'BAD=NAME': ''}), dict(argv=['/bin/true'], env={'X': 1}),
                      dict(argv=['/bin/true'], env={}, extra=True)):
            with self.subTest(value=value):
                self.assertTrue(experiment.brake_settings_refusals(value, '/runner'))
        exp, subject, settings, facts, _ = fixture()
        settings['brake'] = valid
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        settings['brake'] = None
        self.assertIn('settings.brake', [r.member for r in experiment.validate(exp, subject, settings, facts)])


class PortKey(unittest.TestCase):
    setUp = browser_tests.BrowserHost.setUp
    tearDown = browser_tests.BrowserHost.tearDown
    executable = browser_tests.BrowserHost.executable
    write_inputs = browser_tests.BrowserHost.write_inputs
    validate = browser_tests.BrowserHost.validate
    key = browser_tests.BrowserHost.key

    def test_port_values_and_meter_bounds_are_not_in_key(self):
        self.exp['collect'][0]['argv'] = ['{runner}/look/capture', '{port.web}']
        identities = []
        for value in (30300, 30301):
            self.settings['ports'] = {'web': value}
            self.settings['costs'][self.exp['cost']].update(subject_spend=value, meter_seconds=value)
            self.write_inputs()
            status, identity, errors = self.validate()
            self.assertEqual(status, 0, errors)
            self.assertNotIn('ports', self.key(identity)['lab'])
            identities.append(identity)
        self.assertEqual(*identities)


class Meter(unittest.TestCase):
    def test_reading_grammar_and_final_line(self):
        for text, value in ((b'0\n', 0), (b'notice\n12\n', 12), (b'42', 42), (b'', None),
                            (b'01\n', None), (b'+1\n', None), (b'-1\n', None), (b'1 \n', None),
                            (b'1\n\n', None), (b'12\nnotice\n', None), (b'\xff\n', None)):
            with self.subTest(text=text):
                self.assertEqual(meter.reading(text), value)

    def test_periodic_first_due_no_overlap_and_delayed_reading(self):
        state, effects = drive()
        self.assertFalse(any(e.get('group') == 'meter' for e in effects))
        state, _ = event(state, 'driver_spawned', 100)
        for ms in (101, 1099):
            state, effects = event(state, 'meter_due', ms)
            self.assertEqual(effects, [])
        state, effects = event(state, 'meter_due', 1100)
        self.assertEqual(effects[-1]['group'], 'meter')
        self.assertEqual(effects[-1]['argv'], ['/meter', '1'])
        state, _ = event(state, 'spawned', 1100, group='meter', name='spend')
        self.assertEqual(state.data['deadline'], 900100)
        state, effects = event(state, 'meter_due', 2100)
        self.assertEqual(effects, [])
        state, effects = done(state, 2200, spend=3)
        self.assertEqual(effects[-1]['n'], 2)
        self.assertEqual(state.data['meter']['spend'], 3)

    def test_last_waits_for_pending_read_then_precedes_collect(self):
        state, _ = drive()
        state, _ = event(state, 'meter_due', 1100)
        state, _ = event(state, 'spawned', 1100, group='meter', name='spend')
        state.data['driver']['inventory'] = {}
        state, effects = event(state, 'driver_closed', 1200, result={'status': 0})
        self.assertEqual(effects, [])
        state, effects = done(state, 1300, spend=2)
        self.assertEqual(effects[-1]['group'], 'meter')
        self.assertTrue(state.data['meter']['final'])
        state, _ = event(state, 'spawned', 1300, group='meter', name='spend')
        state, effects = done(state, 1400, spend=4)
        self.assertEqual(effects[-1]['group'], 'collect')
        self.assertEqual(state.data['meter']['spend'], 4)
        state, effects = event(state, 'meter_due', 999999)
        self.assertEqual(effects, [])

    def test_null_driver_has_exactly_one_read_and_no_line_fails(self):
        state, effects = drive(False)
        self.assertEqual(effects[-1]['group'], 'meter')
        self.assertTrue(state.data['meter']['final'])
        self.assertIsNone(state.data['meter']['spend'])
        state, _ = event(state, 'spawned', 100, group='meter', name='spend')
        state, effects = done(state, 101)
        self.assertEqual(state.data['phase'], 'collect')
        self.assertEqual(state.data['meter']['n'], 1)
        self.assertIsNone(state.data['meter']['spend'])
        self.assertIn(dict(op='meter_failure', signature='meter/reading'), effects)

    def test_bound_is_strict_stops_driver_and_keeps_last_good(self):
        state, _ = drive()
        state, _ = event(state, 'meter_due', 1100)
        state, effects = done(state, 1101, spend=10)
        self.assertEqual(effects, [])
        state, _ = event(state, 'meter_due', 2100)
        state, effects = done(state, 2101, spend=11)
        self.assertEqual([e['op'] for e in effects], ['bound', 'stop_driver'])
        self.assertEqual(effects[0]['name'], 'subject_spend')
        self.assertEqual(state.data['meter']['spend'], 11)
        state, effects = event(state, 'meter_due', 999999)
        self.assertEqual(effects, [])

    def test_second_consecutive_failure_uses_its_kind(self):
        for fields, signature in ((dict(status=3), 'meter/exit:3'),
                                  (dict(signal='SIGTERM'), 'meter/signal:SIGTERM'),
                                  (dict(timed_out=True), 'bound/command_seconds/spend'),
                                  ({}, 'meter/reading')):
            with self.subTest(signature=signature):
                state, _ = drive()
                state, _ = event(state, 'meter_due', 1100)
                state, effects = done(state, 1101, status=7)
                self.assertEqual(effects, [])
                state, _ = event(state, 'meter_due', 2100)
                state, effects = done(state, 2101, **fields)
                self.assertEqual(effects[0], dict(op='meter_failure', signature=signature))
                self.assertEqual(effects[1]['op'], 'stop_driver')
                self.assertIsNone(state.data['meter']['spend'])

    def test_a_reading_without_its_line_ends_the_drive_at_once(self):
        state, _ = drive()
        state, _ = event(state, 'meter_due', 1100)
        state, effects = done(state, 1101)
        self.assertEqual(effects[0], dict(op='meter_failure', signature='meter/reading'))
        self.assertEqual(effects[1]['op'], 'stop_driver')
        self.assertIsNone(state.data['meter']['spend'])

    def test_good_reading_resets_streak_and_failed_last_keeps_spend(self):
        state, _ = drive()
        for ms, fields in ((1100, dict(status=3)), (2100, dict(spend=0)), (3100, dict(status=3))):
            state, _ = event(state, 'meter_due', ms)
            state, effects = done(state, ms + 1, **fields)
            self.assertEqual(effects, [])
        state.data['driver']['inventory'] = {}
        state, effects = event(state, 'driver_closed', 3200, result={'status': 0})
        self.assertTrue(state.data['meter']['final'])
        state, effects = done(state, 3201, status=4)
        self.assertEqual(effects[0]['signature'], 'meter/exit:4')
        self.assertEqual(state.data['meter']['spend'], 0)
        self.assertEqual(state.data['phase'], 'collect')

    def test_timeout_is_tolerated_once_and_kills_only_meter(self):
        state, _ = drive()
        state, _ = event(state, 'meter_due', 1100)
        state, _ = event(state, 'spawned', 1100, group='meter', name='spend')
        state, effects = event(state, 'meter_due', 6100)
        self.assertEqual(effects, [dict(op='cancel_command', group='meter', name='spend', timed_out=True)])
        state, effects = done(state, 6101, signal='SIGKILL')
        self.assertEqual(effects[-1]['group'], 'meter')
        self.assertFalse(any(e['op'] in ('bound', 'meter_failure') for e in effects))
        state, _ = event(state, 'spawned', 6101, group='meter', name='spend')
        state, _ = event(state, 'meter_due', 11101)
        state, effects = done(state, 11102, signal='SIGKILL')
        self.assertEqual(effects[0]['signature'], 'bound/command_seconds/spend')

    def test_interruption_cancels_active_meter(self):
        state, _ = drive(False)
        state, effects = event(state, 'interrupt', 101, signal=2)
        self.assertIn(dict(op='cancel_command', group='meter', name='spend'), effects)
        state, effects = done(state, 102, signal='SIGKILL')
        self.assertFalse(any(e['op'] == 'spawn' for e in effects))


class Windows(unittest.TestCase):
    def test_brake_admits_swapped_windows_and_refuses_each_at_share(self):
        for swapped in (False, True):
            self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=snapshot(swapped=swapped))), 50, 75),
                             (0, 'go: five-hour 12%/50%, week 3%/75%'))
        for five, week, line in ((50, 3, 'five-hour 50%/50%'), (12, 75, 'week 75%/75%'), (12, 80, 'week 80%/75%')):
            seen = f' (seen: primary 300 min {five}% resets 100, secondary 10080 min {week}% resets ?)'
            self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=snapshot(five, week))), 50, 75),
                             (1, 'refused: ' + line + seen))

    def test_brake_week_alone_admits_and_a_reported_five_hour_window_binds(self):
        # A plan reporting the week alone (promax, read 2026-10-23) has no five-hour share to keep.
        week = dict(limitId='codex', planType='promax',
                    primary=dict(windowDurationMins=10080, usedPercent=1, resetsAt=1793382149), secondary=None)
        self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=week)), 50, 75), (0, 'go: five-hour none, week 1%/75%'))
        week['primary']['usedPercent'] = 75
        self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=week)), 50, 75),
                         (1, 'refused: week 75%/75% (seen: limitId codex, planType promax, '
                             'primary 10080 min 75% resets 1793382149, secondary none)'))
        self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=snapshot(60, 3))), 50, 75),
                         (1, 'refused: five-hour 60%/50% (seen: primary 300 min 60% resets 100, '
                             'secondary 10080 min 3% resets ?)'))
        self.assertEqual(codex.brake_fold(dict(result=dict(rateLimits=dict(rateLimitReachedType='rate_limit_reached',
                                                                          primary=week['primary']))), 50, 75)[0], 1)

    def test_brake_missing_reached_error_and_malformed(self):
        none = ' (seen: primary none, secondary none)'
        for response, line in ((dict(result=dict(rateLimits=dict(primary=None, secondary=None))), 'week window missing' + none),
                               (dict(result=dict(rateLimits=dict(primary=snapshot()['primary']))),
                                'week window missing (seen: primary 300 min 12% resets 100, secondary none)'),
                               (dict(result=dict(rateLimits=dict(primary=dict(windowDurationMins=60, usedPercent=1),
                                                                 secondary=snapshot()['secondary']))),
                                'window of unknown duration or use (seen: primary 60 min 1% resets ?, '
                                'secondary 10080 min 3% resets ?)'),
                               (dict(result=dict(rateLimits=dict(primary=dict(windowDurationMins=10080, usedPercent=None)))),
                                'window of unknown duration or use (seen: primary 10080 min ?% resets ?, secondary none)'),
                               (dict(result=dict(rateLimits=dict(rateLimitReachedType='rate_limit_reached'))), 'rate_limit_reached' + none),
                               (dict(error=dict(message='no account')), 'rateLimits: no account'),
                               (dict(result={}), 'rateLimits missing'),
                               (dict(result=dict(rateLimits=dict(primary=[]))), 'rateLimits malformed' + none),
                               # A fractional percent and a null duration show as read; the plan and limit
                               # ids are named, the account id and anything not an id never.
                               (dict(result=dict(accountId='acct-1', rateLimits=dict(
                                   limitId='codex', planType='plus', accountId='acct-2', limitName='Codex limit',
                                   primary=dict(windowDurationMins=None, usedPercent=None, resetsAt=None),
                                   secondary=dict(windowDurationMins=10080, usedPercent=3.5, resetsAt=7)))),
                                'rateLimits malformed (seen: limitId codex, planType plus, '
                                'primary ? min ?% resets ?, secondary 10080 min 3.5% resets 7)'),
                               (dict(result=dict(rateLimits=dict(
                                   primary=dict(windowDurationMins=None, usedPercent=None, resetsAt=None),
                                   secondary=dict(windowDurationMins=10080, usedPercent=3, resetsAt=7)))),
                                'window of unknown duration or use (seen: primary ? min ?% resets ?, secondary 10080 min 3% resets 7)'),
                               (dict(result=dict(rateLimits=dict(
                                   primary=dict(windowDurationMins=None, usedPercent=5, resetsAt=None),
                                   secondary=None))),
                                'window of unknown duration or use (seen: primary ? min 5% resets ?, secondary none)')):
            with self.subTest(response=response):
                self.assertEqual(codex.brake_fold(response, 50, 75), (1, 'refused: ' + line))

    def test_notifications_replace_last_windows_without_actions_or_audit(self):
        state = codex.initial()
        self.assertIsNone(state['windows'])
        for data in (snapshot(), snapshot(20, 8, True)):
            state = codex.fold(state, dict(method='account/rateLimits/updated', params=dict(rateLimits=data)), 1)
        self.assertEqual(state['windows'], [dict(minutes=300, used_percent=20, resets_at=100),
                                            dict(minutes=10080, used_percent=8, resets_at=None)])
        self.assertEqual(state['actions'], [])
        state = codex.fold(state, dict(method='account/rateLimits/updated', params=dict(
            rateLimits=dict(primary=dict(usedPercent=1), secondary=None))), 2)
        self.assertEqual(state['windows'], [dict(minutes=None, used_percent=1, resets_at=None)])
