#![expect(clippy::disallowed_types, reason = "the kit's own tests of the look's reads (browser.md, 5.1)")]
use skein_browser::Limits;
use skein_browser::boundary::{Event, Look, Looked, Request};
use skein_browser_world::world::World;
use skein_lib::{List, Token, Writer, base64};

/// Each answer reaches its own bound; all three terminals remain queued.
pub fn drive(world: &mut World, limits: Limits) {
    let text = "x".repeat(usize::try_from(limits.text).unwrap());
    let nodes: Vec<_> = (0..limits.entries)
        .map(|n| {
            let children = if n + 1 < limits.entries { format!("[\"{}\"]", n + 1) } else { "[]".into() };
            format!(
                r#"{{"nodeId":"{n}","role":{{"value":"{text}"}},"name":{{"value":"{text}"}},"childIds":{children}}}"#
            )
        })
        .collect();
    world.tree = format!("{{\"nodes\":[{}]}}", nodes.join(","));
    world.ask(Request::Look { page: Token::new(3), op: Token::new(30), look: Look::Tree });
    world.styles = format!(r#"{{"computedStyle":[{{"name":"{text}","value":"{text}"}}]}}"#);
    let faces: Vec<_> = (0..limits.entries)
        .map(|n| {
            format!(
                r#"{{"familyName":"{n:0width$}","isCustomFont":true,"glyphCount":1}}"#,
                width = usize::try_from(limits.text).unwrap()
            )
        })
        .collect();
    world.fonts.insert(8, format!("{{\"fonts\":[{}]}}", faces.join(",")));
    let mut properties = List::with_capacity(limits.properties);
    for _ in 0..limits.properties {
        properties.push(Box::from(text.as_bytes())).unwrap();
    }
    world.ask(Request::Look { page: Token::new(3), op: Token::new(31), look: Look::Style { node: 7, properties } });
    let bytes = vec![0; usize::try_from(limits.screenshot).unwrap()];
    let len = base64::encoded_len(base64::Alphabet::Standard, limits.screenshot).unwrap();
    let mut writer = Writer::new(usize::try_from(len).unwrap());
    base64::encode(base64::Alphabet::Standard, &bytes, &mut writer).unwrap();
    world.png = String::from_utf8(writer.finish().into_vec()).unwrap();
    world.ask(Request::Look { page: Token::new(3), op: Token::new(32), look: Look::Shot { clip: None, scale: 2 } });
    match world.above.pop().unwrap() {
        Event::Looked { looked: Looked::Tree(entries), .. } => assert_eq!(entries.len(), limits.entries),
        other => panic!("tree at bound: {other:?}"),
    }
    match world.above.pop().unwrap() {
        Event::Looked { looked: Looked::Style { values, faces, .. }, .. } => {
            assert_eq!((values.len(), faces.len()), (limits.properties, limits.entries))
        }
        other => panic!("style at bound: {other:?}"),
    }
    match world.above.pop().unwrap() {
        Event::Looked { looked: Looked::Shot(png), .. } => {
            assert_eq!(png.len(), usize::try_from(limits.screenshot).unwrap())
        }
        other => panic!("shot at bound: {other:?}"),
    }
}
