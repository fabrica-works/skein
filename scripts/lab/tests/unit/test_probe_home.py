"""The probe's home listing judged against the profile's own roots (lab.md, 6.2)."""
import unittest
from runner.probe import home_listed, profile_roots
from runner.views import profile_toml

HOME = '/home/lab'
ROOTS = profile_roots(profile_toml('/home/lab/.codex/bin/codex', '/home/lab/src/lab/source', '/home/lab/src/lab/work'))


class HomeListing(unittest.TestCase):
    def test_the_profile_names_its_absolute_roots(self):
        self.assertEqual(ROOTS, ['/home/lab/.codex/bin/codex', '/home/lab/src/lab/source', '/home/lab/src/lab/work'])

    def test_a_refused_listing_passes(self):
        self.assertTrue(home_listed(True, HOME, ROOTS))

    def test_a_listing_of_exactly_the_roots_first_components_passes(self):
        self.assertTrue(home_listed(['.codex', 'src'], HOME, ROOTS))

    def test_a_subset_passes(self):
        self.assertTrue(home_listed(['src'], HOME, ROOTS))

    def test_an_empty_home_passes(self):
        self.assertTrue(home_listed([], HOME, ROOTS))

    def test_one_extra_entry_fails(self):
        self.assertFalse(home_listed(['.codex', 'notes.txt', 'src'], HOME, ROOTS))

    def test_a_component_of_a_root_outside_the_home_fails(self):
        self.assertFalse(home_listed(['srv'], HOME, ['/srv/lab/work']))

    def test_the_home_granted_whole_shows_no_root_component(self):
        self.assertFalse(home_listed(['src'], HOME, [HOME]))

    def test_another_error_or_shape_fails(self):
        for observed in (False, None, 'src', [1]):
            with self.subTest(observed=observed):
                self.assertFalse(home_listed(observed, HOME, ROOTS))

    def test_a_profile_that_does_not_parse_names_no_root(self):
        self.assertEqual(profile_roots('[permissions.lab'), [])
