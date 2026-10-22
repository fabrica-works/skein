"""Expectation/departure admission, through capture only (lab-browser.md, 4)."""
import copy

from . import LookCase, finder, scene


class CheckSceneAdmission(LookCase):
    def setUp(self):
        super().setUp()
        self.value = scene()
        self.value['regions'] = [dict(name='target', mockup=finder('button', 'Send'), page=finder('button', 'Send'))]

    def refuse(self, value, member):
        path = self.write_scene(value)
        out = self.base / ('refused-' + str(len(list(self.base.glob('refused-*')))))
        result = self.cli(self.capture_args([path], out))
        self.assertEqual(result.returncode, 2, result.stderr.decode(errors='replace'))
        self.assertIn(member, result.stderr.decode(errors='replace'))
        self.assertFalse((out / 'browser.log').exists(), 'refusal precedes browser startup')
        self.assertFalse((out / 'capture.json').exists())

    def property_expectation(self):
        return dict(id='expected', width='small', region='target', property='color', value='rgb(1, 2, 3)', cites='QA')

    def test_property_expectation_references_and_types(self):
        for member, invalid in [('id', 'Bad'), ('width', 'absent'), ('region', 'absent'),
                                 ('property', 'opacity'), ('value', 3), ('cites', ''), ('unknown', True)]:
            with self.subTest(member=member, invalid=invalid):
                value = copy.deepcopy(self.value)
                expectation = self.property_expectation()
                expectation[member] = invalid
                value['expectations'] = [expectation]
                self.refuse(value, member)

    def test_outline_count_refuses_negative_boolean_and_noninteger(self):
        for count in [-1, True, '1', None]:
            with self.subTest(count=count):
                value = copy.deepcopy(self.value)
                value['expectations'] = [dict(id='entry-count', width='small', entry='main > button "Send"',
                                               count=count, cites='QA')]
                self.refuse(value, 'count')

    def test_ids_are_unique_across_expectations_and_departures(self):
        value = copy.deepcopy(self.value)
        value['expectations'] = [self.property_expectation()]
        value['departures'] = [dict(id='expected', width='small', signature='style/target/color', cites='QA')]
        self.refuse(value, 'id')

    def test_repeated_expectation_and_departure_ids_are_refused(self):
        for kind, entry in [('expectations', self.property_expectation()),
                            ('departures', dict(id='departed', width='small', entry='main > button "Send"', cites='QA'))]:
            with self.subTest(kind=kind):
                value = copy.deepcopy(self.value)
                value[kind] = [entry, copy.deepcopy(entry)]
                self.refuse(value, 'id')

    def test_departure_requires_width_citation_and_nonempty_signature(self):
        for member, invalid in [('width', 'absent'), ('cites', ''), ('signature', ''),
                                 ('signature', None), ('entry', 'main')]:
            with self.subTest(member=member, invalid=invalid):
                value = copy.deepcopy(self.value)
                departure = dict(id='accepted', width='small', signature='style/target/color', cites='QA')
                departure[member] = invalid
                value['departures'] = [departure]
                self.refuse(value, 'departures')

    def test_every_required_expectation_member_is_checked(self):
        for member in self.property_expectation():
            with self.subTest(member=member):
                value = copy.deepcopy(self.value)
                expectation = self.property_expectation()
                del expectation[member]
                value['expectations'] = [expectation]
                self.refuse(value, member)
