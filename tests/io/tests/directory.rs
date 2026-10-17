use skein_io_world::renames::{self, Found};
use skein_world::Cut;
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

#[test]
fn a_rename_across_cut_at_each_operation_is_in_one_directory_whole() {
    let uncut = renames::world(0).run();
    assert_eq!(uncut.procs[0].found, Some(Found::Destination));
    let (mut source, mut destination) = (false, false);
    for before in 0..uncut.submissions[0] {
        for (seed, cut) in [(0, Cut::Kill), (0, Cut::PowerLoss), (17, Cut::PowerLoss)] {
            let outcome = {
                let mut scenario = renames::world(seed);
                scenario.cut(0, before, cut);
                scenario.run()
            };
            match outcome.procs[0].found {
                Some(Found::Source) => source = true,
                Some(Found::Destination) => destination = true,
                None => panic!("both listed"),
            }
            assert_eq!(outcome.machine.files.open_handles(), 0);
        }
    }
    assert!(source && destination, "cuts leave the file in either directory");
}

#[test]
fn a_rename_across_recovers_in_its_destination_once_that_is_synced() {
    let uncut = renames::world(0).run();
    for before in uncut.procs[0].synced_at.unwrap()..uncut.submissions[0] {
        for seed in [0, 1, 17] {
            let mut scenario = renames::world(seed);
            scenario.cut(0, before, Cut::PowerLoss);
            let outcome = scenario.run();
            assert_eq!(outcome.procs[0].found, Some(Found::Destination), "cut before {before}, seed {seed}");
            assert_eq!(outcome.machine.files.open_handles(), 0);
        }
    }
}

use skein_io_world::pages;
fn each_name_once(owner: &pages::Owner, count: u32) {
    let mut listed: Vec<_> = owner.pages.iter().flatten().map(|entry| entry.name.to_vec()).collect();
    listed.sort();
    assert_eq!(listed, pages::names(count));
}
#[test]
fn ten_entries_come_in_pages_of_four_four_and_two() {
    for seed in [0, 1, 17] {
        let outcome = pages::world(seed, 10, 4).run();
        assert_eq!(outcome.procs[0].pages.iter().map(|page| page.len()).collect::<Vec<_>>(), [4, 4, 2]);
        each_name_once(&outcome.procs[0], 10);
        assert_eq!(outcome.machine.files.open_handles(), 0);
    }
}
#[test]
fn eight_entries_end_with_an_empty_third_page() {
    for seed in [0, 1, 17] {
        let outcome = pages::world(seed, 8, 4).run();
        assert_eq!(outcome.procs[0].pages.iter().map(|page| page.len()).collect::<Vec<_>>(), [4, 4, 0]);
        each_name_once(&outcome.procs[0], 8);
        assert_eq!(outcome.machine.files.open_handles(), 0);
    }
}
#[test]
fn a_page_of_max_entries_stays_within_file_ios_worst_case() {
    for seed in [0, 1, 17] {
        let outcome = pages::world(seed, pages::ENTRIES, pages::ENTRIES).run();
        assert_eq!(outcome.procs[0].pages.iter().map(|page| page.len()).collect::<Vec<_>>(), [4, 0]);
        each_name_once(&outcome.procs[0], pages::ENTRIES);
        assert_eq!(outcome.machine.files.open_handles(), 0);
    }
}
