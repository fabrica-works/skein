//! GitHub contents operations through the component and the in-process fake.
mod fake;
use super::fixtures as f;
use alloc::boxed::Box;
use fake::{Fake, object};
use skein_forge::api::{Answer, Error, File, Op, Read};
use skein_lib::Duration;

fn harness(fake: &Fake) -> f::Harness {
    harness_with(fake, f::shared())
}
fn harness_with(fake: &Fake, shared: crate::Shared) -> f::Harness {
    let mut settings = f::github_settings();
    settings.forges.get_mut(0).expect("forge").access = None;
    let mut h = f::Harness::with(f::limits(), shared, settings);
    h.component.test_calls_ready();
    f::github_account(&mut h, 1, 22, b"acme", fake.token.expose());
    f::github_repository(&mut h, f::REPO, 22);
    h
}
fn run(h: &mut f::Harness, fake: &mut Fake, read: Read) -> crate::Answered {
    h.call(1, f::REPO, Op::Read(read));
    h.run(Duration::ZERO);
    h.connected(f::FORGE);
    for _ in 0_u32..64 {
        if h.request_bytes(f::FORGE).is_empty() {
            h.connected(f::FORGE);
        }
        let response = fake.answer(&h.request_bytes(f::FORGE));
        h.answer(f::FORGE, &response);
        h.run(Duration::ZERO);
        if let Some(answer) = h.answered(1) {
            return answer.clone();
        }
    }
    unreachable!("contents plan settles within sixty-four exchanges")
}
#[test]
fn each_contents_operation_answers_through_the_github_fake() {
    let mut fake = Fake::new(b"contents", false);
    let first = object(fake.first);
    let head = object(fake.head);
    let file =
        File { path: Box::from(b"file".as_slice()), before: None, after: Some(Box::from(b"contents".as_slice())) };
    for (read, cost, expected) in [
        (
            Read::File { head, path: Box::from(b"file".as_slice()), max_bytes: 8 },
            1,
            Answer::File {
                head,
                path: Box::from(b"file".as_slice()),
                bytes: Box::from(b"contents".as_slice()),
                truncated: false,
            },
        ),
        (
            Read::Compare { before: first, after: head },
            3,
            Answer::Compare {
                before: first,
                after: head,
                contains_before: true,
                files: Box::new([file.clone()]),
                commits: Box::new([head]),
            },
        ),
        (
            Read::PullFiles { number: fake.number, head, page: 1 },
            4,
            Answer::PullFiles { head, files: Box::new([file]), more: false },
        ),
    ] {
        let mut h = harness(&fake);
        let answer = run(&mut h, &mut fake, read);
        assert_eq!((answer.cost, answer.result), (cost, Ok(expected)));
    }
}
#[test]
fn file_retains_its_bound_and_marks_one_byte_past_it() {
    // The requested head is smaller than both the answer and HTTP chunk caps.
    for (content, max, kept, truncated) in [
        (b"four".as_slice(), 4, b"four".as_slice(), false),
        (b"fives".as_slice(), 4, b"five".as_slice(), true),
        (b"x".as_slice(), 0, b"".as_slice(), true),
    ] {
        let mut fake = Fake::new(content, false);
        let mut h = harness(&fake);
        let head = object(fake.head);
        let answer = run(&mut h, &mut fake, Read::File { head, path: Box::from(b"file".as_slice()), max_bytes: max });
        assert_eq!(
            (answer.cost, answer.result),
            (1, Ok(Answer::File { head, path: Box::from(b"file".as_slice()), bytes: Box::from(kept), truncated }))
        );
    }
}
#[test]
fn pull_files_reads_before_sides_at_the_merge_base() {
    let mut fake = Fake::new(b"feature only", true);
    assert_ne!(fake.base, fake.first);
    let mut h = harness(&fake);
    let head = object(fake.head);
    let number = fake.number;
    let answer = run(&mut h, &mut fake, Read::PullFiles { number, head, page: 1 });
    // main has this path, but the merge base has an empty tree.
    assert_eq!(
        (answer.cost, answer.result),
        (
            4,
            Ok(Answer::PullFiles {
                head,
                files: Box::new([File {
                    path: Box::from(b"file".as_slice()),
                    before: None,
                    after: Some(Box::from(b"feature only".as_slice()))
                }]),
                more: false,
            })
        )
    );
    assert!(skein_lib::bytes::find(&h.request_bytes(f::FORGE), b"compare/").is_some());
}

#[test]
fn whole_file_bodies_fit_the_answer_bound_and_refuse_one_byte_past_it() {
    // PullFiles pays 64 bytes for File and four bytes for its path. Compare
    // also pays 32 bytes for the commit. The rest is the whole after body.
    const ANSWER: u32 = 4096;
    for comparison in [false, true] {
        let cap = if comparison { 3996 } else { 4028 };
        for extra in [0, 1] {
            let content = f::filled(cap + extra, b'x');
            let mut fake = Fake::new(&content, false);
            let mut h = harness_with(&fake, crate::Shared { answer_bytes: ANSWER, ..f::shared() });
            let head = object(fake.head);
            let read = if comparison {
                Read::Compare { before: object(fake.first), after: head }
            } else {
                Read::PullFiles { number: fake.number, head, page: 1 }
            };
            let answer = run(&mut h, &mut fake, read);
            if extra == 1 {
                assert_eq!(answer.result, Err(Error::TooLarge));
            } else {
                let files =
                    Box::new([File { path: Box::from(b"file".as_slice()), before: None, after: Some(content) }]);
                let expected = if comparison {
                    Answer::Compare {
                        before: object(fake.first),
                        after: head,
                        contains_before: true,
                        files,
                        commits: Box::new([head]),
                    }
                } else {
                    Answer::PullFiles { head, files, more: false }
                };
                assert_eq!(answer.result, Ok(expected));
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum End {
    Loss,
    Close,
}
#[test]
fn active_file_handoffs_drop_on_connection_loss_and_component_close() {
    for end in [End::Loss, End::Close] {
        let fake = Fake::new(b"contents", false);
        let mut h = harness(&fake);
        h.call(
            1,
            f::REPO,
            Op::Read(Read::File { head: object(fake.head), path: Box::from(b"file".as_slice()), max_bytes: 8 }),
        );
        h.run(Duration::ZERO);
        h.connected(f::FORGE);
        assert_eq!(h.component.test_github_handoffs(), 1);
        match end {
            End::Loss => h.reset(f::FORGE),
            End::Close => h.send(crate::Request::Close),
        }
        h.run(Duration::ZERO);
        assert_eq!(h.component.test_github_handoffs(), 0, "{end:?}");
    }
}

#[test]
fn largest_github_compare_page_fits_one_slots_heap() {
    measure_files_page(true);
}

#[test]
fn largest_github_pull_files_page_fits_one_slots_heap() {
    measure_files_page(false);
}

#[expect(clippy::print_stdout, clippy::disallowed_macros, reason = "report measured call peaks for the heap ledger")]
fn measure_files_page(comparison: bool) {
    let l = f::limits();
    let shared = f::shared();
    let slot = crate::limits::github_call_heap(&l, &shared).expect("GitHub slot");
    let content = f::filled(256, b'x');
    let mut fake = Fake::with_files(&content, false, shared.rows);
    let mut h = harness(&fake);
    let head = object(fake.head);
    // Allocate the connection's stack and socket before counting call state.
    drop(run(&mut h, &mut fake, Read::File { head, path: Box::from(b"file0".as_slice()), max_bytes: 0 }));
    drop(h.answers.remove(&1));
    let read = if comparison {
        Read::Compare { before: object(fake.first), after: head }
    } else {
        Read::PullFiles { number: fake.number, head, page: 1 }
    };
    let mut call_heap = CallHeap { peak: 0, held: 0 };
    let span = skein_heap::Span::start();
    h.call(1, f::REPO, Op::Read(read));
    h.run(Duration::ZERO);
    h.connected(f::FORGE);
    call_heap.count(span.end());
    for _ in 0_u32..64 {
        let span = skein_heap::Span::start();
        if h.request_bytes(f::FORGE).is_empty() {
            h.connected(f::FORGE);
        }
        call_heap.count(span.end());
        // The fake's request processing and response allocations are its own.
        let response = fake.answer(&h.request_bytes(f::FORGE));
        let span = skein_heap::Span::start();
        h.answer(f::FORGE, &response);
        h.run(Duration::ZERO);
        call_heap.count(span.end());
        if let Some(answer) = h.answered(1) {
            let Ok(answer) = &answer.result else { unreachable!("full files page succeeds") };
            let files = if comparison {
                let Answer::Compare { files, commits, .. } = answer else { unreachable!("Compare answer") };
                // Component Compare requests admit one commit per page.
                assert_eq!(commits.len(), 1);
                files
            } else {
                let Answer::PullFiles { files, more, .. } = answer else { unreachable!("PullFiles answer") };
                assert!(!more);
                files
            };
            assert_eq!(u32::try_from(files.len()).expect("row count"), shared.rows);
            for file in files {
                assert_eq!(file.after.as_deref(), Some(content.as_ref()));
            }
            assert!(
                u64::try_from(call_heap.peak).expect("positive peak") <= slot,
                "comparison={comparison}: {} > {slot}",
                call_heap.peak
            );
            println!("comparison={comparison}: peak {} bytes <= GitHub call heap {slot}", call_heap.peak);
            return;
        }
    }
    unreachable!("full page settles within sixty-four exchanges")
}

// Sum only component/harness spans. Each next step starts with the retained
// growth of the preceding steps; fake allocations between them are excluded.
pub(super) struct CallHeap {
    pub(super) peak: i64,
    pub(super) held: i64,
}
impl CallHeap {
    pub(super) fn count(&mut self, growth: skein_heap::Grown) {
        self.peak = self.peak.max(self.held.checked_add(growth.peak).expect("peak growth"));
        self.held = self.held.checked_add(growth.net).expect("retained growth");
    }
}
