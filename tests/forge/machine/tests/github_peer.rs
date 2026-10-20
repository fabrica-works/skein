use skein_fake_forge_github::keys::KEYS;
use skein_forge::github::pin::API_VERSION;
use skein_forge_connection::{StartRead, Stopped};
use skein_forge_machine_world::{
    fixtures,
    github::{Fake, WALL_BASE, app_key, bearer},
    machine::{Answer, Machine},
};
use skein_lib::{Duration, Time, Wall};

const SECOND: Duration = Duration::from_secs(1);
const INSTALLATIONS: &str = "/app/installations?per_page=100&page=1";

fn wall(seconds: u64) -> Wall {
    Wall::from_nanos(seconds * 1_000_000_000)
}
fn jwt() -> Vec<u8> {
    bearer(app_key(), 7, wall(1140), wall(1740))
}
fn request(
    method: &str,
    target: &str,
    authorization: Option<&[u8]>,
    agent: bool,
    version: bool,
    body: &[u8],
) -> Vec<u8> {
    let mut bytes = format!("{method} {target} HTTP/1.1\r\n").into_bytes();
    if agent {
        bytes.extend_from_slice(b"User-Agent: kit/0.0.0\r\n");
    }
    if version {
        bytes.extend_from_slice(format!("X-GitHub-Api-Version: {API_VERSION}\r\n").as_bytes());
    }
    if let Some(authorization) = authorization {
        bytes.extend_from_slice(b"Authorization: ");
        bytes.extend_from_slice(authorization);
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    bytes.extend_from_slice(body);
    bytes
}
// Inspect framing without putting a response's token in an assertion's output.
fn response<'a>(bytes: &'a [u8], status: &str) -> (&'a str, &'a str) {
    let text = std::str::from_utf8(bytes).expect("JSON response text");
    let (head, body) = text.split_once("\r\n\r\n").expect("complete response head");
    assert!(head.starts_with(status), "response status");
    assert!(head.contains("\r\nContent-Type: application/json; charset=utf-8"));
    assert!(!head.contains("\r\nConnection: close"));
    let length = head.lines().find_map(|line| line.strip_prefix("Content-Length: ")).expect("response length");
    assert_eq!(length.parse::<usize>().expect("length number"), body.len());
    (head, body)
}

#[test]
fn an_app_jwt_lists_installations_paged_with_a_link() {
    let mut fake = Fake::new(5);
    let jwt = jwt();
    let first = fake.serve(&request("GET", INSTALLATIONS, Some(&jwt), true, true, b""), wall(1200));
    let (head, body) = response(&first, "HTTP/1.1 200 OK\r\n");
    assert!(body.contains("\"id\":1,\"account\":"));
    assert!(body.contains("\"id\":2,\"account\":"));
    assert!(!body.contains("\"id\":3,\"account\":"));
    assert!(head.contains("\r\nLink: <https://api.github.com/app/installations?per_page=100&page=2>; rel=\"next\""));
    let second =
        fake.serve(&request("GET", "/app/installations?per_page=100&page=2", Some(&jwt), true, true, b""), wall(1200));
    let (head, body) = response(&second, "HTTP/1.1 200 OK\r\n");
    assert!(body.contains("\"id\":3,\"account\":"));
    assert!(!body.contains("\"id\":1,\"account\":"));
    assert!(!body.contains("\"id\":2,\"account\":"));
    assert!(!head.contains("\r\nLink:"));
}

#[test]
fn a_jwt_signed_by_another_key_is_401() {
    let mut fake = Fake::new(5);
    let jwt = bearer(KEYS.get(1).expect("second test key"), 7, wall(1140), wall(1740));
    let bytes = fake.serve(&request("GET", INSTALLATIONS, Some(&jwt), true, true, b""), wall(1200));
    let (_, body) = response(&bytes, "HTTP/1.1 401 Unauthorized\r\n");
    assert!(body.contains("Bad credentials"));
}

#[test]
fn a_request_without_a_user_agent_is_403() {
    let mut fake = Fake::new(5);
    let bytes = fake.serve(&request("GET", INSTALLATIONS, Some(&jwt()), false, true, b""), wall(1200));
    let (_, body) = response(&bytes, "HTTP/1.1 403 Forbidden\r\n");
    assert!(body.contains("User agent required"));
}

#[test]
fn a_request_without_the_api_version_is_400() {
    let mut fake = Fake::new(5);
    let bytes = fake.serve(&request("GET", INSTALLATIONS, Some(&jwt()), true, false, b""), wall(1200));
    let (_, body) = response(&bytes, "HTTP/1.1 400 Bad Request\r\n");
    assert!(body.contains("Invalid API version"));
}

#[test]
fn the_date_header_is_the_wall_in_imf_fixdate() {
    let mut fake = Fake::new(5);
    for (seconds, date) in
        [(1_000_000, "Date: Mon, 12 Jan 1970 13:46:40 GMT"), (1_577_836_800, "Date: Wed, 01 Jan 2020 00:00:00 GMT")]
    {
        let bytes = fake.serve(&request("GET", INSTALLATIONS, None, true, true, b""), wall(seconds));
        let (head, _) = response(&bytes, "HTTP/1.1 401 Unauthorized\r\n");
        assert!(head.lines().any(|line| line == date));
    }
}

#[test]
fn the_viewer_answers_the_bot_on_an_installation_token() {
    let mut fake = Fake::new(5);
    let bytes =
        fake.serve(&request("POST", "/app/installations/1/access_tokens", Some(&jwt()), true, true, b""), wall(1200));
    let (_, body) = response(&bytes, "HTTP/1.1 201 Created\r\n");
    assert!(body.contains("\"token\":\"ghs_"));
    let token = body.split_once("\"token\":\"").expect("minted token field").1.split('"').next().expect("minted token");
    let authorization = format!("Bearer {token}");
    let bytes = fake.serve(
        &request(
            "POST",
            "/graphql",
            Some(authorization.as_bytes()),
            true,
            true,
            br#"{"query":"query { viewer { login databaseId } }"}"#,
        ),
        wall(1200),
    );
    let (_, body) = response(&bytes, "HTTP/1.1 200 OK\r\n");
    assert!(body.contains("\"databaseId\":9"));
}

fn github_machine() -> Machine {
    Machine::github(1, fixtures::limits(), fixtures::settings(), vec![Answer::served("GET /api/v1/user")])
}

#[test]
fn a_scripted_served_answer_reaches_the_component_through_the_wire() {
    let mut world = github_machine();
    world.run_for(SECOND);
    assert_eq!(world.stopped(), Some((fixtures::FORGE, Stopped::Refused { read: StartRead::User, status: 400 })));
    assert!(world.script_done());
    assert_eq!(world.requests().len(), 1);
    assert!(world.close());
}

#[test]
fn the_wall_follows_the_clock_in_github_mode() {
    let mut world = github_machine();
    assert_eq!(world.wall(), WALL_BASE);
    world.run_for(SECOND);
    assert_eq!(world.wall(), Wall::from_nanos(WALL_BASE.as_nanos() + SECOND.as_nanos()));
    world.advance_to(Time::ZERO.saturating_add(Duration::from_secs(5)));
    assert_eq!(world.wall(), Wall::from_nanos(WALL_BASE.as_nanos() + Duration::from_secs(5).as_nanos()));
    assert!(world.close());
}

#[test]
fn a_forgejo_machine_keeps_the_epoch_wall() {
    let mut world = Machine::ready(1, vec![]);
    assert_eq!(world.wall(), Wall::EPOCH);
    world.run_for(SECOND);
    assert_eq!(world.wall(), Wall::EPOCH);
    world.advance_to(Time::ZERO.saturating_add(Duration::from_secs(5)));
    assert_eq!(world.wall(), Wall::EPOCH);
    assert!(world.close());
}
