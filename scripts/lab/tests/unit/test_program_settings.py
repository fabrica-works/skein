"""Named host identities, tuned class values and pre-provision refusals."""
from copy import deepcopy
import os
import json
from unittest.mock import patch
import unittest
from runner import canonical, cli, experiment, records, store, transport
from runner.records import Refused
from . import test_browser_settings as browser_tests
from .fixtures import D, fixture


def named_fixture():
    exp, subject, settings, facts, lab = fixture()
    program = dict(path='/pinned/server', sha256=D, version='1')
    settings['programs'] = {'server': program}
    facts['known']['program.server'] = program['path']
    facts['executables'][program['path']] = dict(path=program['path'], sha256=D, regular=True, exec=True)
    exp['processes'][0]['argv'][0] = '{program.server}'
    return exp, subject, settings, facts, lab


class Programs(unittest.TestCase):
    def test_settings_shape_refusals_aggregate_with_template_and_host_refusals(self):
        exp, subject, settings, facts, _ = named_fixture()
        settings['programs'].update({'BAD': {}, 'broken': dict(path='relative', sha256='bad', version='', extra=True),
                                    'missing': None})
        exp['actions'][0]['stdin'] = '{program.unknown}'
        facts['executables']['/pinned/server'].update(path='/alias', sha256='sha256:' + 'b' * 64)
        refusals = experiment.validate(exp, subject, settings, facts)
        for member in ('settings.programs.BAD', 'settings.programs.BAD.path', 'settings.programs.broken.path',
                       'settings.programs.broken.sha256', 'settings.programs.broken.version',
                       'settings.programs.broken.extra', 'settings.programs.missing',
                       'settings.programs.server.path', 'settings.programs.server.sha256', 'actions.0.stdin'):
            self.assertIn(member, [r.member for r in refusals])
        self.assertIn('program unknown not named by the settings', [r.why for r in refusals])
        for programs in (None, [], 'programs'):
            settings['programs'] = programs
            self.assertIn('settings.programs', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_host_file_and_resolved_path_refusals(self):
        for change, member in ((dict(exec=False), 'path'), (dict(regular=False), 'path'),
                               (dict(path='/different'), 'path'), (dict(sha256='sha256:' + 'b' * 64), 'sha256')):
            exp, subject, settings, facts, _ = named_fixture()
            facts['executables']['/pinned/server'].update(change)
            with self.subTest(change=change):
                self.assertIn('settings.programs.server.' + member,
                              [r.member for r in experiment.validate(exp, subject, settings, facts)])
        for path in ('relative', '/pinned/../server', '/pinned/./server', '/pinned//server', '/pinned/server/', '/bad\x00'):
            with self.subTest(path=path):
                self.assertTrue(experiment.programs_settings_refusals({'server': dict(path=path, sha256=D, version='1')}))

    def test_templates_are_immutable_and_known_everywhere(self):
        exp, subject, settings, facts, _ = named_fixture()
        exp['processes'][0]['env']['SERVER'] = '{program.server}'
        exp['actions'][0]['stdin'] = '{program.server}'
        facts['experiment_files']['app.conf'] = '{program.server}'
        host = dict(program=dict(path='/codex', sha256=D, version='1'), model='model', effort='medium')
        settings.update(driver=host, triager=None)
        exp['driver'] = dict(charter='charter', tools=[dict(name='tool', argv=['{program.server}'], env={},
                                                         reads=['{program.server}'], writes=['{program.server}'])])
        facts['experiment_files']['charter'] = 'Inspect.'
        facts['driver_host'] = True
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(experiment.render('{program.server}', facts['known']), '/pinned/server')
        for group in ('processes', 'actions', 'collect', 'checks'):
            changed = deepcopy(exp)
            changed[group][0]['argv'][0] = '{program.server}'
            self.assertEqual(experiment.validate(changed, subject, settings, facts), [])

    def test_only_template_uses_request_programs(self):
        exp, subject, settings, facts, _ = fixture()
        settings['programs'] = {'unused': dict(path='/gone/program', sha256=D, version='1')}
        exp['actions'][0]['stdin'] = '{{program.unused}}'
        self.assertEqual(experiment.uses_programs(exp, facts['experiment_files']), set())
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        for location in ('argv', 'env', 'stdin', 'file', 'reads', 'writes'):
            changed = deepcopy(exp)
            files = deepcopy(facts['experiment_files'])
            if location == 'argv':
                changed['checks'][0]['argv'].append('{program.unused}')
            elif location == 'env':
                changed['processes'][0]['env']['PROGRAM'] = '{program.unused}'
            elif location == 'stdin':
                changed['actions'][0]['stdin'] = '{program.unused}'
            elif location == 'file':
                files['app.conf'] = '{program.unused}'
            else:
                changed['driver'] = dict(tools=[dict(**{location: ['{program.unused}']})])
            with self.subTest(location=location):
                self.assertEqual(experiment.uses_programs(changed, files), {'unused'})

    def test_key_program_version_and_name_only_cost_shapes(self):
        _, subject, _, _, lab = named_fixture()
        lab['programs'][0]['version'] = '1'
        self.assertEqual(records.lab_refusals(lab), [])
        identity = canonical.digest(experiment.key(D, subject, lab))
        for field, value in (('sha256', 'sha256:' + 'b' * 64), ('version', '2')):
            changed = deepcopy(lab)
            changed['programs'][0][field] = value
            self.assertNotEqual(canonical.digest(experiment.key(D, subject, changed)), identity)
        for version in ('', ' ', None, 1):
            changed = deepcopy(lab)
            changed['programs'][0]['version'] = version
            self.assertIn('lab.programs.0.version', [r.member for r in records.lab_refusals(changed)])
        for field in records.COST_FIELDS:
            changed = deepcopy(lab)
            changed['cost'][field] = 1
            self.assertIn('lab.cost.' + field, [r.member for r in records.lab_refusals(changed)])


class ProgramHost(unittest.TestCase):
    executable = browser_tests.BrowserHost.executable
    write_inputs = browser_tests.BrowserHost.write_inputs
    validate = browser_tests.BrowserHost.validate
    key = browser_tests.BrowserHost.key
    tearDown = browser_tests.BrowserHost.tearDown

    def setUp(self):
        browser_tests.BrowserHost.setUp(self)
        self.settings.pop('browser')
        self.settings['programs'] = {'server': dict(path=str(self.program), sha256='sha256:' + '0' * 64, version='1')}
        self.settings['programs']['server']['sha256'] = store.file_digest(b'program')
        self.exp['collect'][0]['argv'] = ['{program.server}']
        self.write_inputs()

    def test_key_deduplicates_helper_and_tracks_used_digest_and_version(self):
        with patch.object(cli.sys, 'executable', str(self.program)):
            status, identity, errors = self.validate()
            self.assertEqual(status, 0, errors)
            value = self.key(identity)
            self.assertEqual(value['lab']['cost'], {'name': 'smoke'})
            self.assertEqual([p for p in value['lab']['programs'] if p['path'] == str(self.program)],
                             [self.settings['programs']['server']])
            self.settings['costs']['smoke']['command_seconds'] += 1
            self.write_inputs()
            self.assertEqual(self.validate()[1], identity)
            self.assertEqual(cli._runtime_cost(self.lab, value)['command_seconds'],
                             self.settings['costs']['smoke']['command_seconds'])
            self.settings['programs']['server']['version'] = '2'
            self.write_inputs()
            self.assertNotEqual(self.validate()[1], identity)
            self.program.write_bytes(b'changed')
            self.settings['programs']['server']['sha256'] = store.file_digest(b'changed')
            self.write_inputs()
            self.assertNotEqual(self.validate()[1], identity)

    def test_unused_missing_file_is_never_inspected(self):
        self.settings['programs']['unused'] = dict(path=str(self.root / 'gone'), sha256=D, version='1')
        self.write_inputs()
        with patch.object(cli, '_program', wraps=cli._program) as read:
            status, identity, errors = self.validate()
            self.assertEqual(status, 0, errors)
            value = self.key(identity)
            self.assertNotIn(self.settings['programs']['unused'], value['lab']['programs'])
            self.assertEqual(transport._runtime_known(self.lab, value)['program.server'], str(self.program))
            self.assertNotIn(str(self.root / 'gone'), [call.args[0] for call in read.call_args_list])

    def test_used_program_refuses_stale_missing_nonexecutable_and_alias_files(self):
        original = self.settings['programs']['server']['sha256']
        self.settings['programs']['server']['sha256'] = D
        self.write_inputs()
        self.assertIn('settings.programs.server.sha256', self.validate()[2])
        self.settings['programs']['server']['sha256'] = original
        alias = self.root / 'alias'
        alias.symlink_to(self.program)
        self.settings['programs']['server']['path'] = str(alias)
        self.write_inputs()
        self.assertIn('settings.programs.server.path', self.validate()[2])
        self.settings['programs']['server']['path'] = str(self.program)
        self.write_inputs()
        self.program.chmod(0o600)
        self.assertIn('settings.programs.server.path', self.validate()[2])
        self.program.unlink()
        self.assertIn('settings.programs.server.path', self.validate()[2])
        os.mkfifo(self.program)
        self.assertIn('settings.programs.server.path', self.validate()[2])

    def test_runtime_rechecks_program_settings_and_bytes_before_any_process(self):
        status, identity, errors = self.validate()
        self.assertEqual(status, 0, errors)
        key = self.key(identity)
        for change in ('bytes', 'sha256', 'version', 'path', 'missing', 'executable'):
            configured = deepcopy(self.settings['programs']['server'])
            if change == 'bytes':
                self.program.write_bytes(b'changed')
            elif change == 'missing':
                self.program.unlink()
            elif change == 'executable':
                self.program.chmod(0o600)
            else:
                self.settings['programs']['server'][change] = {'sha256': D, 'version': '2', 'path': str(self.helper)}[change]
            self.write_inputs()
            with self.subTest(change=change), patch.object(transport.subprocess, 'Popen', side_effect=AssertionError('spawned')):
                with self.assertRaisesRegex(Refused, 'settings.programs'):
                    transport.launch(self.lab, -1, f'keys/browser/{identity}', '1', None)
            self.settings['programs']['server'] = configured
            self.program.write_bytes(b'program')
            self.program.chmod(0o700)
        self.assertFalse((self.lab.root / 'transport').exists())

    def test_runtime_cost_refuses_missing_or_invalid_values(self):
        status, identity, errors = self.validate()
        self.assertEqual(status, 0, errors)
        value = self.key(identity)
        original = deepcopy(self.settings['costs'])
        for costs in ({}, {'smoke': {}}, {'smoke': {**original['smoke'], 'extra': 1}}):
            self.settings['costs'] = costs
            self.write_inputs()
            with self.assertRaisesRegex(Refused, 'settings.costs.smoke'):
                cli._runtime_cost(self.lab, value)
        for field in records.COST_FIELDS:
            for bad in (None, False, 0, -1, 1.5, records.SAFE + 1):
                self.settings['costs'] = deepcopy(original)
                self.settings['costs']['smoke'][field] = bad
                (self.lab.root / 'settings.json').write_text(json.dumps(self.settings, separators=(',', ':'), sort_keys=True) + '\n')
                with self.subTest(field=field, bad=bad), self.assertRaises(Refused):
                    cli._runtime_cost(self.lab, value)

    def test_worker_program_refusal_is_an_exit_two_refusal_at_outer_admission(self):
        import io
        from types import SimpleNamespace
        detail = b'settings.programs.server: program changed before provision'
        wire = io.BytesIO(b'R' + len(detail).to_bytes(8, 'big') + detail)
        with self.assertRaisesRegex(Refused, 'settings.programs.server'):
            transport._admission(SimpleNamespace(recv=wire.read))
        wire = io.BytesIO(b'A')
        transport._admission(SimpleNamespace(recv=wire.read))
