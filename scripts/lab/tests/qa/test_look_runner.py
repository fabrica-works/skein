"""Browser template/key seams, exclusively through the runner's command line."""
import copy
import os

from . import BROWSER, CHROMIUM, FONTS, LAB, LookCase, sha, tree
from runner.canonical import loads, record_bytes


class RunnerBrowserSeams(LookCase):
    def setUp(self):
        super().setUp()
        self.require_native()
        self.private_home = self.base / 'home'
        self.lab = self.private_home / 'src/rust/tmp/lab'
        self.lab.mkdir(mode=0o700, parents=True)
        # Each CLI gets a private home, so it cannot open the user's lab.
        self.environment = dict(os.environ, HOME=str(self.private_home))
        repo = self.base / 'repository'
        repo.mkdir()
        for args in [
            ['git', 'init', '-q', str(repo)],
        ]:
            result = self.cli(args, env=self.environment)
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        (repo / 'source.txt').write_text('private QA subject\n')
        for args in [
            ['git', '-C', repo, 'add', 'source.txt'],
            ['git', '-C', repo, '-c', 'user.name=Lab QA', '-c', 'user.email=qa@example.invalid',
             '-c', 'core.hooksPath=/dev/null', 'commit', '-q', '-m', 'private fixture'],
        ]:
            result = self.cli(args, env=self.environment)
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        commit = self.cli(['git', '-C', repo, 'rev-parse', 'HEAD'], env=self.environment).stdout.decode().strip()
        result = self.runner('subject', '--repository', repo, '--commit', commit,
                             '--artifact', 'fixture=' + str(repo / 'source.txt'))
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        self.subject = result.stdout.decode().strip()
        self.settings = dict(format='lab-settings/1', driver=None, triager=None,
                             costs={'qa': dict(ready_seconds=2, command_seconds=5, stop_seconds=2,
                                               driver_seconds=1, driver_actions=1, driver_input_tokens=1)})
        self.browser_settings = dict(program=str(BROWSER),
                                     chromium=dict(path=str(CHROMIUM), sha256=sha(CHROMIUM.read_bytes()),
                                                   version='147.0.7727.101'),
                                     fonts=dict(path=str(FONTS), tree=tree(FONTS)))
        self.experiment_dir = self.base / 'experiment'
        self.experiment_dir.mkdir()
        self.experiment = dict(format='lab-experiment/1', name='qa-browser', cost='qa', dirs=[],
                               processes=[], actions=[], driver=None, faults=[], planted=[], checks=[],
                               collect=[dict(name='capture', argv=['{runner}/look/capture', '--help'],
                                             env={'PATH': '/usr/bin:/bin'}, scan=[])])

    def runner(self, *args):
        return self.cli([LAB / 'lab', *args], env=self.environment, timeout=90)

    def validate(self, browser='absent'):
        settings = copy.deepcopy(self.settings)
        if browser != 'absent':
            settings['browser'] = browser
        (self.lab / 'settings.json').write_bytes(record_bytes(settings))
        (self.experiment_dir / 'experiment.json').write_bytes(record_bytes(self.experiment))
        return self.runner('validate', self.experiment_dir, '--subject', self.subject)

    def key(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        digest = result.stdout.decode().strip()
        files = list((self.lab / 'keys').rglob('key.json'))
        candidates = [loads(path.read_bytes()) for path in files if path.parent.name in [digest, digest.split(':')[-1]]]
        self.assertEqual(len(candidates), 1, f'key {digest}, files {files}')
        return digest, candidates[0]

    def use_browser(self):
        self.experiment['collect'][0]['argv'] += [
            '--browser', '{browser.program}', '--chromium', '{browser.chromium}', '--fonts', '{browser.fonts}']

    def test_runner_template_alone_ignores_absent_stale_and_malformed_browser(self):
        baseline, key = self.key(self.validate())
        self.assertNotIn('browser', key['lab'])
        for browser in [None, 7, {'program': '/gone'}, dict(self.browser_settings, fonts={'path': '/gone', 'tree': 'sha256:' + '0' * 64})]:
            with self.subTest(browser=browser):
                current, record = self.key(self.validate(browser))
                self.assertEqual(current, baseline)
                self.assertNotIn('browser', record['lab'])

    def test_plain_experiment_ignores_browser_entirely(self):
        self.experiment['collect'][0]['argv'] = ['/usr/bin/true']
        first, _ = self.key(self.validate())
        second, key = self.key(self.validate({'unreadable': ['/gone', 'invalid']}))
        self.assertEqual(first, second)
        self.assertNotIn('browser', key['lab'])

    def test_browser_templates_bind_chromium_fonts_and_program_to_key(self):
        self.use_browser()
        _, key = self.key(self.validate(self.browser_settings))
        self.assertEqual(key['lab']['browser'], dict(chromium=self.browser_settings['chromium'],
                                                    fonts=self.browser_settings['fonts']['tree']))
        program = dict(path=str(BROWSER.resolve()), sha256=sha(BROWSER.read_bytes()))
        self.assertIn(program, key['lab']['programs'])
        original, _ = self.key(self.validate(self.browser_settings))
        settings = copy.deepcopy(self.browser_settings)
        settings['chromium']['version'] += '-other'
        changed, _ = self.key(self.validate(settings))
        self.assertNotEqual(original, changed)

    def test_browser_settings_required_and_stale_inputs_refused(self):
        self.use_browser()
        for browser, member in [
            ('absent', 'browser'), (None, 'browser'),
            (dict(self.browser_settings, chromium=dict(self.browser_settings['chromium'], sha256='sha256:' + '0' * 64)), 'chromium'),
            (dict(self.browser_settings, fonts=dict(self.browser_settings['fonts'], tree='sha256:' + '0' * 64)), 'fonts'),
            (dict(self.browser_settings, program=str(self.base / 'gone')), 'program'),
        ]:
            with self.subTest(member=member, browser=browser):
                result = self.validate(browser)
                self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
                self.assertIn(member, result.stderr.decode(errors='replace'))
                self.assertFalse(list(self.lab.rglob('run.json')))

    def test_browser_reference_in_environment_also_requires_identity(self):
        self.experiment['collect'][0]['env']['QA_FONTS'] = '{browser.fonts}'
        result = self.validate()
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('browser', result.stderr.decode(errors='replace'))
        _, key = self.key(self.validate(self.browser_settings))
        self.assertEqual(key['lab']['browser']['fonts'], self.browser_settings['fonts']['tree'])

    def test_browser_reference_in_process_template_file_requires_identity(self):
        (self.experiment_dir / 'config.txt').write_text('fonts={browser.fonts}\nrunner={runner}\n')
        self.experiment['processes'] = [dict(name='idle', argv=['/usr/bin/sleep', '10'],
                                             env={'PATH': '/usr/bin:/bin'},
                                             ready=[dict(stream='stdout', prefix='ready ', capture='address')], memory_bytes=None,
                                             files=[{'path': 'config', 'from': 'config.txt'}])]
        result = self.validate()
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('browser', result.stderr.decode(errors='replace'))
        _, key = self.key(self.validate(self.browser_settings))
        self.assertEqual(key['lab']['browser']['fonts'], self.browser_settings['fonts']['tree'])

    def test_run_rechecks_browser_before_any_run_or_provisioning(self):
        self.use_browser()
        self.key(self.validate(self.browser_settings))
        stale = copy.deepcopy(self.browser_settings)
        stale['chromium']['sha256'] = 'sha256:' + '0' * 64
        settings = dict(self.settings, browser=stale)
        (self.lab / 'settings.json').write_bytes(record_bytes(settings))
        result = self.runner('run', self.experiment_dir, '--subject', self.subject, '--seed', '1')
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('chromium', result.stderr.decode(errors='replace'))
        self.assertFalse(list(self.lab.rglob('run.json')))
        self.assertFalse(list(self.lab.rglob('live.json')))

    def test_fonts_location_is_not_identity_but_content_is(self):
        import shutil
        self.use_browser()
        original, _ = self.key(self.validate(self.browser_settings))
        private_fonts = self.base / 'private-fonts'
        shutil.copytree(FONTS, private_fonts)
        settings = copy.deepcopy(self.browser_settings)
        settings['fonts']['path'] = str(private_fonts)
        relocated, _ = self.key(self.validate(settings))
        self.assertEqual(original, relocated)
        aliases = private_fonts / 'aliases.conf'
        aliases.chmod(0o600)
        aliases.write_bytes(aliases.read_bytes() + b'\n<!-- QA identity change -->\n')
        aliases.chmod(0o400)
        settings['fonts']['tree'] = tree(private_fonts)
        changed, _ = self.key(self.validate(settings))
        self.assertNotEqual(original, changed)

    def test_run_refuses_font_bytes_changed_after_validation(self):
        import shutil
        self.use_browser()
        private_fonts = self.base / 'private-fonts'
        shutil.copytree(FONTS, private_fonts)
        settings = copy.deepcopy(self.browser_settings)
        settings['fonts']['path'] = str(private_fonts)
        self.key(self.validate(settings))
        aliases = private_fonts / 'aliases.conf'
        aliases.chmod(0o600)
        aliases.write_bytes(aliases.read_bytes() + b'\n<!-- stale bytes -->\n')
        aliases.chmod(0o400)
        result = self.runner('run', self.experiment_dir, '--subject', self.subject, '--seed', '1')
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn('fonts', result.stderr.decode(errors='replace'))
        self.assertFalse(list(self.lab.rglob('run.json')))
        self.assertFalse(list(self.lab.rglob('live.json')))
