"""Browser identity is requested by templates; plain experiments never inspect it."""
from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from runner import canonical, cli, experiment, store, transport
from runner.records import Refused, lab_refusals
from .fixtures import D, fixture


def browser_fixture():
    exp, subject, settings, facts, lab = fixture()
    browser = {'program': '/browser/program',
               'chromium': {'path': '/browser/chromium', 'sha256': D, 'version': '147.0.7727.101'},
               'fonts': {'path': '/browser/fonts', 'tree': D}}
    settings['browser'] = browser
    facts['known'].update({'runner': '/immutable/runner', 'browser.program': browser['program'],
                           'browser.chromium': browser['chromium']['path'], 'browser.fonts': browser['fonts']['path']})
    facts['executables'].update({path: {'regular': True, 'exec': True, 'sha256': D}
                                for path in ('/immutable/runner/look/capture', '/browser/program', '/browser/chromium')})
    facts['browser'] = {'chromium': {'path': '/browser/chromium', 'sha256': D}, 'fonts': D}
    exp['collect'][0]['argv'] = ['{runner}/look/capture', '{browser.chromium}', '{browser.program}', '{browser.fonts}']
    return exp, subject, settings, facts, lab


class BrowserSettings(unittest.TestCase):
    def test_plain_experiment_ignores_absent_null_stale_and_malformed_browser(self):
        exp, subject, settings, facts, lab = fixture()
        expected = canonical.record_bytes(experiment.key(D, subject, lab))
        variants = [{}, {'browser': None}, {'browser': {'program': '/gone/program',
                    'chromium': {'path': '/gone/chromium', 'sha256': D, 'version': '1'},
                    'fonts': {'path': '/gone/fonts', 'tree': D}}}, {'browser': ['malformed']}]
        for variant in variants:
            with self.subTest(variant=variant):
                self.assertFalse(experiment.uses_browser(exp, facts['experiment_files']))
                self.assertEqual(experiment.validate(exp, subject, {**settings, **variant}, facts), [])
                value = experiment.key(D, subject, lab)
                self.assertNotIn('browser', value['lab'])
                self.assertEqual(canonical.record_bytes(value), expected)
                self.assertEqual(canonical.digest(value), canonical.digest(experiment.key(D, subject, lab)))

    def test_browser_and_runner_templates_with_good_facts(self):
        exp, subject, settings, facts, lab = browser_fixture()
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        self.assertEqual(experiment.render('{runner}/look/capture', facts['known']), '/immutable/runner/look/capture')
        lab['browser'] = {'chromium': settings['browser']['chromium'], 'fonts': D}
        lab['programs'].insert(0, {'path': '/browser/program', 'sha256': D})
        value = experiment.key(D, subject, lab)
        self.assertEqual(value['lab']['browser'], lab['browser'])
        self.assertNotIn('/immutable/runner/look/capture', [p['path'] for p in value['lab']['programs']])
        for template in ('{browser.program}', '{browser.chromium}'):
            exp['collect'][0]['argv'][0] = template
            self.assertEqual(experiment.validate(exp, subject, settings, facts), [])

    def test_browser_refusals_name_the_member(self):
        changes = [(lambda s, f: s.pop('browser'), 'settings.browser'),
                   (lambda s, f: s.update(browser=None), 'settings.browser'),
                   (lambda s, f: f['browser']['chromium'].update(sha256='sha256:' + 'b' * 64), 'settings.browser.chromium'),
                   (lambda s, f: f['browser']['chromium'].update(path='/other'), 'settings.browser.chromium'),
                   (lambda s, f: f['browser'].update(fonts='sha256:' + 'b' * 64), 'settings.browser.fonts'),
                   (lambda s, f: f['executables']['/browser/program'].update(exec=False), 'settings.browser.program')]
        for change, member in changes:
            exp, subject, settings, facts, _ = browser_fixture()
            change(settings, facts)
            with self.subTest(member=member):
                self.assertIn(member, [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_every_template_location_requests_browser(self):
        for location in ('argv', 'env', 'stdin', 'file', 'reads', 'writes'):
            exp, subject, settings, facts, _ = fixture()
            text = '{browser.fonts}'
            if location == 'argv':
                exp['collect'][0]['argv'].append(text)
            elif location == 'env':
                exp['processes'][0]['env']['FONTS'] = text
            elif location == 'stdin':
                exp['actions'][0]['stdin'] = text
            elif location == 'file':
                facts['experiment_files']['app.conf'] = text
            else:
                exp['driver'] = {'charter': 'charter', 'tools': [dict(provision=dict(args=[], status=0), name='tool', argv=['{artifact.app}'], env={}, reads=[], writes=[])]}
                exp['driver']['tools'][0][location] = [text]
            with self.subTest(location=location):
                self.assertTrue(experiment.uses_browser(exp, facts['experiment_files']))
                self.assertIn('settings.browser', [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, _, _, facts, _ = fixture()
        exp['actions'][0]['stdin'] = '{{browser.chromium}}'
        self.assertFalse(experiment.uses_browser(exp, facts['experiment_files']))

    def test_templates_are_known_in_files_environments_stdin_and_tools(self):
        exp, subject, settings, facts, _ = browser_fixture()
        exp['processes'][0]['argv'].extend(['{runner}', '{browser.program}'])
        exp['processes'][0]['env']['CHROMIUM'] = '{browser.chromium}'
        facts['experiment_files']['app.conf'] = '{runner} {browser.fonts}'
        exp['actions'][0]['stdin'] = '{browser.program} {browser.chromium} {browser.fonts}'
        host = {'program': {'path': '/codex', 'sha256': D, 'version': '1'},
                'model': 'model', 'effort': 'medium'}
        settings.update(driver=host, triager=None)
        exp['driver'] = {'charter': 'charter', 'tools': [dict(provision=dict(args=[], status=0), name='look', argv=['{browser.program}'],
            env={'RUNNER': '{runner}'}, reads=['{runner}/look', '{browser.fonts}'], writes=['{browser.fonts}'])]}
        facts['experiment_files']['charter'] = 'Inspect the page.'
        facts['driver_host'] = True
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])

    def test_runner_tests_and_fonts_executable_are_refused(self):
        for path in ('{runner}/tests/helper', '{runner}/../escape', '{browser.fonts}'):
            exp, subject, settings, facts, _ = browser_fixture()
            exp['collect'][0]['argv'][0] = path
            facts['executables'][experiment.render(path, facts['known'])] = {'regular': True, 'exec': True, 'sha256': D}
            with self.subTest(path=path):
                self.assertIn('collect.0.argv.0', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_key_browser_shape_is_optional_and_exact(self):
        lab = fixture()[4]
        self.assertEqual(lab_refusals(lab), [])
        good = {'chromium': {'path': '/chromium', 'sha256': D, 'version': '1'}, 'fonts': D}
        self.assertEqual(lab_refusals({**lab, 'browser': good}), [])
        bad = [None, {}, {**good, 'extra': True}, {**good, 'fonts': 'bad'},
               {**good, 'chromium': {'path': 'relative', 'sha256': 'bad', 'version': ''}}]
        for browser in bad:
            with self.subTest(browser=browser):
                self.assertTrue(lab_refusals({**lab, 'browser': browser}))


class BrowserHost(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.lab = store.open_lab(self.root / 'lab')
        self.fonts = self.root / 'fonts'
        self.fonts.mkdir()
        (self.fonts / 'regular.ttf').write_bytes(b'font')
        self.program = self.executable('program')
        self.chromium = self.executable('chromium')
        self.helper = self.executable('helper')
        self.runner = self.root / 'runner'
        (self.runner / 'look').mkdir(parents=True)
        self.capture = self.runner / 'look/capture'
        self.capture.write_bytes(b'capture')
        self.capture.chmod(0o700)
        self.directory = self.root / 'experiment'
        self.directory.mkdir()
        self.exp = dict(format='lab-experiment/1', name='browser', cost='smoke', dirs=[], processes=[], actions=[],
                        driver=None, faults=[], planted=[], checks=[],
                        collect=[dict(name='look', argv=['{runner}/look/capture', '{browser.chromium}', '{browser.program}', '{browser.fonts}'], env={}, scan=[])])
        self.settings = fixture()[2]
        self.settings['browser'] = {'program': str(self.program),
            'chromium': {'path': str(self.chromium), 'sha256': store.file_digest(b'chromium'), 'version': '147.0.7727.101'},
            'fonts': {'path': str(self.fonts), 'tree': canonical.tree_digest(store.walk(self.fonts))}}
        self.write_inputs()
        subject = dict(format='lab-subject/1', commit='a' * 40, source=canonical.tree_digest([]), artifacts={})
        self.subject_id = canonical.digest(subject)
        self.lab.capture('subjects', self.subject_id,
                         [dict(path='subject.json', data=canonical.record_bytes(subject), exec=False)], empty_dirs=('source',))
        facts = {name: True for name in ('private', 'lock_available', 'disk_ok', 'containment', 'bubblewrap')}
        for patcher in (patch.object(cli, '_machine_facts', return_value=facts),
                        patch.object(cli.sys, 'executable', str(self.helper)),
                        patch.object(cli.shutil, 'which', return_value=None),
                        patch.object(cli, '_runner_root', return_value=self.runner)):
            patcher.start()
            self.addCleanup(patcher.stop)

    def executable(self, name):
        path = self.root / name
        path.write_bytes(name.encode())
        path.chmod(0o700)
        return path

    def write_inputs(self):
        (self.directory / 'experiment.json').write_bytes(canonical.record_bytes(self.exp))
        (self.lab.root / 'settings.json').write_bytes(canonical.record_bytes(self.settings))
        (self.lab.root / 'settings.json').chmod(0o600)

    def tearDown(self):
        self.lab.close()
        for directory, _, _ in os.walk(self.root):
            os.chmod(directory, 0o700)
        self.temp.cleanup()

    def validate(self):
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            status = cli.main(['validate', str(self.directory), '--subject', self.subject_id], lab_root=self.lab.root)
        return status, output.getvalue().strip(), errors.getvalue()

    def key(self, identity):
        return canonical.loads(self.lab.read(f'keys/browser/{identity}/key.json'))

    def test_cli_computes_browser_and_runner_identities(self):
        status, identity, errors = self.validate()
        self.assertEqual(status, 0, errors)
        value = self.key(identity)
        self.assertEqual(value['lab']['browser'], {'chromium': self.settings['browser']['chromium'],
                                                 'fonts': canonical.tree_digest(store.walk(self.fonts))})
        self.assertIn({'path': str(self.program), 'sha256': store.file_digest(b'program')}, value['lab']['programs'])
        self.assertNotIn(str(self.capture), [p['path'] for p in value['lab']['programs']])
        self.assertEqual(value['lab']['runner'], canonical.tree_digest(store.walk(self.runner)))
        known = transport._runtime_known(self.lab, value)
        self.assertEqual([experiment.render(t, known) for t in self.exp['collect'][0]['argv']],
                         [str(self.capture), str(self.chromium), str(self.program), str(self.fonts)])

    def test_cli_refuses_changed_fonts_chromium_and_nonexecutable_program(self):
        for member, path in (('fonts', self.fonts / 'regular.ttf'), ('chromium', self.chromium), ('program', self.program)):
            data = path.read_bytes()
            if member == 'program':
                path.chmod(0o600)
            else:
                path.write_bytes(b'changed')
            with self.subTest(member=member):
                status, identity, errors = self.validate()
                self.assertEqual(status, 2)
                self.assertEqual(identity, '')
                self.assertIn('settings.browser.' + member, errors)
            path.write_bytes(data)
            if member == 'program':
                path.chmod(0o700)

    def test_cli_refuses_missing_malformed_and_unreadable_browser(self):
        original = deepcopy(self.settings['browser'])
        variants = [None, {}, {**original, 'chromium': {**original['chromium'], 'path': str(self.root / 'missing')}},
                    {**original, 'fonts': {**original['fonts'], 'path': str(self.root / 'missing')}},
                    {**original, 'program': str(self.root / 'missing')}]
        for browser in variants:
            self.settings['browser'] = browser
            self.write_inputs()
            with self.subTest(browser=browser):
                status, _, errors = self.validate()
                self.assertEqual(status, 2)
                self.assertIn('settings.browser', errors)

    def test_cli_refuses_chromium_alias_and_fonts_links_and_special_files(self):
        alias = self.root / 'alias'
        alias.symlink_to(self.chromium)
        self.settings['browser']['chromium']['path'] = str(alias)
        self.write_inputs()
        self.assertIn('settings.browser.chromium', self.validate()[2])
        self.settings['browser']['chromium']['path'] = str(self.chromium)
        self.write_inputs()
        bad = self.fonts / 'bad'
        bad.symlink_to(self.fonts / 'regular.ttf')
        self.assertIn('settings.browser.fonts', self.validate()[2])
        bad.unlink()
        os.mkfifo(bad)
        self.assertIn('settings.browser.fonts', self.validate()[2])

    def test_runtime_refuses_browser_changed_before_provision(self):
        status, identity, errors = self.validate()
        self.assertEqual(status, 0, errors)
        value = self.key(identity)
        for member, path in (('fonts', self.fonts / 'regular.ttf'), ('chromium', self.chromium)):
            data = path.read_bytes()
            path.write_bytes(b'changed')
            with self.subTest(member=member), self.assertRaisesRegex(RuntimeError, 'browser changed before provision:.*' + member):
                transport._runtime_known(self.lab, value)
            path.write_bytes(data)
        self.program.unlink()
        with self.assertRaisesRegex(RuntimeError, 'browser changed before provision:.*program'):
            transport._runtime_known(self.lab, value)

    def test_plain_cli_key_bytes_and_preparation_ignore_stale_browser(self):
        self.exp['collect'][0]['argv'] = ['{runner}/look/capture']
        variants = [{}, {'browser': None}, {'browser': {'program': '/gone/program',
                    'chromium': {'path': '/gone/chromium', 'sha256': D, 'version': '1'},
                    'fonts': {'path': '/gone/fonts', 'tree': D}}}, {'browser': 'malformed'}]
        expected = None
        for variant in variants:
            self.settings.pop('browser', None)
            self.settings.update(variant)
            self.write_inputs()
            with self.subTest(variant=variant), patch.object(cli, '_browser_facts', side_effect=AssertionError('browser inspected')):
                status, identity, errors = self.validate()
                self.assertEqual(status, 0, errors)
                value = self.key(identity)
                self.assertNotIn('browser', value['lab'])
                data = canonical.record_bytes(value)
                if expected is None:
                    expected = data
                self.assertEqual(data, expected)
                with patch.object(self.lab, 'read', side_effect=AssertionError('settings read')):
                    self.assertEqual(transport._runtime_known(self.lab, value), {'runner': str(self.runner)})

    def test_runner_program_must_be_executable_and_outside_tests(self):
        self.capture.chmod(0o600)
        self.assertIn('collect.0.argv.0', self.validate()[2])
        self.capture.chmod(0o700)
        (self.runner / 'tests').mkdir()
        helper = self.runner / 'tests/helper'
        helper.write_bytes(b'helper')
        helper.chmod(0o700)
        self.exp['collect'][0]['argv'][0] = '{runner}/tests/helper'
        self.write_inputs()
        self.assertIn('collect.0.argv.0', self.validate()[2])
