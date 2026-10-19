use super::{Listing, Shared, Unusable, check, largest_row, page_size};
use crate::tests::fixtures;
use alloc::boxed::Box;
use skein_forge::api;
use skein_lib::{List, Time};

#[test]
fn each_listing_uses_its_largest_answer_row() {
    let limits = fixtures::limits();
    for (listing, row) in [
        (Listing::Items, fixtures::ROW_ITEMS),
        (Listing::Reviews, fixtures::ROW_REVIEWS),
        (Listing::Statuses, fixtures::ROW_STATUSES),
        (Listing::Remarks, fixtures::ROW_REMARKS),
        (Listing::Branches, fixtures::ROW_BRANCHES),
        (Listing::Collaborators, fixtures::ROW_COLLABORATORS),
        (Listing::PullFiles, fixtures::ROW_PULLFILES),
    ] {
        assert_eq!(
            skein_forge::measure::answer(&maximal_answer(listing)),
            Some(u64::from(row)),
            "{listing:?}: independent oracle"
        );
        assert_eq!(largest_row(listing, &limits), Some(u64::from(row)), "{listing:?}: production bound");
        for (answer_bytes, rows, cap, expected) in
            [(row - 1, 2, 2, 0), (row, 2, 2, 1), (2 * row, 2, 2, 2), (2 * row, 2, 1, 1), (2 * row, 1, 2, 1)]
        {
            let shared = Shared { rows, answer_bytes, ..fixtures::shared() };
            assert_eq!(page_size(listing, cap, &limits, &shared), expected, "{listing:?}/{answer_bytes}/{rows}/{cap}");
        }
    }
}
#[test]
fn limits_check_admits_one_largest_row_of_every_listing() {
    let limits = fixtures::limits();
    let settings = fixtures::settings();
    for (answer_bytes, expected) in
        [(fixtures::MAX_ROW - 1, Err(Unusable::PageSize { listing: Listing::Items })), (fixtures::MAX_ROW, Ok(()))]
    {
        let shared = Shared { answer_bytes, ..fixtures::shared() };
        assert_eq!(check(&limits, &shared, &settings), expected);
    }
}
fn maximal_answer(listing: Listing) -> api::Answer {
    let limits = fixtures::limits();
    let string = usize::try_from(limits.string_bytes).expect("fixture string");
    let body = usize::try_from(limits.body_bytes).expect("fixture body");
    match listing {
        Listing::Items => {
            let mut labels = List::with_capacity(limits.inner);
            for _ in 0..limits.inner {
                labels.push(fixtures::filled(string, b'l')).expect("labels");
            }
            api::Answer::Items {
                items: Box::new([api::Summary {
                    labels: labels.into_boxed(),
                    number: 1,
                    kind: api::Kind::Issue,
                    state: api::State::Open,
                    author: fixtures::WRITER,
                    key: Some(fixtures::filled(body / 2, b'k')),
                    title: fixtures::filled(string, b't'),
                    body: fixtures::filled(body, b'b'),
                    updated: Time::ZERO,
                }]),
                more: false,
                now: Time::ZERO,
            }
        }
        Listing::Reviews => api::Answer::Reviews {
            reviews: Box::new([api::Review {
                id: 1,
                provenance: api::Provenance::Original,
                revision: 0,
                author: fixtures::WRITER,
                verdict: api::Verdict::Approve,
                commit: fixtures::BRANCH_HEAD,
                key: Some(fixtures::filled(body / 2, b'k')),
                body: fixtures::filled(body, b'b'),
                at: Time::ZERO,
                official: false,
            }]),
            more: false,
        },
        Listing::Statuses => api::Answer::Statuses {
            ci: api::Ci::Passed,
            statuses: Box::new([api::Status {
                at: Time::ZERO,
                context: fixtures::filled(string, b'c'),
                check: api::Check::Passed,
                description: fixtures::filled(string, b'd'),
                url: fixtures::filled(string, b'u'),
                job: None,
            }]),
            more: false,
        },
        Listing::Remarks => api::Answer::Remarks {
            remarks: Box::new([api::Remark {
                id: 1,
                author: fixtures::WRITER,
                path: fixtures::filled(string, b'p'),
                line: 1,
                body: fixtures::filled(body, b'b'),
            }]),
            more: false,
        },
        Listing::Branches => api::Answer::Branches(Box::new([fixtures::filled(string, b'p')])),
        Listing::Collaborators => api::Answer::Collaborators {
            collaborators: Box::new([api::Collaborator { user: fixtures::WRITER, permission: api::Permission::Write }]),
            more: false,
        },
        Listing::PullFiles => api::Answer::PullFiles {
            head: fixtures::BRANCH_HEAD,
            files: Box::new([api::File {
                path: fixtures::filled(string, b'p'),
                before: Some(fixtures::filled(body, b'b')),
                after: Some(fixtures::filled(body, b'a')),
            }]),
            more: false,
        },
    }
}
