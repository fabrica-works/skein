//! Account admission and removal through owner entrances (oauth.md, section 6.3).

use super::*;
use private::{Owner, owner_of, private_keeper};
use skein_io::{file, kernel};
use skein_lib::Token;
use skein_lib::stream::{Down, Up};

type Setup = fn() -> Owner;
type Failed = (u32, Ends, Failure);
type Sent = (Option<Failed>, (bool, u32, bool));

const ADDED: u32 = 7;
const UNSENT_CASES: usize = 4; // Signing in, held, asked and due refresh (oauth.md, section 6.3).
const KEEPERS: usize = 2; // Owner and private storage.
const PATH_BOUND: usize = 4096; // Private paths must be shorter than io's bound.
const ROOMY_ACCOUNTS: u32 = 3; // One configured account and two added places.
const DRIVE_STEPS: u32 = 128; // One tiny HTTP exchange and its close transitions.
const SOCKET: Token = Token::new(98);
const ADDRESS: core::net::SocketAddr =
    core::net::SocketAddr::new(core::net::IpAddr::V4(core::net::Ipv4Addr::LOCALHOST), 31000);

fn roomy() -> (Component, Env<Limits>) {
    let mut profile = limits();
    profile.accounts = ROOMY_ACCOUNTS;
    let mut configured = List::with_capacity(1);
    assert!(configured.push(Account::HandedIn).is_ok());
    (
        Component::new(configured, &profile, crypto(1)).expect("roomy component"),
        Env { now: Time::ZERO, wall: Wall::EPOCH, limits: profile },
    )
}

fn owner(configuration: Account) -> Owner {
    let mut owner = Owner::new();
    let mut configured = List::with_capacity(1);
    assert!(configured.push(configuration).is_ok());
    owner.component = Component::new(configured, &owner.env.limits, crypto(1)).expect("configured owner");
    owner
}

#[test]
fn an_added_name_is_refused_while_a_live_account_holds_it() {
    let (mut component, env) = roomy();
    let mut up = Queue::with_capacity(MAX_OUT_DOWN.above);
    component.add(&env, 0, Account::HandedIn, &mut up);
    assert_refused(up.pop(), 0, Asked::Add, Refusal::Taken);
    component.add(&env, ADDED, Account::HandedIn, &mut up);
    assert!(up.pop().is_none(), "an admitted add answers nothing");
    component.add(&env, ADDED, Account::HandedIn, &mut up);
    assert_refused(up.pop(), ADDED, Asked::Add, Refusal::Taken);
}

#[test]
fn an_add_past_the_accounts_limit_is_refused_as_full() {
    let (mut component, env) = roomy();
    let mut up = Queue::with_capacity(MAX_OUT_DOWN.above);
    for account in [ADDED, ADDED.saturating_add(1)] {
        component.add(&env, account, Account::HandedIn, &mut up);
    }
    assert!(up.is_empty(), "the account bound admits both free places");
    component.add(&env, ADDED.saturating_add(2), Account::HandedIn, &mut up);
    assert_refused(up.pop(), ADDED.saturating_add(2), Asked::Add, Refusal::Full { bound: ROOMY_ACCOUNTS });
}

#[derive(Clone, Copy, Debug)]
enum Bad {
    Plaintext,
    Redirect,
    Registration,
    Record,
    Path,
}

fn unusable(row: Bad) -> Account {
    let mut configuration = refresh_account(ADDED, ADDRESS);
    if let Account::SignIn { registration, endpoint, keeper } = &mut configuration {
        match row {
            Bad::Plaintext => endpoint.address = ([192, 0, 2, 1], 31000).into(),
            Bad::Redirect => registration.redirect_uri = bytes::copy_of(b"https://example.com/callback"),
            Bad::Registration => registration.client_id = bytes::copy_of(b""),
            Bad::Record => {
                let mut record = tests_record(0);
                record.access_token = bytes::copy_of(b"");
                *keeper = Keeper::Owner { kept: Some(record) };
            }
            Bad::Path => *keeper = private_keeper(bytes::zeroed(PATH_BOUND)),
        }
    }
    configuration
}

#[test]
fn an_added_configuration_new_refuses_is_refused_as_unusable() {
    for (row, why) in [
        (Bad::Plaintext, Unusable::Plaintext { account: ADDED }),
        (Bad::Redirect, Unusable::Redirect { account: ADDED }),
        (Bad::Registration, Unusable::Registration { account: ADDED, failure: skein_oauth::Failure::Limit }),
        (Bad::Record, Unusable::Record { account: ADDED, why: skein_oauth::DecodeError::Malformed }),
        (Bad::Path, Unusable::Path { account: ADDED }),
    ] {
        let configuration = unusable(row);
        let (mut component, env) = roomy();
        let mut up = Queue::with_capacity(MAX_OUT_DOWN.above);
        component.add(&env, ADDED, configuration, &mut up);
        assert_eq!(refusal_of(up.pop()), Some((ADDED, Asked::Add, Refusal::Unusable(why))), "{row:?}");
    }
}

#[test]
fn an_added_private_account_starts_its_load_at_the_next_fire() {
    let (mut component, env) = roomy();
    let mut up = Queue::with_capacity(MAX_OUT_DOWN.above);
    let mut configuration = refresh_account(ADDED, ADDRESS);
    if let Account::SignIn { keeper, .. } = &mut configuration {
        *keeper = private_keeper(bytes::copy_of(b"secret"));
    }
    component.add(&env, ADDED, configuration, &mut up);
    let mut files = Queue::with_capacity(MAX_OUT_FIRE.files);
    component.fire(&env, &mut up, &mut Queue::with_capacity(MAX_OUT_FIRE.io), &mut files);
    match files.pop() {
        Some(FileLower::Request { request: file::Request::OpenPrivate { .. }, .. }) => {}
        Some(FileLower::Request { .. } | FileLower::Cancel { .. }) | None => {
            panic!("the next fire starts the added private load")
        }
    }
}

fn waiting_sign_in() -> Owner {
    let mut owner = Owner::new();
    owner.ask(Request::SignIn { account: 0 });
    let load = owner.opened();
    owner.terminal(file::Event::Failed {
        owner: load,
        error: kernel::Error::NotFound,
        committed: false,
        residue: None,
    });
    owner.fire();
    drop(owner.above.pop());
    owner
}
fn held_grant() -> Owner {
    let mut owner = owner(Account::HandedIn);
    owner.ask(Request::HandIn { account: 0, record: tests_record(0) });
    owner.ask(Request::Grant { account: 0 });
    drop(owner.above.pop());
    owner
}
fn asked_grant() -> Owner {
    let mut owner = Owner::new();
    owner.ask(Request::Grant { account: 0 });
    owner
}
fn due_refresh() -> Owner {
    let mut owner = owner(refresh_account(0, ADDRESS));
    owner.ask(Request::Grant { account: 0 });
    owner
}

#[test]
fn a_removal_cancels_the_unsent_work_of_its_account() {
    let rows: [(&str, Setup, Failed); UNSENT_CASES] = [
        ("sign in", waiting_sign_in, (0, Ends::SignIn, Failure::Exchange(skein_oauth::Failure::Cancelled))),
        ("held grant", held_grant, (0, Ends::Grant, Failure::Exchange(skein_oauth::Failure::Cancelled))),
        ("asked grant", asked_grant, (0, Ends::Grant, Failure::Exchange(skein_oauth::Failure::Cancelled))),
        ("due refresh", due_refresh, (0, Ends::Grant, Failure::Exchange(skein_oauth::Failure::Cancelled))),
    ];
    for (name, setup, expected) in rows {
        let mut owner = setup();
        owner.ask(Request::Remove { account: 0 });
        owner.fire();
        assert_eq!(failure_of(owner.above.pop()), Some(expected), "{name}");
    }
}

#[test]
fn a_removing_account_refuses_its_requests() {
    let mut owner = due_refresh();
    owner.ask(Request::Remove { account: 0 });
    drop(owner.above.pop());
    for (request, asked) in [
        (Request::SignIn { account: 0 }, Asked::SignIn),
        (
            Request::Redirected { account: 0, uri: bytes::copy_of(b"http://localhost:31234/callback") },
            Asked::Redirected,
        ),
        (Request::HandIn { account: 0, record: tests_record(0) }, Asked::HandIn),
        (Request::Grant { account: 0 }, Asked::Grant),
        (Request::Remove { account: 0 }, Asked::Remove),
    ] {
        owner.ask(request);
        assert_eq!(refusal_of(owner.above.pop()), Some((0, asked, Refusal::Account)));
    }
}

#[test]
fn a_removed_name_is_free_for_add() {
    let mut owner = owner(Account::HandedIn);
    owner.ask(Request::Remove { account: 0 });
    owner.fire();
    assert_event(owner.above.pop(), Event::Removed { account: 0 });
    owner.component.add(&owner.env, 0, Account::HandedIn, &mut owner.above);
    owner.ask(Request::HandIn { account: 0, record: tests_record(0) });
    owner.ask(Request::Grant { account: 0 });
    assert_granted(owner.above.pop(), 0, 30);
}

#[test]
fn an_abort_settles_a_removal_before_closed() {
    let mut owner = due_refresh();
    owner.ask(Request::Remove { account: 0 });
    owner.ask(Request::Abort);
    owner.fire();
    assert_event(
        owner.above.pop(),
        Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
    );
    owner.fire();
    assert_event(owner.above.pop(), Event::Removed { account: 0 });
    assert_event(owner.above.pop(), Event::Closed);
}

fn private_refresh() -> Owner {
    let mut owner = Owner::new();
    owner.ask(Request::Grant { account: 0 });
    let load = owner.opened();
    let record = match refresh_account(0, ADDRESS) {
        Account::SignIn { keeper: Keeper::Owner { kept: Some(record) }, .. } => record,
        Account::SignIn { .. } | Account::HandedIn => unreachable!("refresh fixture record"),
    };
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&record, &owner.env.limits.client.document).expect("kept record"),
    });
    owner
}
fn socket_up(owner: &mut Owner, event: LowerEvent) {
    owner.component.up(&owner.env, event, &mut owner.above, &mut owner.io, &mut owner.files);
}
fn sent(owner: &mut Owner) -> Token {
    let mut identity = None;
    for _ in 0..DRIVE_STEPS {
        owner.fire();
        while let Some(request) = owner.io.pop() {
            if let Lower::Connect { owner: token, .. } = request {
                identity = Some(token);
                socket_up(owner, LowerEvent::Connecting { owner: token, socket: SOCKET });
                socket_up(owner, LowerEvent::Connected { owner: token });
            } else if let Lower::Stream { down, .. } = request {
                let token = identity.expect("exchange owner");
                if let Down::Send(bytes) = down {
                    if !bytes.is_empty() {
                        return token;
                    }
                } else if let Down::Demand { room, .. } = down
                    && room != 0
                {
                    socket_up(owner, LowerEvent::Stream { owner: token, up: Up::Room });
                }
            }
        }
    }
    panic!("refresh sends its HTTP request")
}
fn finish_sent(owner: &mut Owner, identity: Token) -> (bool, u32, bool) {
    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 61\r\nConnection: close\r\n\r\n{\"access_token\":\"new\",\"refresh_token\":\"next\",\"expires_in\":30}";
    let mut response = RESPONSE;
    let mut kept = false;
    let mut removed: u32 = 0;
    let mut granted = false;
    for _ in 0..DRIVE_STEPS {
        owner.fire();
        while let Some(event) = owner.above.pop() {
            if let Event::Keep { account, record } = event {
                kept = true;
                owner.ask(Request::Kept { account, generation: record.generation, keeping: Keeping::Kept });
            } else if let Event::Removed { .. } = event {
                removed = removed.saturating_add(1);
            } else if let Event::Granted { .. } = event {
                granted = true;
            }
        }
        finish_socket(owner, identity, &mut response);
        kept |= finish_file(owner);
        if removed != 0 {
            return (kept, removed, granted);
        }
    }
    panic!("sent refresh and its keep settle before removed")
}

#[test]
fn a_removal_lets_a_sent_refresh_and_its_keep_finish() {
    let cancelled = Some((0, Ends::Grant, Failure::Exchange(skein_oauth::Failure::Cancelled)));
    let rows: [(&str, Setup, Sent); KEEPERS] = [
        ("owner", due_refresh, (cancelled, (true, 1, false))),
        ("private", private_refresh, (cancelled, (true, 1, false))),
    ];
    for (name, setup, expected) in rows {
        let mut owner = setup();
        let identity = sent(&mut owner);
        owner.ask(Request::Remove { account: 0 });
        let failed = failure_of(owner.above.pop());
        let finished = finish_sent(&mut owner, identity);
        assert_eq!((failed, finished), expected, "{name}");
    }
}

fn finish_socket(owner: &mut Owner, identity: Token, response: &mut &[u8]) {
    while let Some(request) = owner.io.pop() {
        if let Lower::Close { .. } | Lower::Abort { .. } = request {
            socket_up(owner, LowerEvent::Closed { owner: identity });
        } else if let Lower::Stream { down: Down::Demand { read, room }, .. } = request {
            if room != 0 {
                socket_up(owner, LowerEvent::Stream { owner: identity, up: Up::Room });
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
                    socket_up(owner, LowerEvent::Stream { owner: identity, up: Up::Bytes(bytes) });
                }
            }
        }
    }
}
fn finish_file(owner: &mut Owner) -> bool {
    if let Some(FileLower::Request { request, .. }) = owner.files.pop() {
        let token = owner_of(&request);
        if let file::Request::Store { bytes, .. } = request {
            owner.terminal(file::Event::Stored { owner: token, digest: skein_io::digest::digest(&bytes) });
            return true;
        } else if let file::Request::Close { .. } = request {
            owner.terminal(file::Event::Closed { owner: token });
        }
    }
    false
}

#[test]
fn a_removing_number_is_taken_until_its_terminal() {
    let mut owner = due_refresh();
    owner.ask(Request::Remove { account: 0 });
    drop(owner.above.pop());
    owner.component.add(&owner.env, 0, Account::HandedIn, &mut owner.above);
    assert_refused(owner.above.pop(), 0, Asked::Add, Refusal::Taken);
}

#[test]
fn an_add_after_close_is_refused_as_closed() {
    let mut owner = owner(Account::HandedIn);
    owner.ask(Request::Close);
    drop(owner.above.pop());
    owner.component.add(&owner.env, ADDED, Account::HandedIn, &mut owner.above);
    assert_refused(owner.above.pop(), ADDED, Asked::Add, Refusal::Closed);
}

#[test]
fn removing_an_unknown_account_is_refused() {
    let mut owner = owner(Account::HandedIn);
    owner.ask(Request::Remove { account: ADDED });
    assert_refused(owner.above.pop(), ADDED, Asked::Remove, Refusal::Account);
}

#[test]
fn removing_after_close_is_refused_as_closed() {
    let mut owner = owner(Account::HandedIn);
    owner.ask(Request::Close);
    drop(owner.above.pop());
    owner.ask(Request::Remove { account: ADDED });
    assert_refused(owner.above.pop(), ADDED, Asked::Remove, Refusal::Closed);
}
