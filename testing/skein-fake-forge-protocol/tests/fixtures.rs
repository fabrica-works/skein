use crate::{Config, Faults, Limits, Repository};
use alloc::boxed::Box;
use skein_forge::forgejo::request::Repo;
use skein_lib::{List, Secret, bytes};
pub const FIXTURE_SECRET: &[u8] = b"fixture-hook-secret";
pub fn hooked_config() -> Config {
    let mut config = config();
    config.hooks = Some(crate::delivery::Endpoint {
        forge: 1,
        host: boxed(b"127.0.0.1:7200"),
        secret: Secret::new(boxed(FIXTURE_SECRET)),
    });
    config
}
pub const TOKEN: &[u8] = b"1111111111111111111111111111111111111111";
pub fn config() -> Config {
    let mut tokens = List::with_capacity(1);
    tokens.push((Secret::new(bytes::copy_of(TOKEN)), 1)).expect("one token");
    let mut users = List::with_capacity(2);
    users.push((1, bytes::copy_of(b"engine"))).expect("two users");
    users.push((2, bytes::copy_of(b"person"))).expect("two users");
    let mut repositories = List::with_capacity(1);
    repositories
        .push(Repository { id: 2, full: bytes::copy_of(b"org/repo"), protected: None })
        .expect("one repository");
    Config {
        base: bytes::copy_of(b"http://127.0.0.1:3000"),
        tokens,
        users,
        repositories,
        max_response_items: 2,
        default_paging_num: 30,
        hooks: None,
        faults: Faults { reset: 0, drop: 0, server_error: 0 },
    }
}
pub fn at() -> Repo {
    Repo { owner: bytes::copy_of(b"org"), name: bytes::copy_of(b"repo") }
}
pub fn limits() -> Limits {
    Limits {
        calls: 4,
        connections: 4,
        raw_bytes: 4096,
        http: skein_http::server::Limits {
            head: 4096,
            headers: 16,
            body: 16384,
            read: 1024,
            response: 4096,
            send: 1024,
        },
        documents: skein_forge::forgejo::limits::Limits {
            json: skein_json::tokenizer::Limits { chunk: 1024, string: 16384, number: 32, depth: 16, length: 65536 },
            writer: skein_json::writer::Limits { depth: 16, length: 65536 },
            string_bytes: 512,
            body_bytes: 16384,
            inner: 32,
            target_bytes: 4096,
        },
    }
}
pub fn boxed(text: &[u8]) -> Box<[u8]> {
    bytes::copy_of(text)
}

use crate::{Event, Server, Service};
use skein_fake_forge_domain::{self as fake, api as a};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, Env, Intake, Queue, ReplyTo, Time, Token, Wall, Writer};
pub struct Harness {
    pub service: Service,
    pub domain: fake::Domain,
    pub fake_env: Env<fake::Config>,
    pub env: Env<Limits>,
    pub server: Server,
    pub above: Queue<Event>,
    pub below: Queue<Down>,
    pub domain_out: Queue<fake::Request>,
    pub input: Intake,
    pub demand: Option<(Read, u32)>,
    pub output: List<u8>,
    pub calls: u32,
    pub closed: bool,
    pub deliveries: List<crate::delivery::Post>,
}
impl Harness {
    pub fn new(config: Config) -> Self {
        Self::with(config, false)
    }
    pub fn hooked() -> Self {
        Self::with(hooked_config(), true)
    }
    fn with(config: Config, hooked: bool) -> Self {
        let limits = limits();
        let fake_config = fake_config();
        let mut domain = fake::Domain::new(&fake_config, 3);
        fake::repository(
            &mut domain,
            &fake_config,
            a::Setup {
                id: 2,
                name: boxed(b"org/repo"),
                default: boxed(b"main"),
                tree: Box::from([]),
                labels: Box::from([]),
                checks: a::Checks {
                    contexts: Box::new([boxed(b"build")]),
                    latency_min: Duration::ZERO,
                    latency_max: Duration::ZERO,
                    silent: 0,
                    passes: 0,
                    reruns: 0,
                    cue: None,
                },
                protection: None,
                hooked,
            },
        );
        fake::grant(&mut domain, b"org/repo", 1, a::Permission::Admin);
        fake::grant(&mut domain, b"org/repo", 2, a::Permission::Write);
        let mut h = Self {
            service: Service::new(config, &limits, 3).expect("config"),
            server: Server::new(Token::new(1), &limits).expect("server"),
            domain,
            fake_env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: fake_config },
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            above: Queue::with_capacity(64),
            below: Queue::with_capacity(64),
            domain_out: Queue::with_capacity(fake::MAX_OUT),
            input: Intake::with_capacity(65536),
            demand: None,
            output: List::with_capacity(65536),
            calls: 0,
            closed: false,
            deliveries: List::with_capacity(32),
        };
        crate::start(&mut h.server, &mut h.service, &h.env, &mut h.above, &mut h.below);
        h
    }
    pub fn request(&mut self, bytes: &[u8]) {
        self.output.clear();
        self.input.append(bytes).expect("request fits");
    }
    pub fn run(&mut self) {
        for _ in 0..2048u32 {
            self.fake_env.now = self.fake_env.now.saturating_add(Duration::from_millis(1));
            self.env.now = fake::time(&self.fake_env.limits, self.fake_env.now);
            self.env.wall = Wall::from_nanos(self.env.now.as_nanos());
            crate::resume(&mut self.server, &mut self.service, &self.env, &mut self.above, &mut self.below);
            while let Some(down) = self.below.pop() {
                match down {
                    Down::Demand { read: Read::Nothing, room: 0 } => {
                        self.demand = None;
                    }
                    Down::Demand { read, room } => {
                        assert!(self.demand.replace((read, room)).is_none(), "one demand");
                    }
                    Down::Send(bytes) => {
                        for byte in bytes {
                            self.output.push(byte).expect("response fits");
                        }
                    }
                    Down::Finish => {}
                }
            }
            if let Some((read, room)) = self.demand {
                let up = if let Some(bytes) = self.input.meet(read) {
                    Some(Up::Bytes(bytes))
                } else if room > 0 {
                    Some(Up::Room)
                } else {
                    None
                };
                if let Some(up) = up {
                    self.demand = None;
                    crate::up(&mut self.server, &mut self.service, &self.env, up, &mut self.above, &mut self.below);
                }
            }
            while let Some(event) = self.above.pop() {
                match event {
                    Event::Domain(event) => {
                        self.calls = self.calls.saturating_add(1);
                        fake::step(&mut self.domain, &self.fake_env, event, &mut self.domain_out);
                    }
                    Event::Close => {
                        self.closed = true;
                        crate::closed(&mut self.server, &mut self.service, &self.env, &mut self.above, &mut self.below);
                    }
                    Event::Closed => {}
                }
            }
            fake::fire(&mut self.domain, &self.fake_env, &mut self.domain_out);
            while let Some(reply) = self.domain_out.pop() {
                if let Some(post) = self.service.delivery(&reply, &self.env.limits) {
                    self.deliveries.push(post).expect("fixture delivery capacity");
                }
                if let Ok((_, reply)) = self.service.target(reply) {
                    crate::down(
                        &mut self.server,
                        &mut self.service,
                        &self.env,
                        reply,
                        &mut self.above,
                        &mut self.below,
                    );
                }
            }
            self.service.reclaim();
            self.domain.reclaim();
            if !self.server.has_work()
                && self.above.is_empty()
                && self.below.is_empty()
                && self.input.is_empty()
                && self.domain.next_deadline().is_none()
            {
                break;
            }
        }
    }
    pub fn service(&self) -> &Service {
        &self.service
    }
    pub fn domain_calls(&self) -> u32 {
        self.calls
    }
    pub fn status(&self) -> u16 {
        let bytes = self.output.as_slice();
        let digits = &bytes[9..12];
        skein_forge::forgejo::id::unsigned(digits).expect("status").try_into().expect("u16")
    }
    pub fn head(&self) -> Box<[u8]> {
        let out = self.output.as_slice();
        boxed(&out[..bytes::find(out, b"\r\n\r\n").expect("head").saturating_add(4)])
    }
    pub fn body(&self) -> Box<[u8]> {
        let out = self.output.as_slice();
        boxed(&out[bytes::find(out, b"\r\n\r\n").expect("head").saturating_add(4)..])
    }
    pub fn call(&mut self, user: u64, op: a::Op) -> Result<a::Answer, a::Error> {
        fake::step(
            &mut self.domain,
            &self.fake_env,
            fake::Event::Call { reply_to: ReplyTo::new(Token::new(999)), user, repository: boxed(b"org/repo"), op },
            &mut self.domain_out,
        );
        for _ in 0..1000u32 {
            self.fake_env.now = self.fake_env.now.saturating_add(Duration::from_millis(1));
            self.env.now = fake::time(&self.fake_env.limits, self.fake_env.now);
            self.env.wall = Wall::from_nanos(self.env.now.as_nanos());
            fake::fire(&mut self.domain, &self.fake_env, &mut self.domain_out);
            while let Some(request) = self.domain_out.pop() {
                if let Some(post) = self.service.delivery(&request, &self.env.limits) {
                    self.deliveries.push(post).expect("fixture delivery capacity");
                }
                if let fake::Request::Reply { to, result } = request {
                    assert_eq!(to.into_token(), Token::new(999));
                    self.domain.reclaim();
                    return result;
                }
            }
        }
        panic!("direct reply")
    }
    pub fn ask(&mut self, method: &[u8], path: &[u8], body: &[u8]) {
        self.ask_as(method, path, body, TOKEN);
    }
    pub fn ask_as(&mut self, method: &[u8], path: &[u8], body: &[u8], token: &[u8]) {
        let digits = skein_lib::Decimal::of(body.len().try_into().expect("length"));
        let parts = [
            method,
            b" ",
            path,
            b" HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nAuthorization: token ",
            token,
            b"\r\nContent-Length: ",
            digits.as_bytes(),
            b"\r\n\r\n",
            body,
        ];
        let mut length = 0usize;
        for part in parts {
            length = length.checked_add(part.len()).expect("request length");
        }
        let mut out = Writer::new(length);
        for part in parts {
            out.put(part).expect("request");
        }
        self.request(&out.finish());
        self.run();
    }
    pub fn issue(&mut self, title: &[u8]) -> u64 {
        let a::Answer::Created(number) = self
            .call(
                2,
                a::Op::Write(a::Write::CreateIssue {
                    title: boxed(title),
                    body: boxed(b"body"),
                    labels: Box::from([]),
                }),
            )
            .expect("issue")
        else {
            panic!("created")
        };
        number
    }
    pub fn branch(&mut self, name: &[u8], at: u64) {
        self.call(2, a::Op::Write(a::Write::CreateBranch { branch: boxed(name), commit: at })).expect("branch");
    }
    pub fn push(&mut self, branch: &[u8]) -> u64 {
        let a::Answer::Commit(parent) =
            self.call(2, a::Op::Read(a::Read::Branch { branch: boxed(b"main") })).expect("tip")
        else {
            panic!("tip")
        };
        let commit = fake::commit(
            &mut self.domain,
            &self.fake_env.limits,
            parent,
            Box::new([a::File { path: boxed(b"file"), content: boxed(b"content") }]),
            b"message",
        )
        .expect("commit")
        .expect("changed");
        self.call(2, a::Op::Git(a::Git::Push { branch: boxed(branch), commit, expected: None })).expect("push");
        commit
    }
}

/// Reads the event line that the renderer places in each request.
pub fn delivery_event(post: &crate::delivery::Post) -> &[u8] {
    let start = bytes::find(&post.head, b"X-Forgejo-Event: ").expect("event header").saturating_add(17);
    let value = &post.head[start..];
    &value[..bytes::find(value, b"\r\n").expect("event line")]
}
impl Harness {
    pub fn delivered(&self, event: &[u8]) -> &crate::delivery::Post {
        for post in &self.deliveries {
            if delivery_event(post) == event {
                return post;
            }
        }
        panic!("domain event delivered");
    }
}

pub fn setup_harness() -> Harness {
    let mut h = Harness::new(config());
    let admin = h.service.setup_mut().account(b"kit-admin", true, b"");
    h.service.setup_mut().token(admin, b"setup-admin", &[b"all"], Secret::new(boxed(TOKEN)));
    h.service.setup_mut().account(b"kit", false, b"kit:bot");
    h
}
pub fn make_setup_token(h: &mut Harness) -> skein_forge::forgejo::document::AccessToken {
    h.ask(b"POST", b"/api/v1/admin/users/kit/tokens", br#"{"name":"kit-api","scopes":["read:user"]}"#);
    assert_eq!(h.status(), 201);
    // The fake and the world share the codec's vocabulary, without another JSON reader.
    let token = h.service.setup().issued().last().expect("token made");
    skein_forge::forgejo::document::AccessToken {
        id: token.id,
        name: token.name.clone(),
        scopes: token.scopes.clone(),
        last_eight: Box::new([]),
        value: Some(token.secret.clone()),
    }
}

pub fn fake_config() -> fake::Config {
    fake::Config {
        limits: fake::Limits {
            repositories: 1,
            users: 3,
            labels: 2,
            items: 64,
            comments: 32,
            dependencies: 2,
            reviews: 8,
            branches: 4,
            commits: 16,
            files: 4,
            statuses: 4,
            contexts: 4,
            pages: 1,
            name_bytes: 64,
            title_bytes: 64,
            body_bytes: 1024,
            content_bytes: 128,
            page_size: 2,
            calls: 4,
            hooks: 8,
            observations: 64,
        },
        latency_min: Duration::from_millis(10),
        latency_max: Duration::from_millis(50),
        late: 0,
        late_min: Duration::from_secs(1),
        late_max: Duration::from_secs(3),
        unavailable: 0,
        timeouts: 0,
        landing: 0,
        land_min: Duration::from_secs(1),
        land_max: Duration::from_secs(3),
        rate_limit: 0,
        rate_window: Duration::from_secs(10),
        ci: 3,
        hook_min: Duration::ZERO,
        hook_max: Duration::ZERO,
        hooks_late: 0,
        hooks_lost: 0,
        resolution: Duration::from_secs(1),
        skew: fake::Skew::Ahead(Duration::from_secs(20)),
        status_updates: false,
        edit_updates: false,
    }
}
