use super::github_fixtures as fixtures;
use crate::github::checks::{self, Lacking};
use skein_forge::github::{
    document::{App, Permission},
    setup::Access,
};
use skein_lib::List;

// forge-setup.md, 5.6: nine permissions followed by fourteen events.
const WANTED: u32 = 23;

fn permission(name: &[u8], access: Access) -> Lacking {
    Lacking::Permission { name: name.into(), access }
}
fn event(name: &[u8]) -> Lacking {
    Lacking::Event { name: name.into() }
}
fn wanted() -> Box<[Lacking]> {
    [
        permission(b"contents", Access::Write),
        permission(b"pull_requests", Access::Write),
        permission(b"issues", Access::Write),
        permission(b"merge_queues", Access::Write),
        permission(b"checks", Access::Read),
        permission(b"statuses", Access::Read),
        permission(b"actions", Access::Read),
        permission(b"administration", Access::Read),
        permission(b"metadata", Access::Read),
        event(b"issues"),
        event(b"issue_comment"),
        event(b"pull_request"),
        event(b"pull_request_review"),
        event(b"pull_request_review_comment"),
        event(b"push"),
        event(b"create"),
        event(b"delete"),
        event(b"check_run"),
        event(b"check_suite"),
        event(b"status"),
        event(b"workflow_job"),
        event(b"workflow_run"),
        event(b"merge_group"),
    ]
    .into()
}
fn without(app: App, missing: &Lacking) -> App {
    let mut permissions = List::with_capacity(WANTED);
    let mut events = List::with_capacity(WANTED);
    for row in app.permissions {
        let omit = match missing {
            Lacking::Permission { name, .. } => row.name == *name,
            Lacking::Event { .. } => false,
        };
        if !omit {
            permissions.push(row).expect("bounded fixture");
        }
    }
    for name in app.events {
        let omit = match missing {
            Lacking::Permission { .. } => false,
            Lacking::Event { name: missing } => name == *missing,
        };
        if !omit {
            events.push(name).expect("bounded fixture");
        }
    }
    App { permissions: permissions.into_boxed(), events: events.into_boxed(), ..app }
}

#[test]
fn each_missing_permission_or_event_is_named_once() {
    let app = fixtures::app();
    for missing in wanted() {
        let narrowed = without(app.clone(), &missing);
        assert_eq!(checks::lacks(&narrowed).as_ref(), &[missing]);
    }
    let empty = App { permissions: Box::new([]), events: Box::new([]), ..app.clone() };
    assert_eq!(checks::lacks(&empty), wanted());
    assert!(checks::lacks(&app).is_empty());
}

#[test]
fn write_satisfies_read_but_read_does_not_satisfy_write() {
    for held in [Access::Write, Access::Admin] {
        let mut app = fixtures::app();
        for row in &mut app.permissions {
            row.access = held;
        }
        assert!(checks::lacks(&app).is_empty());
    }
    let mut app = fixtures::app();
    for row in &mut app.permissions {
        row.access = Access::Read;
    }
    assert_eq!(
        checks::lacks(&app).as_ref(),
        &[
            permission(b"contents", Access::Write),
            permission(b"pull_requests", Access::Write),
            permission(b"issues", Access::Write),
            permission(b"merge_queues", Access::Write),
        ]
    );
}

#[test]
fn unknown_extra_access_does_not_expand_the_wanted_set() {
    // Unknown/none access is rejected by the dialect before admission.
    for access in [b"unknown".as_slice(), b"none", b""] {
        assert_eq!(Access::read(access), None);
    }
    let missing = permission(b"contents", Access::Write);
    let mut app = without(fixtures::app(), &missing);
    let mut permissions = List::with_capacity(WANTED);
    for row in app.permissions {
        permissions.push(row.clone()).expect("first copy");
        permissions.push(row).expect("duplicate copy");
    }
    for name in [b"workflows".as_slice(), b"unknown_permission"] {
        permissions.push(Permission { name: name.into(), access: Access::Admin }).expect("extra grant");
    }
    app.permissions = permissions.into_boxed();
    let mut events = List::with_capacity(WANTED.checked_mul(2).expect("duplicate events"));
    for name in app.events {
        events.push(name.clone()).expect("first event");
        events.push(name).expect("duplicate event");
    }
    events.push(b"unknown_event".as_slice().into()).expect("extra event");
    app.events = events.into_boxed();
    assert_eq!(checks::lacks(&app).as_ref(), &[missing]);
}
