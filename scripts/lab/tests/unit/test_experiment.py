from copy import deepcopy
import unittest
from runner import canonical, experiment
from runner.records import Refused
from .fixtures import D, fixture


class Experiment(unittest.TestCase):
    def test_transitive_ready_file_is_accepted_and_rendered_late(self):
        exp, subject, settings, facts, _ = fixture()
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        text = facts['experiment_files']['app.conf']
        with self.assertRaises(Refused):
            experiment.render(text, facts['known'])
        self.assertEqual(experiment.render(text, {'proc.backend.ready.address': '127.0.0.1:123'}), 'endpoint=127.0.0.1:123')
        self.assertEqual(experiment.render('{{x}} {run.seed} $HOME', {'run.seed': '1'}), '{x} 1 $HOME')

    def test_independent_malformed_members_aggregate(self):
        exp, subject, settings, facts, _ = fixture()
        exp.update(name='BAD', unknown=True, dirs=[[], 'x', 'x'], faults=[None], planted=[False], driver=[])
        exp['processes'][0].update(argv=False, env=[], ready=[None], after=[{}], files=[{'path': '../escape', 'from': []}], memory_bytes=True)
        exp['processes'][1]['argv'] = ['{artifact.missing}', '{source}', '{proc.no.ready.no}', '{dir.no}', '{collected.obs}']
        settings['costs']['smoke'].update(command_seconds=False, driver_actions=0)
        facts.update(containment=False, source_available=False)
        refusals = experiment.validate(exp, subject, settings, facts)
        members = {r.member for r in refusals}
        for member in ['experiment.name', 'experiment.unknown', 'dirs.0', 'dirs.2', 'processes.0.argv',
                       'processes.0.env', 'processes.0.ready.0', 'processes.0.files.0.path', 'processes.0.files.0.from',
                       'processes.0.memory_bytes', 'settings.costs.selected.command_seconds', 'facts.containment', 'driver']:
            self.assertIn(member, members)
        self.assertGreater(len(refusals), 20)

    def test_capture_scope_cycles_and_unknown_names(self):
        for edit, expected in [
            (lambda e: e['processes'][0]['argv'].append('{proc.app.ready.address}'), 'unavailable'),
            (lambda e: e['processes'][2]['argv'].append('{proc.backend.ready.missing}'), 'unavailable'),
            (lambda e: e['processes'][0]['after'].append('app'), 'cycle'),
            (lambda e: e['processes'][0]['after'].append('missing'), 'unknown process'),
            (lambda e: e['processes'][0]['files'].append({'path': '/escape', 'from': 'missing'}), 'contained'),
        ]:
            exp, subject, settings, facts, _ = fixture()
            exp['processes'][0]['files'] = []
            edit(exp)
            with self.subTest(expected=expected):
                self.assertTrue(any(expected in r.why for r in experiment.validate(exp, subject, settings, facts)))

    def test_every_template_location_and_executable_identity(self):
        for location in ['argv', 'env', 'stdin', 'file']:
            exp, subject, settings, facts, _ = fixture()
            p = exp['processes'][0]
            if location == 'argv':
                p['argv'].append('{proc.app.ready.address}')
            elif location == 'env':
                p['env']['VALUE'] = '{proc.app.ready.address}'
            elif location == 'stdin':
                exp['actions'][0]['stdin'] = '{missing}'
            else:
                p['files'] = [{'path': 'config', 'from': 'app.conf'}]
                facts['experiment_files']['app.conf'] = '{proc.app.ready.address}'
            with self.subTest(location=location):
                self.assertTrue(experiment.validate(exp, subject, settings, facts))
        for program in ['relative', '{dir.client}/app', '{run.id}', '{seed.app}', '{proc.app.ready.address}']:
            exp, subject, settings, facts, _ = fixture()
            exp['actions'][0]['argv'][0] = program
            with self.subTest(program=program):
                self.assertTrue(experiment.validate(exp, subject, settings, facts))
        for text in ['{', '}', '{bad name}', '{source']:
            with self.assertRaises(Refused):
                experiment.render(text, {})

    def test_missing_inputs_cost_and_machine_facts(self):
        for field in ['private', 'lock_available', 'disk_ok', 'containment', 'bubblewrap', 'subject_available']:
            exp, subject, settings, facts, _ = fixture()
            del facts[field]
            self.assertIn('facts.' + field, [r.member for r in experiment.validate(exp, subject, settings, facts)])
        exp, subject, settings, facts, _ = fixture()
        settings['costs'] = {}
        subject['source'] = None
        subject['artifacts'] = {}
        exp['actions'][0]['stdin'] = '{source}'
        facts['pruned'] = True
        members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
        self.assertIn('experiment.cost', members)
        self.assertIn('subject', members)
        self.assertIn('actions.0.stdin', members)
        self.assertIn('processes.0.argv.0', members)

    def test_null_driver_and_fault_refusal(self):
        exp, subject, settings, facts, _ = fixture()
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        exp['faults'] = [{'kill': 'backend', 'signal': 'SIGTERM', 'after_actions': [1, 2]}]
        self.assertIn('faults.0', [r.member for r in experiment.validate(exp, subject, settings, facts)])

    def test_driver_input_shapes_and_tool_templates(self):
        exp, subject, settings, facts, _ = fixture()
        host = {'program': {'path': '/usr/bin/codex', 'sha256': D, 'version': '1'},
                'model': 'gpt-6.1-sol', 'effort': 'medium'}
        settings['driver'] = host
        settings['triager'] = None
        exp['driver'] = {'charter': 'charter.md', 'tools': [
            {'provision': {'args': [], 'status': 0}, 'name': 'client', 'argv': ['{artifact.app}', '{proc.app.ready.address}'], 'env': {},
             'writes': ['{dir.client}'], 'reads': ['{source}']} ]}
        facts['experiment_files']['charter.md'] = 'Exercise the client.'
        facts['driver_host'] = True
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        exp['driver']['tools'][0].update(stdin='unknown member', writes=['../escape'])
        settings['driver'] = None
        settings['triager'] = None
        facts['driver_host'] = False
        members = [r.member for r in experiment.validate(exp, subject, settings, facts)]
        for member in ['driver.tools.0.stdin', 'driver.tools.0.writes.0', 'settings.driver',
                       'facts.driver_host']:
            self.assertIn(member, members)

    def test_tool_mounts_must_be_their_own_resolved_paths(self):
        # Bubblewrap refuses a symlink as a mount ("Can't mount on symlink destination /usr/bin/python3").
        def case():
            exp, subject, settings, facts, _ = fixture()
            settings['driver'] = {'program': {'path': '/usr/bin/codex', 'sha256': D, 'version': '1'},
                                  'model': 'gpt-6.1-sol', 'effort': 'medium'}
            exp['driver'] = {'charter': 'charter.md', 'tools': [
                {'provision': {'args': [], 'status': 0}, 'name': 'client', 'argv': ['/usr/bin/python3', '-I'], 'env': {},
                 'writes': ['{dir.client}'], 'reads': ['{source}', '/usr/share/data']}]}
            facts['experiment_files']['charter.md'] = 'Exercise the client.'
            facts['driver_host'] = True
            return exp, subject, settings, facts
        for python, roots, expected in (
                (dict(regular=True, exec=True, sha256=D), {}, []),
                (dict(path='/usr/bin/python3', regular=True, exec=True, sha256=D), {'/usr/share/data': '/usr/share/data'}, []),
                (dict(path='/usr/bin/python3.13', regular=True, exec=True, sha256=D), {},
                 [('driver.tools.0.argv.0', 'expected a resolved path: /usr/bin/python3.13')]),
                (dict(path='/usr/bin/python3', regular=True, exec=True, sha256=D),
                 {'/immutable/source': '/elsewhere/source', '/usr/share/data': '/srv/data'},
                 [('driver.tools.0.reads.0', 'expected a resolved path: /elsewhere/source'),
                  ('driver.tools.0.reads.1', 'expected a resolved path: /srv/data')])):
            exp, subject, settings, facts = case()
            facts['executables']['/usr/bin/python3'] = python
            facts['resolved_roots'] = roots
            with self.subTest(python=python, roots=roots):
                self.assertEqual([(r.member, r.why) for r in experiment.validate(exp, subject, settings, facts)], expected)
        # A process's program is no mount: its resolved path is the key's alone.
        exp, subject, settings, facts, _ = fixture()
        facts['executables']['/usr/bin/helper']['path'] = '/usr/bin/helper-1'
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])

    def test_key_identity(self):
        exp, subject, settings, facts, lab = fixture()
        identity = canonical.digest(experiment.key(D, subject, lab))
        for field in ['run.seed', 'run.id', 'dir.client', 'proc.app.ready.address', 'proc.app.dir']:
            facts['known'][field] = 'different'
            self.assertEqual(identity, canonical.digest(experiment.key(D, subject, lab)))
        subject['about'] = {'repository': '/other', 'machine': 'other'}
        self.assertEqual(identity, canonical.digest(experiment.key(D, subject, lab)))
        changed = deepcopy(subject)
        changed['artifacts']['app']['exec'] = False
        self.assertNotEqual(identity, canonical.digest(experiment.key(D, changed, lab)))
        changed['artifacts']['app']['sha256'] = 'sha256:' + 'b' * 64
        self.assertNotEqual(identity, canonical.digest(experiment.key(D, changed, lab)))
        self.assertNotEqual(identity, canonical.digest(experiment.key('sha256:' + 'b' * 64, subject, lab)))
        for change in [lambda l: l.update(runner='sha256:' + 'b' * 64),
                       lambda l: l['programs'][0].update(sha256='sha256:' + 'b' * 64),
                       lambda l: l['cost'].update(name='other'), lambda l: l.update(python='3.12.0')]:
            changed = deepcopy(lab)
            change(changed)
            self.assertNotEqual(identity, canonical.digest(experiment.key(D, subject, changed)))
        lab['driver'] = {'program': {'path': '/usr/bin/codex', 'sha256': D, 'version': '1'},
                         'model': 'gpt-6.1-sol', 'effort': 'medium', 'config': {'args': ['{run.id}']}}
        driver_identity = canonical.digest(experiment.key(D, subject, lab))
        for field, value in [('model', 'another'), ('config', {'args': ['changed']})]:
            changed = deepcopy(lab)
            changed['driver'][field] = value
            self.assertNotEqual(driver_identity, canonical.digest(experiment.key(D, subject, changed)))


class ProvisionValidation(unittest.TestCase):
    def test_required_closed_provision_and_literal_arguments(self):
        exp, subject, settings, facts, _ = fixture()
        exp['driver'] = dict(charter='charter', tools=[dict(name='tool', argv=['{artifact.app}'], env={},
            reads=[], writes=[], provision=dict(args=['{literal}', ''], status=255))])
        facts.update(driver_host=True)
        facts['experiment_files']['charter'] = 'charter'
        settings['driver'] = dict(program=dict(path='/host', sha256=D, version='1'), model='script', effort='low')
        self.assertEqual(experiment.validate(exp, subject, settings, facts), [])
        tool = exp['driver']['tools'][0]
        for provision in (None, {}, dict(args=[], status=True), dict(args=[], status=256),
                          dict(args=[], status=-1), dict(args=[1], status=0), dict(args=['bad\0argument'], status=0),
                          dict(args='literal', status=0), dict(args=[], status=0, extra=1)):
            with self.subTest(provision=provision):
                tool['provision'] = provision
                self.assertTrue(any(r.member.startswith('driver.tools.0.provision') for r in experiment.validate(exp, subject, settings, facts)))
        del tool['provision']
        self.assertTrue(any(r.member == 'driver.tools.0.provision' for r in experiment.validate(exp, subject, settings, facts)))
