use crate::world::World;
use skein_browser::{
    boundary::{Below, Event, Expect, Match, Query, Request},
    up,
};
use skein_lib::{Duration, Time, Token, stream};

const PAGE: Token = Token::new(3);
const READ: Token = Token::new(4);
fn query() -> Query {
    Query {
        role: b"button".as_slice().into(),
        name: b"Save".as_slice().into(),
        matching: Match::Exact,
        within: None,
        boxes: false,
    }
}
fn notify(world: &mut World, body: &[u8]) {
    up(
        &mut world.browser,
        &world.env,
        Below::Replies(stream::Up::Bytes(body.into())),
        &mut world.above,
        &mut world.below,
    );
    world.tick(world.env.now);
}
fn navigating() -> World {
    let mut world = World::new();
    world.start();
    world.open();
    notify(
        &mut world,
        b"{\"method\":\"Page.frameStartedLoading\",\"sessionId\":\"s1\",\"params\":{\"frameId\":\"f1\"}}\0",
    );
    world.sent.clear();
    world
}
fn loaded(world: &mut World) {
    notify(world, b"{\"method\":\"Page.loadEventFired\",\"sessionId\":\"s1\",\"params\":{}}\0");
}
fn methods(world: &World) -> Vec<Box<[u8]>> {
    world
        .sent
        .iter()
        .map(|bytes| {
            let document = skein_browser::wire::decode::Document::parse(
                bytes.strip_suffix(&[0]).expect("NUL"),
                world.env.limits.command,
            )
            .expect("command");
            document.root().get(b"method").expect("method").text().expect("method text")
        })
        .collect()
}
#[test]
fn a_navigation_holds_page_reads_until_load() {
    for snapshot in [false, true] {
        let mut world = navigating();
        let request = if snapshot {
            Request::Snapshot { page: PAGE, op: READ }
        } else {
            Request::Find { page: PAGE, op: READ, query: query() }
        };
        world.ask(request);
        let during = methods(&world);
        loaded(&mut world);
        let expected: &[&[u8]] = if snapshot {
            &[b"Accessibility.getFullAXTree"]
        } else {
            &[b"Runtime.evaluate", b"Accessibility.queryAXTree", b"Runtime.releaseObject"]
        };
        let sent = methods(&world);
        let after: Vec<&[u8]> = sent.iter().map(AsRef::as_ref).collect();
        assert_eq!((during.len(), after), (0, expected.to_vec()), "snapshot {snapshot}");
    }
}
#[test]
fn a_held_await_keeps_its_deadline() {
    for expect in [Expect::Present, Expect::Absent] {
        let mut world = navigating();
        world.ask(Request::Await { page: PAGE, op: READ, query: query(), expect, within: Duration::from_millis(100) });
        world.tick(Time::from_nanos(100_000_000));
        match world.above.pop() {
            Some(Event::Missed { op, seen, more }) => assert_eq!((op, seen.len(), more), (READ, 0, 0)),
            other => panic!("expected deadline miss for {expect:?}, got {other:?}"),
        }
    }
}

#[test]
fn a_navigation_discards_the_previous_documents_root() {
    let mut world = World::new();
    world.start();
    world.open();
    world.sent.clear();
    world.fault(b"Runtime.evaluate", crate::world::Fault::Delay);
    world.ask(Request::Find { page: PAGE, op: READ, query: query() });
    notify(
        &mut world,
        b"{\"method\":\"Page.frameStartedLoading\",\"sessionId\":\"s1\",\"params\":{\"frameId\":\"f1\"}}\0",
    );
    world.release_delayed();
    loaded(&mut world);
    let sent = methods(&world);
    let methods: Vec<&[u8]> = sent.iter().map(AsRef::as_ref).collect();
    assert_eq!(
        methods,
        [
            b"Runtime.evaluate".as_slice(),
            b"Runtime.releaseObject",
            b"Runtime.evaluate",
            b"Accessibility.queryAXTree",
            b"Runtime.releaseObject"
        ]
    );
}

#[test]
fn a_find_retries_a_discarded_context() {
    for method in [b"Runtime.evaluate".as_slice(), b"Accessibility.queryAXTree"] {
        let mut world = World::new();
        world.start();
        world.open();
        world.fault(method, crate::world::Fault::ContextGone);
        world.ask(Request::Find { page: PAGE, op: READ, query: query() });
        world.tick(Time::from_nanos(50_000_000));
        match world.above.pop() {
            Some(Event::Found { op, seen, more }) => assert_eq!((op, seen.len(), more), (READ, 1, 0), "{method:?}"),
            other => panic!("expected a fresh query for {method:?}, got {other:?}"),
        }
    }
}
#[test]
fn a_find_asks_cdp_for_the_role_and_exact_name() {
    let mut world = World::new();
    world.start();
    world.open();
    world.ask(Request::Find { page: PAGE, op: READ, query: query() });
    let command = world
        .sent
        .iter()
        .find(|bytes| {
            bytes.windows(b"Accessibility.queryAXTree".len()).any(|part| part == b"Accessibility.queryAXTree")
        })
        .expect("AX query");
    let document = skein_browser::wire::decode::Document::parse(
        command.strip_suffix(&[0]).expect("NUL"),
        world.env.limits.command,
    )
    .expect("command");
    let params = document.root().get(b"params").expect("params");
    let values: Vec<_> = [b"objectId".as_slice(), b"role", b"accessibleName"]
        .iter()
        .map(|key| params.get(key).expect("query field").text().expect("text"))
        .collect();
    assert_eq!(values, [b"document-1".as_slice(), b"button", b"Save"].map(Box::<[u8]>::from));
}

#[test]
fn a_cached_history_restore_releases_page_reads() {
    let mut world = navigating();
    world.ask(Request::Find { page: PAGE, op: READ, query: query() });
    notify(&mut world, b"{\"method\":\"Page.frameNavigated\",\"sessionId\":\"s1\",\"params\":{\"type\":\"BackForwardCacheRestore\",\"frame\":{\"id\":\"f1\"}}}\0");
    assert_eq!(
        methods(&world),
        [b"Runtime.evaluate".as_slice(), b"Accessibility.queryAXTree", b"Runtime.releaseObject"].map(Box::<[u8]>::from)
    );
}
