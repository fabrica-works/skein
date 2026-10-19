//! Credential schemes follow their route's fields (llm.md, section 2.6).
use skein_http::Header;
use skein_llm::{Call, Cap, Credential, Endpoint, Error, Provider, Scheme, client};
use skein_llm_world::{World, call, fields_holding, limits, response};

fn input(provider: Provider, scheme: Scheme) -> Call {
    let mut input = call(1);
    input.endpoint = match provider {
        Provider::OpenAiCodex => Endpoint::codex(),
        Provider::Anthropic => Endpoint::anthropic(),
    };
    input.credential.scheme = scheme;
    if provider == Provider::Anthropic {
        input.credential.account_id = Box::new([]);
    }
    input.prompt.affinity = None;
    input
}
fn header(name: &[u8], value: &[u8]) -> Header {
    Header { name: name.to_vec().into(), value: value.to_vec().into() }
}
fn rejection(input: Call, bounds: &client::Limits) -> Option<Error> {
    client::Client::prepare(input, bounds).err()
}
fn head_fields(sent: &[u8]) -> Vec<(&str, &str)> {
    std::str::from_utf8(sent)
        .expect("valid request fixture")
        .split("\r\n\r\n")
        .next()
        .expect("valid request fixture")
        .lines()
        .filter_map(|line| line.split_once(": "))
        .filter(|(name, _)| ["anthropic-version", "anthropic-beta", "chatgpt-account-id"].contains(name))
        .collect()
}
type RouteCase = (Provider, Scheme, Option<&'static str>, Vec<&'static str>, Vec<(&'static str, &'static str)>);
fn route_cases() -> [RouteCase; 4] {
    [
        (Provider::Anthropic, Scheme::Key, None, vec!["x-api-key"], vec![("anthropic-version", "2023-06-01")]),
        (
            Provider::Anthropic,
            Scheme::Key,
            Some("fixture-beta"),
            vec!["x-api-key"],
            vec![("anthropic-version", "2023-06-01"), ("anthropic-beta", "fixture-beta")],
        ),
        (
            Provider::Anthropic,
            Scheme::Bearer,
            None,
            vec!["Authorization"],
            vec![("anthropic-version", "2023-06-01"), ("anthropic-beta", "oauth-2025-04-20")],
        ),
        (
            Provider::OpenAiCodex,
            Scheme::Bearer,
            None,
            vec!["Authorization"],
            vec![("chatgpt-account-id", "account-test")],
        ),
    ]
}
#[test]
fn each_scheme_goes_where_its_route_takes_it() {
    for (provider, scheme, beta, names, fields) in route_cases() {
        let mut input = input(provider, scheme);
        if let Some(beta) = beta {
            input.endpoint.headers = Box::new([header(b"anthropic-beta", beta.as_bytes())]);
        }
        let value = input.credential.value.clone();
        let mut world = World::new(input, limits(), response(401, "", b"{}", false), 1);
        world.request(client::Request::Start);
        world.run();
        assert_eq!(fields_holding(&world.sent, &value), names, "{provider:?} {scheme:?} {beta:?}");
        assert_eq!(head_fields(&world.sent), fields, "{provider:?} {scheme:?} {beta:?}");
    }
}
#[test]
fn a_credential_its_route_refuses_is_invalid() {
    for (provider, credential) in [
        (Provider::OpenAiCodex, Credential::key(call(1).credential.value)),
        (Provider::OpenAiCodex, Credential::bearer(call(1).credential.value)),
        (Provider::Anthropic, call(1).credential),
    ] {
        let mut input = input(provider, Scheme::Bearer);
        input.credential = credential;
        assert_eq!(rejection(input, &limits()), Some(Error::Invalid), "{provider:?}");
    }
}
#[test]
fn an_endpoint_header_naming_a_credential_field_is_refused() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        for name in [b"Authorization".as_slice(), b"x-api-key", b"chatgpt-account-id"] {
            let mut input = input(provider, Scheme::Bearer);
            input.endpoint.headers = Box::new([header(name, b"override")]);
            assert_eq!(rejection(input, &limits()), Some(Error::Invalid), "{provider:?} {name:?}");
        }
    }
}
#[test]
fn a_key_head_at_its_bound_is_admitted() {
    // llm.md 4.7: six fixed fields with a key, including Content-Length.
    const KEY_FIELDS: u32 = 6;
    let mut world = World::new(input(Provider::Anthropic, Scheme::Key), limits(), response(401, "", b"{}", false), 1);
    world.request(client::Request::Start);
    world.run();
    let measured =
        u32::try_from(world.sent.windows(4).position(|bytes| bytes == b"\r\n\r\n").expect("valid request fixture") + 4)
            .expect("valid request fixture");
    for (head, fields, expected) in [
        (measured, KEY_FIELDS, None),
        (measured - 1, KEY_FIELDS, Some(Error::Limit { which: Cap::RequestHead, bound: u64::from(measured - 1) })),
        (measured, KEY_FIELDS - 1, Some(Error::Limit { which: Cap::ResponseFields, bound: u64::from(KEY_FIELDS - 1) })),
    ] {
        let mut bounds = limits();
        bounds.http.request = head;
        bounds.http.headers = fields;
        assert_eq!(rejection(input(Provider::Anthropic, Scheme::Key), &bounds), expected, "{head} {fields}");
    }
}
