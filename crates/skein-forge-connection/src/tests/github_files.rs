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
    for _ in 0_u32..16 {
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
    unreachable!("contents plan settles within sixteen exchanges")
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
