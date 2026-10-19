//! Private keeper entrance and terminal ownership (oauth.md, section 6.4).

use super::*;
use crate::{FileLower, Place, Unloaded};
use skein_io::{file, kernel};
use skein_lib::Token;
use skein_lib::stream::{Down, Up};

pub(super) struct Owner {
    pub(super) component: Component,
    pub(super) env: Env<Limits>,
    pub(super) above: Queue<Event>,
    pub(super) io: Queue<Lower>,
    pub(super) files: Queue<FileLower>,
}

impl Owner {
    pub(super) fn new() -> Owner {
        Self::with_terms(None)
    }
    fn with_terms(terms: Option<skein_oauth::Terms>) -> Owner {
        let mut limits = limits();
        limits.client.document.issued_client_bytes = limits.client.document.client_bytes;
        let mut configured = List::with_capacity(1);
        match configured.push(Account::SignIn {
            registration: skein_oauth::Registration {
                authorization_url: bytes::copy_of(b"https://issuer.example/authorize"),
                token_endpoint: bytes::copy_of(b"http://127.0.0.1:31000/token"),
                client_id: bytes::copy_of(b"client"),
                client_secret: Some(bytes::copy_of(b"fake secret")),
                redirect_uri: bytes::copy_of(b"https://service.example/callback"),
                scope: bytes::copy_of(b"read"),
                wire: skein_oauth::WireFormat::Form,
                pkce_for_confidential: false,
                metadata_claim: None,
                terms,
            },
            endpoint: Endpoint {
                address: kernel::Addr::from(([127, 0, 0, 1], 31000)),
                transport: Transport::Plaintext,
            },
            keeper: private_keeper(bytes::copy_of(b"secret")),
        }) {
            Ok(()) => {}
            Err(_) => unreachable!("account slot"),
        }
        Owner {
            component: Component::new(configured, &limits, crypto(7)).expect("private account"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            above: Queue::with_capacity(32),
            io: Queue::with_capacity(32),
            files: Queue::with_capacity(32),
        }
    }
    pub(super) fn ask(&mut self, request: Request) {
        self.component.down(&self.env, request, &mut self.above, &mut self.io, &mut self.files);
    }
    pub(super) fn fire(&mut self) {
        let before_above = self.above.len();
        let before_io = self.io.len();
        let before_files = self.files.len();
        self.component.fire(&self.env, &mut self.above, &mut self.io, &mut self.files);
        assert!(self.above.len().saturating_sub(before_above) <= MAX_OUT_FIRE.above);
        assert!(self.io.len().saturating_sub(before_io) <= MAX_OUT_FIRE.io);
        assert!(self.files.len().saturating_sub(before_files) <= MAX_OUT_FIRE.files);
    }
    pub(super) fn terminal(&mut self, event: file::Event) {
        let before_above = self.above.len();
        let before_io = self.io.len();
        let before_files = self.files.len();
        self.component.filed(&self.env, event, &mut self.above, &mut self.io, &mut self.files);
        assert!(self.above.len().saturating_sub(before_above) <= MAX_OUT_FILED.above);
        assert!(self.io.len().saturating_sub(before_io) <= MAX_OUT_FILED.io);
        assert!(self.files.len().saturating_sub(before_files) <= MAX_OUT_FILED.files);
    }
    pub(super) fn next(&mut self) -> file::Request {
        for _ in 0_u32..32 {
            match self.files.pop() {
                Some(FileLower::Request { request, deadline }) => {
                    assert_eq!(deadline, self.env.now.saturating_add(self.env.limits.file_stall));
                    return request;
                }
                Some(FileLower::Cancel { .. }) => panic!("expected a file request"),
                None => self.fire(),
            }
        }
        panic!("one ready file request")
    }
    pub(super) fn opened(&mut self) -> Token {
        let request = self.next();
        check_file(&request, 0);
        let owner = owner_of(&request);
        self.terminal(file::Event::Opened { owner, file: Token::new(100), len: 0 });
        let request = self.next();
        check_file(&request, 1);
        owner_of(&request)
    }
}

pub(super) fn owner_of(request: &file::Request) -> Token {
    match request {
        file::Request::Lock { owner, .. }
        | file::Request::MakeDirectory { owner, .. }
        | file::Request::OpenPrivate { owner, .. }
        | file::Request::Create { owner, .. }
        | file::Request::CreateNoFollow { owner, .. }
        | file::Request::OpenWrite { owner, .. }
        | file::Request::OpenWriteNoFollow { owner, .. }
        | file::Request::Cut { owner, .. }
        | file::Request::OpenRead { owner, .. }
        | file::Request::OpenReadNoFollow { owner, .. }
        | file::Request::OpenDirectory { owner, .. }
        | file::Request::Load { owner, .. }
        | file::Request::Scan { owner, .. }
        | file::Request::Store { owner, .. }
        | file::Request::Stat { owner, .. }
        | file::Request::WriteAt { owner, .. }
        | file::Request::ReadAt { owner, .. }
        | file::Request::Sync { owner, .. }
        | file::Request::Close { owner, .. }
        | file::Request::SyncDirectory { owner, .. }
        | file::Request::Rename { owner, .. }
        | file::Request::Remove { owner, .. }
        | file::Request::ListPage { owner, .. }
        | file::Request::List { owner, .. } => *owner,
    }
}

#[test]
fn a_refresh_refused_as_sign_in_again_loads_the_file_at_the_next_grant() {
    let mut owner = Owner::new();
    owner.ask(Request::Grant { account: 0 });
    let load = owner.opened();
    let mut record = tests_record(0);
    record.refresh_token = Some(bytes::copy_of(b"refresh"));
    record.expires_at = Wall::from_nanos(5_000_000_000);
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&record, &owner.env.limits.client.document).expect("private record"),
    });
    component::refuse_refresh(&mut owner.component, &owner.env, &mut owner.above, &mut owner.io);
    assert_event(
        owner.above.pop(),
        Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Exchange(skein_oauth::Failure::Refused) },
    );
    owner.ask(Request::Grant { account: 0 });
    let request = owner.next();
    check_file(&request, 1);
    owner.terminal(file::Event::Loaded {
        owner: owner_of(&request),
        bytes: skein_oauth::encode_record(&tests_record(1), &owner.env.limits.client.document).expect("new record"),
    });
    assert_granted(owner.above.pop(), 1, 30);
    assert!(owner.files.is_empty(), "loading again neither erases nor rewrites the person's file");
}

#[test]
fn requests_during_a_load_join_it_and_end_once() {
    let mut owner = Owner::new();
    owner.ask(Request::SignIn { account: 0 });
    owner.ask(Request::Grant { account: 0 });
    owner.ask(Request::Grant { account: 0 });
    assert_refused(owner.above.pop(), 0, Asked::Grant, Refusal::Held);
    let load = owner.opened();
    assert!(owner.above.is_empty() && owner.io.is_empty(), "no visit before loading");
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&tests_record(1), &owner.env.limits.client.document).expect("record"),
    });
    for _ in 0_u32..4 {
        owner.fire();
    }
    assert_visit(owner.above.pop());
    owner.ask(Request::Cancel { account: 0 });
    assert_event(
        owner.above.pop(),
        Event::Failed { account: 0, ends: Ends::SignIn, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
    );
    assert_event(
        owner.above.pop(),
        Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
    );
    assert!(owner.above.is_empty());
}

#[test]
fn missing_and_unreadable_records_end_grants_but_admit_sign_in() {
    for unreadable in [false, true] {
        let mut owner = Owner::new();
        owner.ask(Request::SignIn { account: 0 });
        owner.ask(Request::Grant { account: 0 });
        let load = owner.opened();
        let event = if unreadable {
            file::Event::Loaded { owner: load, bytes: bytes::copy_of(b"old format") }
        } else {
            file::Event::Failed { owner: load, error: kernel::Error::NotFound, committed: false, residue: None }
        };
        owner.terminal(event);
        assert_event(
            owner.above.pop(),
            Event::Failed {
                account: 0,
                ends: Ends::Grant,
                failure: if unreadable {
                    Failure::Unloaded { at: Place::File, why: Unloaded::Unreadable }
                } else {
                    Failure::Expired
                },
            },
        );
        owner.fire();
        assert_visit(owner.above.pop());
        assert!(owner.above.is_empty());
    }
}

#[test]
fn an_unread_load_fails_sign_in_before_visit_and_the_next_request_reloads() {
    for why in [
        Unloaded::Refused(file::Unsafe::Mode(0o644)),
        Unloaded::Failed(kernel::Error::Permission),
        Unloaded::Stalled,
        Unloaded::TooLarge { size: 300 },
    ] {
        let mut owner = Owner::new();
        owner.ask(Request::SignIn { account: 0 });
        owner.ask(Request::Grant { account: 0 });
        let load = owner.opened();
        let event = match why {
            Unloaded::Refused(found) => file::Event::Refused { owner: load, found },
            Unloaded::Failed(error) => file::Event::Failed { owner: load, error, committed: false, residue: None },
            Unloaded::Stalled => {
                file::Event::Failed { owner: load, error: kernel::Error::TimedOut, committed: false, residue: None }
            }
            Unloaded::TooLarge { size } => file::Event::TooLarge { owner: load, size },
            Unloaded::Unreadable => unreachable!("unread failure stories"),
        };
        owner.terminal(event);
        for ends in [Ends::Grant, Ends::SignIn] {
            assert_event(
                owner.above.pop(),
                Event::Failed { account: 0, ends, failure: Failure::Unloaded { at: Place::File, why } },
            );
        }
        assert!(owner.above.is_empty() && owner.io.is_empty());
        owner.component.reclaim();
        owner.ask(Request::Grant { account: 0 });
        let request = owner.next();
        check_file(&request, 1);
        owner.terminal(file::Event::Loaded {
            owner: owner_of(&request),
            bytes: skein_oauth::encode_record(&tests_record(1), &owner.env.limits.client.document)
                .expect("person fixed record"),
        });
        assert_granted(owner.above.pop(), 1, 30);
        owner.ask(Request::Release { account: 0 });
        owner.ask(Request::Grant { account: 0 });
        assert_granted(owner.above.pop(), 1, 30);
        assert!(owner.files.is_empty(), "a held record is cached");
    }
}

#[test]
fn cancelling_a_sign_in_waiting_on_load_leaves_the_account_load_running() {
    let mut owner = Owner::new();
    owner.ask(Request::SignIn { account: 0 });
    let load = owner.opened();
    owner.ask(Request::Grant { account: 0 });
    owner.ask(Request::Cancel { account: 0 });
    assert_event(
        owner.above.pop(),
        Event::Failed { account: 0, ends: Ends::SignIn, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
    );
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&tests_record(1), &owner.env.limits.client.document).expect("record"),
    });
    assert_granted(owner.above.pop(), 1, 30);
    assert!(owner.files.is_empty());
}

#[test]
fn close_finishes_the_load_then_closes_only_its_private_root() {
    let mut owner = Owner::new();
    owner.ask(Request::Grant { account: 0 });
    let load = owner.opened();
    owner.ask(Request::Close);
    assert!(owner.above.is_empty());
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&tests_record(1), &owner.env.limits.client.document).expect("record"),
    });
    assert_granted(owner.above.pop(), 1, 30);
    let close = owner.next();
    check_file(&close, 2);
    assert!(owner.above.is_empty());
    owner.terminal(file::Event::Closed { owner: owner_of(&close) });
    assert_event(owner.above.pop(), Event::Closed);
    assert!(!owner.component.has_work());
    assert_eq!(owner.component.next_deadline(), None);
}

#[test]
fn abort_cancels_a_load_and_waiters_and_waits_for_its_terminal_before_closed() {
    let mut owner = Owner::new();
    owner.ask(Request::SignIn { account: 0 });
    owner.ask(Request::Grant { account: 0 });
    let load = owner.opened();
    owner.ask(Request::Abort);
    for _ in 0_u32..4 {
        owner.fire();
    }
    for ends in [Ends::SignIn, Ends::Grant] {
        assert_event(
            owner.above.pop(),
            Event::Failed { account: 0, ends, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
        );
    }
    match owner.files.pop() {
        Some(FileLower::Cancel { owner: token }) => assert_eq!(token, load),
        Some(FileLower::Request { .. }) | None => panic!("abort cancels the outstanding owner"),
    }
    assert!(owner.above.is_empty());
    owner.terminal(file::Event::Cancelled { owner: load });
    let close = owner.next();
    owner.terminal(file::Event::Closed { owner: owner_of(&close) });
    assert_event(owner.above.pop(), Event::Closed);
}

fn assert_visit(event: Option<Event>) {
    match event {
        Some(Event::Visit { .. }) => {}
        Some(
            Event::Removed { .. }
            | Event::SignedIn { .. }
            | Event::Granted { .. }
            | Event::Expiring { .. }
            | Event::Keep { .. }
            | Event::Failed { .. }
            | Event::Refused { .. }
            | Event::Closed,
        )
        | None => panic!("one visit after loading"),
    }
}

fn check_file(request: &file::Request, shape: u32) {
    match request {
        file::Request::OpenPrivate { root, path, .. } => {
            assert_eq!(shape, 0);
            assert_eq!(*root, Token::new(99));
            assert_eq!(path.as_ref(), b"secret");
        }
        file::Request::Load { root, no_follow, .. } => {
            assert_eq!(shape, 1);
            assert_eq!(*root, Token::new(100));
            assert!(*no_follow);
        }
        file::Request::Close { file, .. } => {
            assert_eq!(shape, 2);
            assert_eq!(*file, Token::new(100));
        }
        file::Request::Lock { .. }
        | file::Request::MakeDirectory { .. }
        | file::Request::Create { .. }
        | file::Request::CreateNoFollow { .. }
        | file::Request::OpenWrite { .. }
        | file::Request::OpenWriteNoFollow { .. }
        | file::Request::Cut { .. }
        | file::Request::OpenRead { .. }
        | file::Request::OpenReadNoFollow { .. }
        | file::Request::OpenDirectory { .. }
        | file::Request::Scan { .. }
        | file::Request::Store { .. }
        | file::Request::Stat { .. }
        | file::Request::WriteAt { .. }
        | file::Request::ReadAt { .. }
        | file::Request::Sync { .. }
        | file::Request::SyncDirectory { .. }
        | file::Request::Rename { .. }
        | file::Request::Remove { .. }
        | file::Request::ListPage { .. }
        | file::Request::List { .. } => panic!("one private open, load or close"),
    }
}

fn issued_owner(request: Request, issued_client_id: Option<Box<[u8]>>) -> (Owner, skein_io::digest::Digest) {
    let mut owner = Owner::with_terms(Some(issued_terms()));
    owner.ask(request);
    let load = owner.opened();
    let mut record = tests_record(0);
    record.issued_client_id = issued_client_id;
    let bytes =
        skein_oauth::encode_record(&record, &owner.env.limits.client.document).expect("record without issued id");
    let digest = skein_io::digest::digest(&bytes);
    owner.terminal(file::Event::Loaded { owner: load, bytes });
    (owner, digest)
}

#[test]
fn a_record_without_an_issued_id_loads_unreadable() {
    for (name, issued_client_id) in [("absent", None), ("empty", Some(Box::from([])))] {
        let (mut owner, _) = issued_owner(Request::Grant { account: 0 }, issued_client_id);
        assert_eq!(
            failure_of(owner.above.pop()),
            Some(Failed {
                account: 0,
                ends: Ends::Grant,
                failure: Failure::Unloaded { at: Place::File, why: Unloaded::Unreadable }
            }),
            "{name}"
        );
    }
}

#[test]
fn a_sign_in_replaces_it_expecting_the_digest_it_loaded() {
    let (owner, digest) = issued_owner(Request::SignIn { account: 0 }, None);
    assert_eq!(owner.component.private_expected(0), file::Expect::Digest(digest));
}

pub(super) fn private_keeper(directory: Box<[u8]>) -> Keeper {
    Keeper::Private { root: Token::new(99), directory, file: bytes::copy_of(b"record") }
}

const DRIVE_STEPS: u32 = 128; // One tiny HTTP exchange and its close transitions.
const HTTP_SOCKET: Token = Token::new(98);
const BODY: &[u8] = br#"{"access_token":"new","refresh_token":"next","expires_in":30}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RefreshOutcome {
    pub(super) kept: bool,
    pub(super) removed: u32,
    pub(super) granted: bool,
}

impl Owner {
    fn socket_up(&mut self, event: LowerEvent) {
        self.component.up(&self.env, event, &mut self.above, &mut self.io, &mut self.files);
    }

    pub(super) fn sent(&mut self) -> Token {
        let mut identity = None;
        for _ in 0..DRIVE_STEPS {
            self.fire();
            while let Some(request) = self.io.pop() {
                if let Lower::Connect { owner: token, .. } = request {
                    identity = Some(token);
                    self.socket_up(LowerEvent::Connecting { owner: token, socket: HTTP_SOCKET });
                    self.socket_up(LowerEvent::Connected { owner: token });
                } else if let Lower::Stream { down, .. } = request {
                    let token = identity.expect("exchange owner");
                    if let Down::Send(bytes) = down {
                        if !bytes.is_empty() {
                            return token;
                        }
                    } else if let Down::Demand { room, .. } = down
                        && room != 0
                    {
                        self.socket_up(LowerEvent::Stream { owner: token, up: Up::Room });
                    }
                }
            }
        }
        panic!("refresh sends its HTTP request")
    }

    fn finish_socket(&mut self, identity: Token, response: &mut &[u8]) {
        while let Some(request) = self.io.pop() {
            if let Lower::Close { .. } | Lower::Abort { .. } = request {
                self.socket_up(LowerEvent::Closed { owner: identity });
            } else if let Lower::Stream { down: Down::Demand { read, room }, .. } = request {
                if room != 0 {
                    self.socket_up(LowerEvent::Stream { owner: identity, up: Up::Room });
                } else if !response.is_empty() {
                    let cap = match read {
                        skein_lib::stream::Read::Nothing => 0,
                        skein_lib::stream::Read::Fill(max) | skein_lib::stream::Read::Arrived { max } => {
                            usize::try_from(max).expect("u32 fits")
                        }
                        skein_lib::stream::Read::Line { max } => bytes::line_end(response)
                            .unwrap_or(response.len())
                            .saturating_add(1)
                            .min(usize::try_from(max).expect("u32 fits")),
                        skein_lib::stream::Read::Scan { until, max } => {
                            let cap = usize::try_from(max).expect("u32 fits");
                            match bytes::find(response, until.as_bytes()) {
                                Some(at) => at.saturating_add(until.as_bytes().len()).min(cap),
                                None => cap,
                            }
                        }
                    }
                    .min(response.len());
                    if cap != 0 {
                        let bytes = bytes::copy_of(response.get(..cap).expect("response piece"));
                        *response = response.get(cap..).expect("remaining response");
                        self.socket_up(LowerEvent::Stream { owner: identity, up: Up::Bytes(bytes) });
                    }
                }
            }
        }
    }

    fn finish_file(&mut self) -> bool {
        if let Some(FileLower::Request { request, .. }) = self.files.pop() {
            let token = owner_of(&request);
            if let file::Request::Store { bytes, .. } = request {
                self.terminal(file::Event::Stored { owner: token, digest: skein_io::digest::digest(&bytes) });
                return true;
            } else if let file::Request::Close { .. } = request {
                self.terminal(file::Event::Closed { owner: token });
            }
        }
        false
    }

    pub(super) fn finish_refresh(&mut self, identity: Token) -> RefreshOutcome {
        const PREFIX: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: ";
        const SUFFIX: &[u8] = b"\r\nConnection: close\r\n\r\n";
        let length = skein_lib::Decimal::of(u64::try_from(BODY.len()).expect("body length fits"));
        let mut response = skein_lib::Writer::new(
            PREFIX
                .len()
                .saturating_add(length.as_bytes().len())
                .saturating_add(SUFFIX.len())
                .saturating_add(BODY.len()),
        );
        for part in [PREFIX, length.as_bytes(), SUFFIX, BODY] {
            response.put(part).expect("response measured");
        }
        let response = response.finish();
        let mut remaining = response.as_ref();
        let mut outcome = RefreshOutcome { kept: false, removed: 0, granted: false };
        for _ in 0..DRIVE_STEPS {
            self.fire();
            while let Some(event) = self.above.pop() {
                if let Event::Keep { account, record } = event {
                    outcome.kept = true;
                    self.ask(Request::Kept { account, generation: record.generation, keeping: Keeping::Kept });
                } else if let Event::Removed { .. } = event {
                    outcome.removed = outcome.removed.saturating_add(1);
                } else if let Event::Granted { .. } = event {
                    outcome.granted = true;
                }
            }
            self.finish_socket(identity, &mut remaining);
            outcome.kept |= self.finish_file();
            self.component.reclaim();
            if outcome.removed != 0
                || (remaining.is_empty() && !self.component.has_work() && self.io.is_empty() && self.files.is_empty())
            {
                return outcome;
            }
        }
        panic!("refresh and its keep settle")
    }
}
