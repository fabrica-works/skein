"""An agent-free cluster with a transitive ready capture in a file template."""
from runner.canonical import digest

D = 'sha256:' + 'a' * 64


def fixture():
    subject = {'format': 'lab-subject/1', 'commit': 'a' * 40, 'source': D,
               'artifacts': {'app': {'sha256': D, 'bytes': 7, 'exec': True}}}
    def proc(name, after):
        return {'name': name, 'after': after, 'argv': ['{artifact.app}', '{seed.' + name + '}'], 'env': {},
                'ready': [{'stream': 'stdout', 'prefix': 'ready ', 'capture': 'address'}], 'memory_bytes': None}
    exp = {'format': 'lab-experiment/1', 'name': 'hello', 'cost': 'smoke', 'dirs': ['client'],
           'processes': [proc('backend', []), proc('middle', ['backend']), proc('app', ['middle'])],
           'actions': [{'name': 'seed', 'argv': ['/usr/bin/helper', '{proc.app.ready.address}'], 'env': {}, 'stdin': '{{x}}'}],
           'driver': None, 'faults': [], 'planted': [],
           'collect': [{'name': 'obs', 'argv': ['{artifact.app}', '{collect.dir}'], 'env': {}, 'scan': ['log']}],
           'checks': [{'name': 'end', 'argv': ['{experiment}/check', '{collected.obs}/log'], 'env': {}}]}
    exp['processes'][2]['files'] = [{'path': 'app.conf', 'from': 'app.conf'}]
    # Director245's smoke caps; these are fixture inputs, never runner defaults.
    cost = dict(ready_seconds=60, command_seconds=120, stop_seconds=10, driver_seconds=900,
                driver_actions=80, driver_input_tokens=2500000)
    settings = {'format': 'lab-settings/1', 'costs': {'smoke': cost}, 'driver': None, 'triager': None}
    known = {'experiment': '/immutable/experiment', 'artifact.app': '/immutable/artifacts/app', 'source': '/immutable/source'}
    facts = {name: True for name in ('private', 'lock_available', 'disk_ok', 'containment', 'bubblewrap',
                                    'subject_available', 'source_available')}
    facts.update(known=known, artifacts_available=['app'],
                 experiment_files={'app.conf': 'endpoint={proc.backend.ready.address}', 'check': '#!/bin/sh\nexit 0\n'},
                 executables={path: {'regular': True, 'exec': True, 'sha256': D}
                              for path in ('/immutable/artifacts/app', '/usr/bin/helper', '/immutable/experiment/check')})
    lab = {'runner': D, 'python': '3.11.0', 'programs': [{'path': '/usr/bin/helper', 'sha256': D}],
           'driver': None, 'cost': {'name': 'smoke'}}
    return exp, subject, settings, facts, lab
