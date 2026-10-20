use skein_fake_forge_github::keys;
use skein_forge_machine_world::github::app_key;
use skein_forge_setup::{
    github::manifest::Target,
    host::{Need, Plan},
    item::Item,
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Connect, Heard, HostWorld, fixtures};
use skein_lib::{Duration, Rng, Secret, Time, Token};
use std::{collections::BTreeMap, fmt::Write};

const FORGE: u16 = 4; // fixtures::github_forge.
const SEED: u64 = 7;
const APP: u64 = 8; // The fake starts at App 7; conversion advances its id.
const BOT: u64 = 9; // The fake's stable bot id, distinct from its App id.
const ITEMS: [Item; 4] = [Item::AppId, Item::AppSlug, Item::AppKey, Item::User];

fn empty(script: &[Connect]) -> HostWorld {
    let mut world = HostWorld::github(SEED, script);
    for item in ITEMS.into_iter().chain([Item::Accepted]) {
        world.forget(item);
    }
    world
}

// Parse the generated elements' quoted attributes; escaped JSON stays one value.
fn attributes(html: &str, element: &str) -> BTreeMap<String, String> {
    let mut tags = html.split('<').filter_map(|piece| piece.split_once('>').map(|(tag, _)| tag));
    let tag = tags.find(|tag| tag.strip_prefix(element).is_some_and(|rest| rest.starts_with(' '))).expect("element");
    let mut rest = &tag[element.len()..];
    let mut attributes = BTreeMap::new();
    while !rest.trim_start().is_empty() {
        rest = rest.trim_start();
        let (name, value) = rest.split_once('=').expect("attribute assignment");
        let quote = value.as_bytes()[0];
        assert!(quote == b'"' || quote == b'\'');
        let end = value.as_bytes()[1..].iter().position(|byte| *byte == quote).expect("closing quote") + 1;
        assert!(attributes.insert(name.to_owned(), entities(&value[1..end])).is_none());
        rest = &value[end + 1..];
    }
    attributes
}
fn entities(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(at) = rest.find('&') {
        output.push_str(&rest[..at]);
        let (entity, tail) = rest[at + 1..].split_once(';').expect("entity terminator");
        output.push(match entity {
            "amp" => '&',
            "quot" => '"',
            "#39" => '\'',
            "lt" => '<',
            "gt" => '>',
            _ => panic!("supported HTML entity"),
        });
        rest = tail;
    }
    output.push_str(rest);
    output
}
fn encoded(input: &[u8]) -> String {
    let mut output = String::new();
    for byte in input {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            output.push(char::from(*byte));
        } else {
            write!(output, "%{byte:02X}").expect("String accepts text");
        }
    }
    output
}
fn unencoded(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut bytes = input.bytes();
    while let Some(byte) = bytes.next() {
        output.push(match byte {
            b'+' => b' ',
            b'%' => {
                let high = char::from(bytes.next().expect("high nibble")).to_digit(16).expect("hex");
                let low = char::from(bytes.next().expect("low nibble")).to_digit(16).expect("hex");
                u8::try_from(high * 16 + low).expect("byte")
            }
            byte => byte,
        });
    }
    output
}
fn query(url: &str) -> BTreeMap<String, Vec<u8>> {
    url.split_once('?')
        .expect("query")
        .1
        .split('&')
        .map(|field| {
            let (name, value) = field.split_once('=').expect("query field");
            (name.to_owned(), unencoded(value))
        })
        .collect()
}
fn request(method: &str, url: &str, body: &[u8]) -> Vec<u8> {
    let (scheme, address) = url.split_once("://").expect("absolute browser URL");
    assert!(scheme == "https" || scheme == "http");
    let slash = address.find('/').expect("path");
    let mut bytes = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n",
        &address[slash..], &address[..slash], body.len()
    ).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn response(bytes: &[u8], status: u16) -> (&str, &str) {
    let text = core::str::from_utf8(bytes).expect("browser response UTF-8");
    let (head, body) = text.split_once("\r\n\r\n").expect("response head");
    let got: u16 = head.split_whitespace().nth(1).expect("status").parse().expect("numeric status");
    assert_eq!(got, status);
    let length: usize = header(head, "Content-Length").expect("length").parse().expect("numeric length");
    assert_eq!(length, body.len());
    (head, body)
}
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
}
#[derive(Debug)]
struct Callback {
    code: Secret,
    state: Box<[u8]>,
}
fn created(world: &mut HostWorld, target: Target) -> Callback {
    world.run_plan(1, Plan::Manifest { forge: FORGE, target });
    world.settle();
    let Some(Heard::Done { outcome: Outcome::Manifest { state, page }, .. }) = world.heard().last() else {
        panic!("manifest page")
    };
    let state = state.clone();
    let html = core::str::from_utf8(page).expect("manifest page UTF-8");
    let form = attributes(html, "form");
    assert_eq!(form["method"], "post");
    let field = attributes(html, "input");
    assert_eq!(field["name"], "manifest");
    assert_eq!(field["type"], "hidden");
    let body = format!("manifest={}", encoded(field["value"].as_bytes()));
    let bytes = world.github_page(&request("POST", &form["action"], body.as_bytes()));
    let (head, html) = response(&bytes, 200);
    assert_eq!(header(head, "Content-Type"), Some("text/html; charset=utf-8"));
    let link = attributes(html, "a");
    assert_eq!(link["id"], "create");
    let given = fixtures::github_given();
    assert!(link["href"].split_once('?').expect("callback").0.as_bytes() == given.redirect_url.as_ref());
    let mut fields = query(&link["href"]);
    assert_eq!(fields.remove("state").expect("callback state"), state.as_ref());
    Callback { code: Secret::new(fields.remove("code").expect("callback code").into_boxed_slice()), state }
}
fn convert(world: &mut HostWorld, run: u64, callback: &Callback) {
    world.run_plan(run, Plan::Convert { forge: FORGE, code: callback.code.clone(), state: callback.state.clone() });
    world.settle();
}
fn stop(run: u64, stage: Stage, why: Why, at: Duration) -> Heard {
    Heard::Stop {
        run: Token::new(run),
        failure: Failure { forge: FORGE, step: stage, subject: b"convert".as_slice().into(), why },
        at,
    }
}
fn done(run: u64) -> Heard {
    Heard::Done { run: Token::new(run), outcome: Outcome::Nothing, at: Duration::ZERO }
}
fn closed(world: &mut HostWorld) {
    world.close();
    world.settle();
    assert!(matches!(world.heard().last(), Some(Heard::Closed { .. })));
    let events = world.heard().len();
    let sends = world.connects().len();
    world.finish();
    assert_eq!(world.heard().len(), events, "one terminal and no expiry wake");
    assert_eq!(world.connects().len(), sends, "ended host is silent");
}
fn kept_manifest(world: &HostWorld, state: &[u8], account: &[u8]) -> Box<[u8]> {
    let manifest = world.github_manifest(state).expect("posted manifest retained");
    let given = fixtures::github_given();
    assert_eq!(manifest.account.as_ref(), account);
    assert_eq!(manifest.url, given.url);
    assert_eq!(manifest.redirect_url, given.redirect_url);
    assert_eq!(manifest.setup_url.as_ref(), Some(&given.setup_url));
    assert!(manifest.public);
    assert!(manifest.setup_on_update);
    let hook = manifest.hook_attributes.as_ref().expect("hook");
    assert!(hook.active);
    let expected = format!("https://{}.invalid/", core::str::from_utf8(&manifest.name).expect("App name"));
    assert_eq!(hook.url.as_deref(), Some(expected.as_bytes()));
    manifest.name.clone()
}
fn identity(world: &HostWorld, slug: &[u8]) {
    for (item, value) in [
        (Item::AppId, APP.to_le_bytes().as_slice()),
        (Item::AppSlug, slug),
        (Item::AppKey, app_key()),
        (Item::User, BOT.to_le_bytes().as_slice()),
    ] {
        assert_eq!(world.file(item), Some(&fixtures::secret(value)));
    }
    assert!(world.file(Item::Accepted).is_none());
    assert!(world.file(Item::WebhookSecret).is_none());
    assert_eq!(world.connects(), &[Duration::ZERO; 3], "conversion, App proof, bot lookup");
    assert!(world.reads().is_empty());
}

#[test]
fn the_host_keeps_conversion_items_before_answering_the_browser() {
    let mut world = empty(&[]);
    let callback = created(&mut world, Target::Personal);
    let slug = kept_manifest(&world, &callback.state, b"alice");
    world.pause_writes();
    convert(&mut world, 2, &callback);
    for (index, (item, value)) in [
        (Item::AppId, APP.to_le_bytes().as_slice()),
        (Item::AppSlug, slug.as_ref()),
        (Item::AppKey, app_key()),
        (Item::User, BOT.to_le_bytes().as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(world.heard().len(), 1, "browser response waits for Done (forge-setup.md, 5.2)");
        assert_eq!(world.needs().len(), index + 1, "one outstanding write");
        assert_eq!(world.needs().last(), Some(&Need::Write { item, bytes: fixtures::secret(value) }));
        assert!(world.file(item).is_none(), "write not yet durable");
        for unwritten in &ITEMS[index..] {
            assert!(world.file(*unwritten).is_none());
        }
        world.write_pending(); // Owner saves the bytes, then sends Written.
        world.settle();
        assert_eq!(world.file(item), Some(&fixtures::secret(value)));
    }
    // This is the owner's simulated browser response, released only after Done (5.2).
    assert_eq!(world.heard().last(), Some(&done(2)));
    identity(&world, &slug);
    assert_eq!(kept_manifest(&world, &callback.state, b"alice"), slug);
    world.run_plan(3, Plan::Install { forge: FORGE });
    world.settle();
    let Some(Heard::Done { outcome: Outcome::Install { state, url }, .. }) = world.heard().last() else {
        panic!("install link")
    };
    let expected = format!(
        "https://github.test/apps/{}/installations/new?state={}",
        core::str::from_utf8(&slug).expect("slug"),
        core::str::from_utf8(state).expect("state")
    );
    assert_eq!(url.as_ref(), expected.as_bytes());
    assert_eq!(world.reads(), &[Item::AppSlug]);
    closed(&mut world);
}

#[test]
fn an_organisation_manifest_returns_through_its_own_state() {
    let mut world = empty(&[]);
    let callback = created(&mut world, Target::Org(b"acme".as_slice().into()));
    let slug = kept_manifest(&world, &callback.state, b"acme");
    convert(&mut world, 2, &callback);
    assert_eq!(world.heard().last(), Some(&done(2)));
    identity(&world, &slug);
    world.run_plan(3, Plan::Install { forge: FORGE });
    world.settle();
    let Some(Heard::Done { outcome: Outcome::Install { state, url }, .. }) = world.heard().last() else {
        panic!("install link")
    };
    let install_state = state.clone();
    // Exercise Location framing on the browser install page too.
    let url = core::str::from_utf8(url).expect("install URL").to_owned();
    let bytes = world.github_page(&request("GET", &url, b""));
    let (head, body) = response(&bytes, 302);
    assert!(body.is_empty());
    assert_eq!(header(head, "Content-Type"), None);
    let location = header(head, "Location").expect("setup redirect");
    assert!(location.split_once('?').expect("setup query").0.as_bytes() == fixtures::github_given().setup_url.as_ref());
    assert_eq!(query(location).get("state").expect("install state"), install_state.as_ref());
    closed(&mut world);
}

#[test]
fn a_reused_or_unknown_conversion_state_never_sends_a_code() {
    let mut world = empty(&[]);
    let callback = created(&mut world, Target::Personal);
    convert(&mut world, 2, &callback);
    assert_eq!(world.heard().last(), Some(&done(2)));
    for state in [callback.state.clone(), b"00000000000000000000000000000000".as_slice().into()] {
        let sends = world.connects().len();
        let writes = world.needs().len();
        convert(&mut world, 3, &Callback { code: callback.code.clone(), state });
        assert_eq!(world.heard().last(), Some(&stop(3, Stage::State, Why::Unknown, Duration::ZERO)));
        assert_eq!(world.connects().len(), sends);
        assert_eq!(world.needs().len(), writes);
    }
    closed(&mut world);
}

#[test]
fn an_expired_or_restarted_conversion_state_sends_nothing() {
    let mut previous = empty(&[]);
    let callback = created(&mut previous, Target::Personal);
    closed(&mut previous);
    let mut restarted = empty(&[]);
    // A new Host keeps acknowledged files alone; no state is persisted (5.7).
    for item in ITEMS.into_iter().chain([Item::Accepted, Item::AdminToken]) {
        if let Some(bytes) = previous.file(item) {
            restarted.put(item, bytes.clone());
        }
    }
    let mut expired = empty(&[]);
    let expiry_callback = created(&mut expired, Target::Personal);
    let life = fixtures::github_given().limits.state_life;
    expired.advance_to(Time::from_nanos(life.as_nanos()));
    for (mut world, callback, at) in [(expired, expiry_callback, life), (restarted, callback, Duration::ZERO)] {
        convert(&mut world, 2, &callback);
        assert_eq!(world.heard().last(), Some(&stop(2, Stage::State, Why::Unknown, at)));
        assert!(world.connects().is_empty());
        assert!(world.needs().is_empty());
        closed(&mut world);
    }
}

#[test]
fn a_conversion_refused_by_github_still_consumes_its_state() {
    let mut world = empty(&[]);
    let mut callback = created(&mut world, Target::Personal);
    callback.code = fixtures::secret(b"invalid-conversion-code");
    convert(&mut world, 2, &callback);
    assert_eq!(world.heard().last(), Some(&stop(2, Stage::App, Why::Refused(404), Duration::ZERO)));
    convert(&mut world, 3, &callback);
    assert_eq!(world.heard().last(), Some(&stop(3, Stage::State, Why::Unknown, Duration::ZERO)));
    assert_eq!(world.connects(), &[Duration::ZERO]);
    assert!(world.needs().is_empty());
    closed(&mut world);
}

#[test]
fn a_conversion_deadline_cannot_restore_its_state() {
    let mut world = empty(&[Connect::Silent]);
    let callback = created(&mut world, Target::Personal);
    convert(&mut world, 2, &callback);
    world.finish();
    let deadline = fixtures::github_given().limits.deadline;
    assert_eq!(world.heard().last(), Some(&stop(2, Stage::App, Why::Down, deadline)));
    convert(&mut world, 3, &callback);
    assert_eq!(world.heard().last(), Some(&stop(3, Stage::State, Why::Unknown, deadline)));
    assert_eq!(world.connects(), &[Duration::ZERO]);
    assert!(world.needs().is_empty());
    closed(&mut world);
}

#[test]
fn a_lost_conversion_reply_reports_the_code_as_spent() {
    let mut world = empty(&[Connect::LostGitHubReply]);
    let callback = created(&mut world, Target::Personal);
    convert(&mut world, 2, &callback);
    world.finish();
    let backoff = fixtures::github_given().limits.backoff;
    assert_eq!(world.heard().last(), Some(&stop(2, Stage::App, Why::Refused(404), backoff)));
    assert_eq!(world.connects(), &[Duration::ZERO, backoff]);
    assert!(world.needs().is_empty());
    convert(&mut world, 3, &callback);
    assert_eq!(world.heard().last(), Some(&stop(3, Stage::State, Why::Unknown, backoff)));
    assert_eq!(world.connects().len(), 2);
    closed(&mut world);
}

#[test]
fn conversion_trace_and_failures_hold_no_secret() {
    let mut world = empty(&[]);
    let callback = created(&mut world, Target::Personal);
    let plan = Plan::Convert { forge: FORGE, code: callback.code.clone(), state: callback.state.clone() };
    let mut diagnostics = format!("{plan:?} {callback:?}");
    convert(&mut world, 2, &callback);
    assert_eq!(world.heard().last(), Some(&done(2)));
    convert(&mut world, 3, &callback);
    // Manifest's page/state is public; the conversion's trace and failure are not secrets.
    write!(diagnostics, "{:?} {:?}", &world.heard()[1..], world.needs()).expect("String accepts text");
    // The fake draws twenty hex code bytes, then thirty-two webhook bytes, from its seed.
    let mut rng = Rng::new(SEED);
    let hex: Vec<u8> = (0..52).map(|_| b"0123456789abcdef"[usize::try_from(rng.below(16)).expect("nibble")]).collect();
    assert!(hex[..20] == *callback.code.expose(), "fixture code draw");
    let webhook = &hex[20..];
    let pem = keys::pem(app_key()).expect("fixture PEM");
    for secret in [app_key(), pem.as_ref(), callback.code.expose(), webhook] {
        assert!(!diagnostics.as_bytes().windows(secret.len()).any(|bytes| bytes == secret));
        for spelling in [format!("{secret:?}"), format!("{secret:x?}"), format!("{secret:X?}")] {
            assert!(!diagnostics.contains(&spelling));
        }
    }
    for item in
        [Item::AdminToken, Item::AppId, Item::AppSlug, Item::User, Item::Accepted, Item::WebhookSecret, Item::Pasted]
    {
        if let Some(value) = world.file(item) {
            for secret in [app_key(), pem.as_ref(), callback.code.expose(), webhook] {
                assert!(!value.expose().windows(secret.len()).any(|bytes| bytes == secret));
            }
        }
    }
    assert_eq!(world.file(Item::AppKey), Some(&fixtures::secret(app_key())));
    closed(&mut world);
}
