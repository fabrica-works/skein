use super::tables::{self, Method, Tables};
use crate::{api, github::document};
use alloc::boxed::Box;

fn key(repository: u32) -> api::Repository {
    api::Repository { forge: 3, repository }
}
fn document() -> document::Repository {
    document::Repository {
        id: 7,
        owner: Box::from(*b"owner"),
        name: Box::from(*b"name"),
        owner_id: 9_007_199_254_740_993,
        organization: true,
        default_branch: Box::from(*b"main"),
        html_url: Box::from(*b"https://example.invalid/owner/name"),
        merge: true,
        squash: true,
        rebase: true,
    }
}

#[test]
fn github_plans_tables_evict_least_recently_used() {
    let row = tables::repository(&document());
    let mut tables = Tables::new(2, 2);
    tables.keep_repository(key(1), row.clone());
    tables.keep_repository(key(2), row.clone());
    tables.keep_repository(key(3), row.clone());
    assert_eq!(tables.repository(key(1)), None);
    assert_eq!(tables.repository(key(2)), Some(&row));
    tables.keep_repository(key(4), row.clone());
    assert_eq!(tables.repository(key(3)), None);
    assert_eq!(tables.repository(key(2)), Some(&row));
    assert_eq!(tables.repository(key(4)), Some(&row));

    let mut replacement = row.clone();
    replacement.name = Box::from(*b"renamed");
    tables.keep_repository(key(2), replacement.clone());
    assert_eq!(tables.repository(key(2)), Some(&replacement));
    assert_eq!(tables.repository(key(4)), Some(&row));
    tables.drop_repository(key(2));
    tables.drop_repository(key(2));
    assert_eq!(tables.repository(key(2)), None);

    tables.keep_user(3, 1, Box::from(*b"first"));
    tables.keep_user(3, 2, Box::from(*b"second"));
    tables.keep_user(3, 3, Box::from(*b"third"));
    assert_eq!(tables.user(3, 1), None);
    assert_eq!(tables.user(3, 2), Some(b"second".as_slice()));
    tables.keep_user(4, 2, Box::from(*b"other-forge"));
    assert_eq!(tables.user(3, 3), None);
    assert_eq!(tables.user(3, 2), Some(b"second".as_slice()));
    assert_eq!(tables.user(4, 2), Some(b"other-forge".as_slice()));
    tables.keep_user(3, 2, Box::from(*b"renamed"));
    assert_eq!(tables.user(3, 2), Some(b"renamed".as_slice()));
    assert_eq!(tables.user(4, 2), Some(b"other-forge".as_slice()));

    let mut zero = Tables::new(0, 0);
    zero.keep_repository(key(1), row);
    zero.keep_user(3, 1, Box::from(*b"first"));
    assert_eq!(zero.repository(key(1)), None);
    assert_eq!(zero.user(3, 1), None);
    zero.drop_repository(key(1));
}

#[test]
fn github_plans_row_from_document() {
    let mut document = document();
    for (merge, squash, rebase, method) in [
        (true, true, true, Some(Method::Merge)),
        (false, true, true, Some(Method::Squash)),
        (false, false, true, Some(Method::Rebase)),
        (false, false, false, None),
    ] {
        document.merge = merge;
        document.squash = squash;
        document.rebase = rebase;
        assert_eq!(
            tables::repository(&document),
            tables::Repository {
                owner: document.owner.clone(),
                name: document.name.clone(),
                owner_id: document.owner_id,
                organization: document.organization,
                merge: method,
                html_url: document.html_url.clone(),
            }
        );
    }
}
