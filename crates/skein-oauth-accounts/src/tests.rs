use super::*;
use skein_lib::{Duration, Env, List, Queue, Time, Wall, bytes};
use skein_oauth::{ClientLimits, SavedToken};

pub(crate) fn limits() -> Limits {
    Limits {
        accounts: 2,
        file_stall: Duration::from_secs(1),
        exchanges: 2,
        listeners: 1,
        server: skein_http::server::Limits {
            head: 2048,
            headers: 16,
            body: 1024,
            read: 256,
            response: 2048,
            send: 256,
        },
        http: skein_http::client::Limits { request: 2048, head: 2048, headers: 16, read: 256, send: 256 },
        tls: skein_tls::client::Limits { read: 2048, send: 2048, records: 131_072 },
        io: skein_io::Limits {
            sockets: 4,
            refusals: 1,
            intake: 32_768,
            receive: 1024,
            output: 32_768,
            sends: 4,
            accepts: 1,
            backlog: 2,
            close_timeout: Duration::from_secs(1),
            names: 0,
            retry: Duration::from_millis(1),
        },
        refresh_lead: Duration::from_secs(10),
        client: ClientLimits {
            document: skein_oauth::Limits {
                document_bytes: 1024,
                string_bytes: 256,
                token_bytes: 64,
                client_bytes: 64,
                detail_bytes: 64,
                record_bytes: 512,
                depth: 8,
                tokens: 64,
                issued_client_bytes: 0,
                granted_bytes: 64,
                subject_bytes: 0,
                id_token_bytes: 64,
            },
            uri_bytes: 256,
            scope_bytes: 64,
            state_bytes: 64,
            code_bytes: 64,
            url_bytes: 1024,
            request_bytes: 1024,
            parameter_bytes: 0,
            resource_bytes: 0,
            nonce_bytes: 0,
            sign_in_time: Duration::from_secs(120),
            request_time: Duration::from_secs(10),
            backoff_base: Duration::from_secs(2),
            backoff_ceiling: Duration::from_secs(8),
            max_attempts: 3,
        },
    }
}
fn setup() -> (Component, Env<Limits>, Queue<Event>) {
    let limits = limits();
    let mut accounts = List::with_capacity(2);
    match accounts.push(Account::HandedIn) {
        Ok(()) => {}
        Err(_) => unreachable!("account slot"),
    }
    match accounts.push(Account::HandedIn) {
        Ok(()) => {}
        Err(_) => unreachable!("account slot"),
    }
    let component = Component::new(accounts, &limits, crypto(1)).expect("component");
    (component, Env { now: Time::ZERO, wall: Wall::EPOCH, limits }, Queue::with_capacity(MAX_OUT_DOWN.above))
}
fn tests_record(generation: u64) -> SavedToken {
    SavedToken {
        key: 0,
        generation,
        access_token: bytes::copy_of(b"access"),
        refresh_token: None,
        metadata: None,
        expires_at: Wall::from_nanos(30_000_000_000),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
fn hand_in(component: &mut Component, env: &Env<Limits>, out: &mut Queue<Event>, generation: u64) {
    component.down(
        env,
        Request::HandIn { account: 0, record: tests_record(generation) },
        out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
}
fn assert_event(event: Option<Event>, expected: Event) {
    assert!(event == Some(expected), "the request has its expected typed outcome");
}
fn assert_granted(event: Option<Event>, generation: u64, seconds: u64) {
    assert_event(
        event,
        Event::Granted {
            account: 0,
            token: bytes::copy_of(b"access"),
            generation,
            valid: Duration::from_secs(seconds),
        },
    );
}
fn assert_expired(event: Option<Event>) {
    assert_event(event, Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Expired });
}
fn assert_refused(event: Option<Event>, account: u32, asked: Asked, why: Refusal) {
    assert_event(event, Event::Refused { account, asked, why });
}

#[test]
fn a_handed_in_record_is_lent_while_it_is_valid() {
    let (mut component, env, mut out) = setup();
    hand_in(&mut component, &env, &mut out, 1);
    assert!(out.is_empty());
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_granted(out.pop(), 1, 30);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_refused(out.pop(), 0, Asked::Grant, Refusal::Held);
    component.down(
        &env,
        Request::Release { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_granted(out.pop(), 1, 30);
}

#[test]
fn an_access_only_record_is_announced_expiring_once_and_fails_at_expiry() {
    let (mut component, mut env, mut out) = setup();
    hand_in(&mut component, &env, &mut out, 1);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    drop(out.pop());
    assert_eq!(component.next_deadline(), Some(Time::from_nanos(20_000_000_000)));
    env.now = Time::from_nanos(20_000_000_000);
    // Wall jumps never extend a running grant's deadline.
    env.wall = Wall::from_nanos(100_000_000_000);
    component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
    assert_event(out.pop(), Event::Expiring { account: 0, generation: 1 });
    component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
    assert!(out.is_empty());
    assert_eq!(component.next_deadline(), Some(Time::from_nanos(30_000_000_000)));
    env.now = Time::from_nanos(30_000_000_000);
    component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
    assert_expired(out.pop());
    component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
    assert!(out.is_empty());
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_expired(out.pop());
}

#[test]
fn a_rejected_record_fails_until_a_newer_one_is_handed_in() {
    let (mut component, mut env, mut out) = setup();
    hand_in(&mut component, &env, &mut out, 1);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    drop(out.pop());
    component.down(
        &env,
        Request::Rejected { account: 0, generation: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert!(out.is_empty());
    component.down(
        &env,
        Request::Rejected { account: 0, generation: 1 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_expired(out.pop());
    hand_in(&mut component, &env, &mut out, 1);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_expired(out.pop());
    env.now = Time::from_nanos(1_000_000_000);
    env.wall = Wall::from_nanos(1_000_000_000);
    hand_in(&mut component, &env, &mut out, 2);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_granted(out.pop(), 2, 29);
}

#[test]
fn a_newer_record_announces_the_new_generation_while_held() {
    let (mut component, env, mut out) = setup();
    hand_in(&mut component, &env, &mut out, 1);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    drop(out.pop());
    hand_in(&mut component, &env, &mut out, 0);
    assert!(out.is_empty());
    hand_in(&mut component, &env, &mut out, 2);
    assert_granted(out.pop(), 2, 30);
}

#[test]
fn a_record_with_a_refresh_token_or_past_its_bounds_is_refused() {
    let (mut component, env, mut out) = setup();
    let mut record = tests_record(1);
    record.refresh_token = Some(bytes::copy_of(b"refresh"));
    component.down(
        &env,
        Request::HandIn { account: 0, record },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_refused(out.pop(), 0, Asked::HandIn, Refusal::NotAccessOnly);
    let mut record = tests_record(1);
    record.access_token = bytes::copy_of(&[b'x'; 65]);
    component.down(
        &env,
        Request::HandIn { account: 0, record },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_refused(out.pop(), 0, Asked::HandIn, Refusal::Record(skein_oauth::DecodeError::TooLarge));
}

#[test]
fn close_and_abort_settle_once_clear_deadlines_and_refuse_new_work_as_closed() {
    for close in [Request::Close, Request::Abort] {
        let (mut component, env, mut out) = setup();
        hand_in(&mut component, &env, &mut out, 1);
        component.down(&env, close, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
        assert_event(out.pop(), Event::Closed);
        assert_eq!(component.next_deadline(), None);
        component.down(
            &env,
            Request::Close,
            &mut out,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(1),
        );
        component.down(
            &env,
            Request::Abort,
            &mut out,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(1),
        );
        component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
        assert!(out.is_empty());
        component.down(
            &env,
            Request::Grant { account: 9 },
            &mut out,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(1),
        );
        assert_refused(out.pop(), 9, Asked::Grant, Refusal::Closed);
        component.down(
            &env,
            Request::SignIn { account: 9 },
            &mut out,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(1),
        );
        assert_refused(out.pop(), 9, Asked::SignIn, Refusal::Closed);
        component.down(
            &env,
            Request::Redirected { account: 9, uri: bytes::copy_of(b"late") },
            &mut out,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(1),
        );
        assert_refused(out.pop(), 9, Asked::Redirected, Refusal::Closed);

        hand_in(&mut component, &env, &mut out, 2);
        assert_refused(out.pop(), 0, Asked::HandIn, Refusal::Closed);
    }
}

#[test]
fn a_missing_account_and_missing_record_have_typed_terminals() {
    let (mut component, env, mut out) = setup();
    component.down(
        &env,
        Request::Grant { account: 3 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_refused(out.pop(), 3, Asked::Grant, Refusal::Account);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    assert_expired(out.pop());
}

#[test]
fn a_released_record_still_announces_its_lead_without_an_expiry_failure() {
    let (mut component, mut env, mut out) = setup();
    hand_in(&mut component, &env, &mut out, 1);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    drop(out.pop());
    component.down(
        &env,
        Request::Release { account: 0 },
        &mut out,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(1),
    );
    env.now = Time::from_nanos(20_000_000_000);
    component.fire(&env, &mut out, &mut Queue::with_capacity(MAX_OUT_DOWN.io), &mut Queue::with_capacity(1));
    assert_event(out.pop(), Event::Expiring { account: 0, generation: 1 });
    assert_eq!(component.next_deadline(), None);
}

fn refresh_account(account: u32, address: core::net::SocketAddr) -> Account {
    let mut record = tests_record(0);
    record.key = account;
    record.refresh_token = Some(bytes::copy_of(b"refresh"));
    record.expires_at = Wall::from_nanos(5_000_000_000);
    Account::SignIn {
        registration: skein_oauth::Registration {
            authorization_url: bytes::copy_of(b"http://127.0.0.1:31000/authorize"),
            token_endpoint: bytes::copy_of(b"http://127.0.0.1:31000/token"),
            client_id: bytes::copy_of(b"client"),
            redirect_uri: bytes::copy_of(b"http://localhost:31234/callback"),
            scope: bytes::copy_of(b"read"),
            wire: skein_oauth::WireFormat::Form,
            client_secret: None,
            pkce_for_confidential: false,
            metadata_claim: None,
            terms: None,
        },
        endpoint: Endpoint { address, transport: Transport::Plaintext },
        keeper: Keeper::Owner { kept: Some(record) },
    }
}

#[test]
fn a_refresh_refused_as_sign_in_again_drops_an_owner_kept_record() {
    let profile = limits();
    let mut configured = List::with_capacity(1);
    assert!(configured.push(refresh_account(0, ([127, 0, 0, 1], 31000).into())).is_ok());
    let mut component = Component::new(configured, &profile, crypto(1)).expect("owner-kept account");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: profile };
    let mut above = Queue::with_capacity(MAX_OUT_DOWN.above);
    let mut io = Queue::with_capacity(MAX_OUT_DOWN.io);
    let mut files = Queue::with_capacity(1);
    component.down(&env, Request::Grant { account: 0 }, &mut above, &mut io, &mut files);
    component::refuse_refresh(&mut component, &env, &mut above, &mut io);
    assert_event(
        above.pop(),
        Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Exchange(skein_oauth::Failure::Refused) },
    );
    component.down(&env, Request::Grant { account: 0 }, &mut above, &mut io, &mut files);
    assert_expired(above.pop());
    assert!(above.is_empty() && io.is_empty() && files.is_empty(), "the dead record starts no new refresh");
}

#[test]
fn an_exchange_bound_refuses_a_second_account_before_admitting_its_grant() {
    let mut limits = limits();
    limits.exchanges = 1;
    let mut configured = List::with_capacity(2);
    assert!(
        configured.push(refresh_account(0, core::net::SocketAddr::from(([127, 0, 0, 1], 31000)))).is_ok(),
        "first account"
    );
    assert!(
        configured.push(refresh_account(1, core::net::SocketAddr::from(([127, 0, 0, 1], 31000)))).is_ok(),
        "second account"
    );
    let mut component = Component::new(configured, &limits, crypto(1)).expect("configured component");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut above = Queue::with_capacity(MAX_OUT_DOWN.above);
    let mut io = Queue::with_capacity(MAX_OUT_DOWN.io);
    component.down(&env, Request::Grant { account: 0 }, &mut above, &mut io, &mut Queue::with_capacity(1));
    assert!(above.is_empty() && io.is_empty(), "grant waits for refresh child work");
    component.down(&env, Request::Grant { account: 1 }, &mut above, &mut io, &mut Queue::with_capacity(1));
    assert_refused(above.pop(), 1, Asked::Grant, Refusal::Full { bound: 1 });
    component.down(&env, Request::Abort, &mut above, &mut io, &mut Queue::with_capacity(1));
    component.fire(&env, &mut above, &mut io, &mut Queue::with_capacity(1));
    assert_event(
        above.pop(),
        Event::Failed { account: 0, ends: Ends::Grant, failure: Failure::Exchange(skein_oauth::Failure::Cancelled) },
    );
    assert_event(above.pop(), Event::Closed);
    assert!(io.is_empty(), "unstarted exchange has no socket to abort");
    component.down(
        &env,
        Request::Kept { account: 0, generation: 1, keeping: Keeping::Kept },
        &mut above,
        &mut io,
        &mut Queue::with_capacity(1),
    );
    assert!(above.is_empty() && io.is_empty(), "stale keeper terminal after abort is inert");
    assert!(!component.has_work() && component.next_deadline().is_none(), "abort settled every child");
}

#[test]
fn a_plaintext_endpoint_off_loopback_is_refused_at_startup() {
    let mut configured = List::with_capacity(1);
    assert!(
        configured.push(refresh_account(0, core::net::SocketAddr::from(([192, 0, 2, 1], 31000)))).is_ok(),
        "account"
    );
    assert_eq!(Component::new(configured, &limits(), crypto(1)).err(), Some(Unusable::Plaintext { account: 0 }));
}

#[test]
fn handed_in_accounts_refuse_sign_in_and_redirect_without_child_work() {
    let (mut component, env, mut above) = setup();
    let mut below = Queue::with_capacity(MAX_OUT_DOWN.io);
    component.down(&env, Request::SignIn { account: 0 }, &mut above, &mut below, &mut Queue::with_capacity(1));
    assert_refused(above.pop(), 0, Asked::SignIn, Refusal::Source);
    component.down(
        &env,
        Request::Redirected { account: 0, uri: bytes::copy_of(b"http://localhost:31234/callback?state=a&code=b") },
        &mut above,
        &mut below,
        &mut Queue::with_capacity(1),
    );
    assert_refused(above.pop(), 0, Asked::Redirected, Refusal::Source);
    assert!(below.is_empty());
}

#[test]
fn redirect_uri_is_checked_before_bounded_query_decoding() {
    let limits = limits();
    let registered = b"http://localhost:31234/callback";
    assert!(
        redirect::parse(b"http://localhost:31234/callback?state=a%2Bb&code=c%20d", registered, &limits.client)
            .is_some()
    );
    for uri in [
        b"http://127.0.0.1:31234/callback?state=a&code=b".as_slice(),
        b"http://localhost.:31234/callback?state=a&code=b",
        b"http://localhost:31234/wrong?state=a&code=b",
        b"http://localhost:31234/callback?state=a&state=b&code=c",
        b"http://localhost:31234/callback?state=%xx&code=b",
        b"http://localhost:31234/callback?state=a&code=b#fragment",
    ] {
        assert!(redirect::parse(uri, registered, &limits.client).is_none());
    }
}

#[test]
fn public_sign_in_admission_respects_listener_and_account_bounds() {
    let mut profile = limits();
    profile.listeners = 0;
    let address = core::net::SocketAddr::new(core::net::IpAddr::V4(core::net::Ipv4Addr::LOCALHOST), 31000);
    let mut configured = List::with_capacity(2);
    assert!(configured.push(refresh_account(0, address)).is_ok());
    assert!(configured.push(refresh_account(1, address)).is_ok());
    let mut component = Component::new(configured, &profile, crypto(17)).expect("component");
    let mut env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: profile };
    let mut above = Queue::with_capacity(MAX_OUT_DOWN.above);
    let mut below = Queue::with_capacity(MAX_OUT_DOWN.io);
    component.down(&env, Request::SignIn { account: 0 }, &mut above, &mut below, &mut Queue::with_capacity(1));
    assert_refused(above.pop(), 0, Asked::SignIn, Refusal::Full { bound: 0 });
    assert!(below.is_empty());
    env.limits.listeners = 1;
    let mut configured = List::with_capacity(2);
    assert!(configured.push(refresh_account(0, address)).is_ok());
    assert!(configured.push(refresh_account(1, address)).is_ok());
    component = Component::new(configured, &env.limits, crypto(17)).expect("one listener component");
    component.down(&env, Request::SignIn { account: 0 }, &mut above, &mut below, &mut Queue::with_capacity(1));
    assert!(above.is_empty());
    assert!(below.pop().is_some());
    component.down(&env, Request::SignIn { account: 0 }, &mut above, &mut below, &mut Queue::with_capacity(1));
    assert_refused(above.pop(), 0, Asked::SignIn, Refusal::Busy);
    component.down(&env, Request::SignIn { account: 1 }, &mut above, &mut below, &mut Queue::with_capacity(1));
    assert_refused(above.pop(), 1, Asked::SignIn, Refusal::Full { bound: 1 });
    assert!(below.is_empty());
}

#[path = "private_tests.rs"]
mod private;

fn crypto(n: u8) -> skein_lib::CryptoRng {
    let seed = skein_lib::Secret::new(bytes::copy_of(&[n; 32]));
    skein_lib::CryptoRng::new(&seed, 0).expect("a 32-byte fixture seed")
}

#[test]
fn two_sign_ins_draw_different_states_and_verifiers() {
    let mut random = crypto(0);
    let state = component::draw(&mut random, 24);
    let verifier = component::draw(&mut random, 32);
    assert_ne!(state, component::draw(&mut random, 24));
    assert_ne!(verifier, component::draw(&mut random, 32));
}

#[test]
fn a_verifier_is_43_url_safe_characters() {
    let verifier = component::draw(&mut crypto(0), 32);
    assert_eq!(verifier.len(), 43);
    for byte in verifier.as_ref() {
        assert!(byte.is_ascii_alphanumeric() || b"-_".contains(byte));
    }
}

fn issued_terms() -> skein_oauth::Terms {
    skein_oauth::Terms {
        parameters: Box::new([]),
        openid: None,
        issued_client_id: true,
        varying_port: false,
        resource: None,
        required_scopes: None,
        revocation_endpoint: None,
        sign_in_again: Box::new([]),
    }
}

#[test]
fn a_name_no_live_account_holds_is_refused() {
    const MISSING: u32 = 7;
    let (mut component, env, mut above) = setup();
    for (request, asked) in [
        (Request::SignIn { account: MISSING }, Asked::SignIn),
        (
            Request::Redirected { account: MISSING, uri: bytes::copy_of(b"https://service.example/callback") },
            Asked::Redirected,
        ),
        (Request::HandIn { account: MISSING, record: tests_record(0) }, Asked::HandIn),
    ] {
        component.down(
            &env,
            request,
            &mut above,
            &mut Queue::with_capacity(MAX_OUT_DOWN.io),
            &mut Queue::with_capacity(MAX_OUT_DOWN.files),
        );
        assert_refused(above.pop(), MISSING, asked, Refusal::Account);
    }
}

#[test]
fn an_owner_record_without_an_issued_id_is_held_as_none() {
    let mut profile = limits();
    profile.client.document.issued_client_bytes = profile.client.document.client_bytes;
    let mut account = refresh_account(0, ([127, 0, 0, 1], 31000).into());
    match &mut account {
        Account::SignIn { registration, .. } => registration.terms = Some(issued_terms()),
        Account::HandedIn => unreachable!("refresh fixture"),
    }
    let mut configured = List::with_capacity(1);
    assert!(configured.push(account).is_ok());
    let mut component = Component::new(configured, &profile, crypto(1)).expect("issued registration");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: profile };
    let mut above = Queue::with_capacity(MAX_OUT_DOWN.above);
    component.down(
        &env,
        Request::Grant { account: 0 },
        &mut above,
        &mut Queue::with_capacity(MAX_OUT_DOWN.io),
        &mut Queue::with_capacity(MAX_OUT_DOWN.files),
    );
    assert_expired(above.pop());
}
