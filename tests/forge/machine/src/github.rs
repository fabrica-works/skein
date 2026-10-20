//! The fake GitHub layer, framed as HTTP for the machine world's peer.
use skein_fake_forge_github::{
    app::App,
    installations::{self, Installations, Kind, Reach},
    keys::KEYS,
    pages,
    service::{Account, AppFacts, Config, Limits, Outcome, Request, Row, Service},
};
use skein_forge::github::{self, request::Method};
use skein_json::{tokenizer, writer};
use skein_lib::{List, ReplyTo, Secret, Token, Wall};

/// A wall past the fake's installation dates, advanced by the machine's clock.
pub const WALL_BASE: Wall = Wall::from_nanos(1_000_000_000_000_000);

/// The GitHub service's state behind one machine's scripted peer.
pub struct Fake {
    service: Service,
}
impl Fake {
    /// Uses the GitHub fake's fixture App, accounts, repositories and installations.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut repositories = List::with_capacity(3);
        for (id, name, organization, private) in
            [(4, b"tools".as_slice(), false, false), (2, b"widgets", true, true), (6, b"absent", true, false)]
        {
            repositories
                .push(Row {
                    id,
                    owner: Box::from(b"acme".as_slice()),
                    owner_id: 22,
                    organization,
                    name: Box::from(name),
                    private,
                })
                .expect("fixture rows");
        }
        let mut accounts = List::with_capacity(2);
        for (login, id, organization) in [(b"acme".as_slice(), 22, true), (b"alice".as_slice(), 31, false)] {
            accounts.push(Account { login: Box::from(login), id, organization }).expect("fixture accounts");
        }
        let config = Config {
            app: App {
                id: 7,
                slug: Box::from(b"test-app".as_slice()),
                owner: 1,
                permissions: Box::from([]),
                events: Box::from([]),
                key: app_key(),
                webhook: Secret::new(Box::from(b"test-webhook".as_slice())),
            },
            app_facts: AppFacts::Complete,
            bot: 9,
            hook: Box::from(b"https://hook.test/start".as_slice()),
            repositories,
            accounts,
            person: Box::from(b"alice".as_slice()),
        };
        let limits = Limits {
            github: github::limits::Limits {
                json: tokenizer::Limits { depth: 10, string: 4096, number: 32, chunk: 128, length: 8192 },
                writer: writer::Limits { depth: 10, length: 8192 },
                string_bytes: 256,
                body_bytes: 4096,
                inner: 8,
                target_bytes: 1024,
            },
            installations: installations::Limits { installations: 3, tokens: 4, listed: 2 },
            pages: pages::Limits { states: 3, manifest_bytes: 2048 },
            page: 2,
            body: 4096,
            response: 8192,
        };
        let mut service = Service::new(config, &limits, seed).expect("valid fixture config");
        let mut listed = List::with_capacity(2);
        for id in [2, 4] {
            listed.push(id).expect("fixture reach");
        }
        let store = service.installations_mut();
        assert_eq!(
            store
                .install(22, b"acme", Kind::Organization, Reach::Listed(listed), Wall::from_nanos(1_000_000_000_000))
                .expect("fixture installation"),
            1
        );
        assert_eq!(
            store
                .install(23, b"example", Kind::User, Reach::All, Wall::from_nanos(1_001_000_000_000))
                .expect("fixture installation"),
            2
        );
        assert_eq!(
            store
                .install(24, b"other", Kind::User, Reach::All, Wall::from_nanos(1_002_000_000_000))
                .expect("fixture installation"),
            3
        );
        assert!(store.suspend(3, true), "third installation exists");
        Self { service }
    }

    /// Scenario access to the fake's installation store.
    pub fn installations_mut(&mut self) -> &mut Installations {
        self.service.installations_mut()
    }

    /// Answers one complete HTTP/1.1 request, keeping the connection open.
    pub fn serve(&mut self, request: &[u8], wall: Wall) -> Vec<u8> {
        let end = request.windows(4).position(|bytes| bytes == b"\r\n\r\n").expect("complete request head");
        let mut lines =
            request[..end].split(|byte| *byte == b'\n').map(|line| line.strip_suffix(b"\r").unwrap_or(line));
        let mut line = lines.next().expect("request line").split(|byte| *byte == b' ');
        let method = match line.next().expect("request method") {
            b"GET" => Some(Method::Get),
            b"POST" => Some(Method::Post),
            b"PUT" => Some(Method::Put),
            b"PATCH" => Some(Method::Patch),
            b"DELETE" => Some(Method::Delete),
            _ => None,
        }
        .expect("supported request method");
        let target = line.next().expect("request target");
        assert_eq!(line.next(), Some(b"HTTP/1.1".as_slice()), "HTTP version");
        assert!(line.next().is_none(), "complete request line");
        let mut authorization = None;
        let mut version = None;
        let mut agent = false;
        for line in lines {
            let colon = line.iter().position(|byte| *byte == b':').expect("header colon");
            let name = &line[..colon];
            let value = line[colon + 1..].trim_ascii();
            if name.eq_ignore_ascii_case(b"Authorization") {
                authorization = Some(value);
            } else if name.eq_ignore_ascii_case(b"X-GitHub-Api-Version") {
                version = Some(value);
            } else if name.eq_ignore_ascii_case(b"User-Agent") {
                agent = true;
            }
        }
        let response = match self.service.request(
            &Request { method, target, authorization, version, agent, body: &request[end + 4..] },
            wall,
            ReplyTo::new(Token::new(77)),
        ) {
            Outcome::Reply(response) => response,
            Outcome::Call(..) => panic!("domain reads arrive with task 27"),
        };
        let status = response.status;
        let reason = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            413 => "Content Too Large",
            422 => "Unprocessable Content",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "Status",
        };
        let mut bytes = format!(
            "HTTP/1.1 {status} {reason}\r\nDate: {}\r\nContent-Type: application/json; charset=utf-8\r\n",
            imf(wall)
        )
        .into_bytes();
        if let Some(link) = response.link {
            bytes.extend_from_slice(b"Link: ");
            bytes.extend_from_slice(&link);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(format!("Content-Length: {}\r\n\r\n", response.body.len()).as_bytes());
        bytes.extend_from_slice(&response.body);
        bytes
    }
}

/// The fake App's public test key.
#[must_use]
pub fn app_key() -> &'static [u8] {
    KEYS.first().expect("first test key")
}

/// Signs an App JWT and prefixes its whole Authorization value.
#[must_use]
pub fn bearer(key: &[u8], app: u64, issued: Wall, expires: Wall) -> Vec<u8> {
    let input = github::jwt::signing_input(app, issued, expires).expect("fixture JWT window");
    let signature = skein_tls::sign_rs256(key, &input).expect("test key signs");
    let token = github::jwt::token(&input, &signature).expect("fixture JWT");
    let mut header = b"Bearer ".to_vec();
    header.extend_from_slice(token.expose());
    header
}

// Hinnant's civil_from_days, with March starting each Gregorian cycle.
fn imf(wall: Wall) -> String {
    let seconds = wall.as_secs();
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let shifted = days + 719_468;
    let cycle = shifted / 146_097;
    let of_cycle = shifted % 146_097;
    let year_of_cycle = (of_cycle - of_cycle / 1_460 + of_cycle / 36_524 - of_cycle / 146_096) / 365;
    let day_of_year = of_cycle - (year_of_cycle * 365 + year_of_cycle / 4 - year_of_cycle / 100);
    let month = (day_of_year * 5 + 2) / 153;
    let day = day_of_year - (month * 153 + 2) / 5 + 1;
    let year = cycle * 400 + year_of_cycle + u64::from(month >= 10);
    let weekday =
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][usize::try_from((days + 4) % 7).expect("weekday index")];
    let month = ["Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec", "Jan", "Feb"]
        [usize::try_from(month).expect("month index")];
    format!("{weekday}, {day:02} {month} {year:04} {:02}:{:02}:{:02} GMT", time / 3600, time / 60 % 60, time % 60)
}
