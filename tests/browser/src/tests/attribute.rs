use crate::world::{Fault, World};
use skein_browser::boundary::{Below, Event, Read, Refusal, Request};
use skein_browser::{Limits, up};
use skein_lib::{Time, Token, stream};
const PAGE: Token = Token::new(3);
const READ: Token = Token::new(4);
fn world() -> World {
    let mut world = World::new();
    world.start();
    world.open();
    world.sent.clear();
    world
}
fn ask(world: &mut World, selector: &[u8], name: &[u8]) {
    world.ask(Request::Attribute { page: PAGE, op: READ, selector: selector.into(), name: name.into() });
}
fn value(world: &mut World) -> Read {
    match world.above.pop().expect("read terminal") {
        Event::Attribute { op, read } => {
            assert_eq!(op, READ);
            read
        }
        other => panic!("expected attribute: {other:?}"),
    }
}
fn methods(world: &World) -> Vec<Box<[u8]>> {
    world
        .sent
        .iter()
        .map(|bytes| {
            skein_browser::wire::decode::Document::parse(
                bytes.strip_suffix(&[0]).expect("NUL"),
                world.env.limits.command,
            )
            .expect("command")
            .root()
            .get(b"method")
            .expect("method")
            .text()
            .expect("method text")
        })
        .collect()
}
#[test]
fn an_attribute_read_uses_only_the_three_dom_commands() {
    let mut world = world();
    ask(&mut world, b"input[type=hidden]", b"value");
    assert_eq!(value(&mut world), Read::Value(b"first".as_slice().into()));
    assert_eq!(
        methods(&world),
        [b"DOM.getDocument".as_slice(), b"DOM.querySelector", b"DOM.getAttributes"].map(Box::<[u8]>::from)
    );
    let document = skein_browser::wire::decode::Document::parse(
        world.sent[0].strip_suffix(&[0]).expect("NUL"),
        world.env.limits.command,
    )
    .expect("command");
    assert_eq!(document.root().get(b"params").expect("params").raw(), br#"{"depth":0,"pierce":false}"#);
}
#[test]
fn selector_reads_use_the_first_matching_node() {
    let mut world = world();
    world.selected = 7;
    ask(&mut world, b"input", b"value");
    let document = skein_browser::wire::decode::Document::parse(
        world.sent[2].strip_suffix(&[0]).expect("NUL"),
        world.env.limits.command,
    )
    .expect("command");
    assert_eq!(document.root().get(b"params").expect("params").get(b"nodeId").and_then(|id| id.u64()), Some(7));
}
#[test]
fn a_selector_without_an_element_returns_no_element() {
    let mut world = world();
    world.selected = 0;
    ask(&mut world, b".absent", b"value");
    assert_eq!(value(&mut world), Read::NoElement);
}
#[test]
fn an_element_without_the_attribute_returns_no_attribute() {
    let mut world = world();
    ask(&mut world, b"input", b"absent");
    assert_eq!(value(&mut world), Read::NoAttribute);
}
#[test]
fn an_empty_attribute_is_a_value() {
    let mut world = world();
    ask(&mut world, b"input", b"empty");
    assert_eq!(value(&mut world), Read::Value(Box::new([])));
}
#[test]
fn a_selector_chromium_rejects_is_refused_protocol() {
    let mut world = world();
    world.fault(b"DOM.querySelector", Fault::InvalidSelector);
    ask(&mut world, b"[", b"value");
    assert!(matches!(world.above.pop(), Some(Event::Refused { op: READ, why: Refusal::Protocol })));
}
fn bound_world() -> World {
    // The existing startup's frame/context identifiers fit the six-byte value bound.
    let mut world = World::with_limits(Limits { text: 6, ..Limits::default() });
    world.start();
    world.open();
    world
}
#[test]
fn an_attribute_at_the_text_bound_is_kept_whole() {
    let mut world = bound_world();
    world.attributes = r#"["value","123456"]"#.into();
    ask(&mut world, b"input", b"value");
    assert_eq!(value(&mut world), Read::Value(b"123456".as_slice().into()));
}
#[test]
fn an_attribute_past_the_text_bound_is_refused() {
    let mut world = bound_world();
    world.attributes = r#"["value","1234567"]"#.into();
    ask(&mut world, b"input", b"value");
    assert!(matches!(world.above.pop(), Some(Event::Refused { op: READ, why: Refusal::Limit })));
}
fn notify(world: &mut World, message: &[u8]) {
    up(
        &mut world.browser,
        &world.env,
        Below::Replies(stream::Up::Bytes(message.into())),
        &mut world.above,
        &mut world.below,
    );
    world.tick(world.env.now);
}
const NAVIGATING: &[u8] =
    b"{\"method\":\"Page.frameStartedLoading\",\"sessionId\":\"s1\",\"params\":{\"frameId\":\"f1\"}}\0";
const LOADED: &[u8] = b"{\"method\":\"Page.loadEventFired\",\"sessionId\":\"s1\",\"params\":{}}\0";
#[test]
fn an_attribute_read_waits_for_navigation_to_finish() {
    let mut world = world();
    notify(&mut world, NAVIGATING);
    ask(&mut world, b"input", b"value");
    assert!(world.sent.is_empty());
    notify(&mut world, LOADED);
    assert_eq!(value(&mut world), Read::Value(b"first".as_slice().into()));
}
#[test]
fn a_navigation_restarts_an_attribute_from_the_document() {
    let mut world = world();
    world.fault(b"DOM.querySelector", Fault::Delay);
    ask(&mut world, b"input", b"value");
    notify(&mut world, NAVIGATING);
    world.release_delayed();
    notify(&mut world, LOADED);
    world.tick(Time::ZERO);
    assert_eq!(
        methods(&world),
        [
            b"DOM.getDocument".as_slice(),
            b"DOM.querySelector",
            b"DOM.getDocument",
            b"DOM.querySelector",
            b"DOM.getAttributes"
        ]
        .map(Box::<[u8]>::from)
    );
    assert_eq!(value(&mut world), Read::Value(b"first".as_slice().into()));
}

#[test]
fn a_selector_at_the_text_bound_is_admitted() {
    let mut world = bound_world();
    ask(&mut world, b"#input", b"value");
    assert_eq!(value(&mut world), Read::Value(b"first".as_slice().into()));
}
#[test]
fn a_selector_past_the_text_bound_is_refused() {
    let mut world = bound_world();
    ask(&mut world, b"#inputs", b"value");
    assert!(matches!(world.above.pop(), Some(Event::Refused { op: READ, why: Refusal::Limit })));
}
