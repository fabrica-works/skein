use skein_forge::{
    api::{self, Answer as Value, Error, Op, Read},
    github::redirect::Origin,
};
use skein_forge_connection::{
    Access, Event, Fact, Request,
    settings::{Kind, LogOrigin},
};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    github::{self, Fake},
    machine::{Answer, Machine, Pause},
};
use skein_lib::{Duration, List, Secret};

const SECOND: Duration = Duration::from_secs(1);
const SEED: u64 = 7;
const REPO: api::Repository = api::Repository { forge: FORGE, repository: 2 };
const LOCATE: &str = "GET /repos/acme/widgets";
const JOB: &str = "GET /repos/acme/widgets/actions/jobs/1";
const LOG: &str = "GET /repos/acme/widgets/actions/jobs/1/logs";
const BLOB: &str = "GET /logs/2/1/1";
const THIRD: &[u8] = b"http://third.example.test:3002";
// Startup: two installation pages, Full, Contents, Viewer; then locate.
const READY_REQUESTS: usize = 6;
// Job metadata, API log redirect, and the blob GET are one call.
const FOLLOW_COST: u32 = 3;
const REFUSED_COST: u32 = 2;
// The fake's signed target lives for 600 wall seconds (fake-forge.md, 10.1).
const SIGNATURE_LIFE: Duration = Duration::from_secs(600);
// The failing build fixture's complete plaintext log, including setup and completion.
const WHOLE_LOG: &[u8] = b"set up build\nfailing check build 0\nfailing check build 1\nfailing check build 2\nfailing check build 3\njob build failed\n";
const TAIL_BYTES: u32 = 8;

fn origin(host: &[u8], port: u16) -> Origin {
    Origin { tls: false, host: Box::from(host), port }
}
fn world(third_origin: Option<&[u8]>, blob_redirect: Option<&[u8]>, calls: &[&'static str]) -> Machine {
    let mut settings = fixtures::settings();
    let forge = settings.forges.get_mut(0).expect("GitHub fixture");
    forge.kind = Kind::GitHub;
    forge.access = Some(Access {
        credential: skein_forge::credential::Credential::App { id: 7, key: Secret::new(Box::from(github::app_key())) },
        user: 9,
        webhook_secret: None,
    });
    forge.log_origins = List::with_capacity(1);
    forge
        .log_origins
        .push(LogOrigin {
            origin: origin(b"logs.example.test", github::BLOB_ADDRESS.port()),
            address: skein_io::Addr::Ip(github::BLOB_ADDRESS),
        })
        .expect("one configured blob origin");
    let script = [
        "GET /app/installations?per_page=100&page=1",
        "GET /app/installations?per_page=100&page=2",
        "POST /app/installations/1/access_tokens",
        "POST /app/installations/1/access_tokens",
        "POST /graphql",
        LOCATE,
    ]
    .into_iter()
    .chain(calls.iter().copied())
    .map(Answer::served)
    .collect();
    let mut world = Machine::github(SEED, fixtures::limits(), settings, script);
    *world.github_fake_mut() = Fake::job_logs(SEED, third_origin, blob_redirect);
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    world.run_for(SECOND);
    world.call(
        1,
        api::Repository { repository: 0, ..REPO },
        Op::Read(Read::Locate { owner: Box::from(b"acme".as_slice()), name: Box::from(b"widgets".as_slice()) }),
    );
    world.run_for(SECOND);
    world
}
fn attempt() -> api::JobAttempt {
    // The first repository's initial commit starts the fixture's first run and job.
    let mut head = [0; 32];
    head[..8].copy_from_slice(&1_u64.to_be_bytes());
    api::JobAttempt { head, run: 1, job: 1, attempt: 1 }
}
fn read(world: &mut Machine, call: u64, max_bytes: u32) {
    world.call(call, REPO, Op::Read(Read::Job { attempt: attempt(), max_bytes }));
    world.run_for(SECOND);
}
fn terminal(world: &Machine, call: u64, cost: u32, expected: &Result<Value, Error>) {
    let answer = world.answered(call).expect("job terminal");
    assert_eq!((answer.cost, &answer.result), (cost, expected));
    assert_eq!(world.deliveries().iter().filter(|a| a.call.raw() == call).count(), 1, "one terminal per call");
}
fn finish(world: &mut Machine, requests: usize) {
    assert_eq!(world.requests().len(), requests);
    assert!(world.script_done());
    assert!(world.close());
    assert_eq!(world.closed_events(), 1);
}

#[test]
fn github_job_log_follows_once_and_keeps_whole_or_tail() {
    let mut w = world(None, None, &[JOB, LOG, BLOB, JOB, LOG, BLOB]);
    for (call, cap, log, truncated) in
        [(2, u32::try_from(WHOLE_LOG.len()).unwrap(), WHOLE_LOG, false), (3, TAIL_BYTES, b" failed\n".as_slice(), true)]
    {
        read(&mut w, call, cap);
        terminal(
            &w,
            call,
            FOLLOW_COST,
            &Ok(Value::Job { attempt: attempt(), step: None, log: Box::from(log), truncated }),
        );
    }
    assert_eq!(w.blob_records().len(), 2);
    assert!(w.facts().is_empty());
    finish(&mut w, READY_REQUESTS + 2 * usize::try_from(FOLLOW_COST).unwrap());
}

#[test]
fn github_blob_carries_neither_credential_nor_range() {
    let mut w = world(None, None, &[JOB, LOG, BLOB]);
    read(&mut w, 2, TAIL_BYTES);
    // Calibrate on the API requests from this same job call, without printing values.
    for request in &w.requests()[READY_REQUESTS..READY_REQUESTS + 2] {
        assert!(request.split(|b| *b == b'\n').any(|line| line.starts_with(b"Authorization: Bearer ")));
    }
    assert_eq!(w.blob_records().len(), 1);
    for record in w.blob_records() {
        assert!(record.path.starts_with(b"/logs/2/1/1?expires="));
        assert!(!record.authorization);
        assert!(!record.range);
    }
    terminal(
        &w,
        2,
        FOLLOW_COST,
        &Ok(Value::Job { attempt: attempt(), step: None, log: Box::from(b" failed\n".as_slice()), truncated: true }),
    );
    finish(&mut w, READY_REQUESTS + usize::try_from(FOLLOW_COST).unwrap());
}

#[test]
fn github_unconfigured_log_origin_is_reported_once_per_connect() {
    let mut w = world(Some(THIRD), None, &[JOB, LOG, JOB, LOG]);
    for call in [2, 3] {
        read(&mut w, call, TAIL_BYTES);
        terminal(&w, call, REFUSED_COST, &Err(Error::Unavailable));
        let [Event::Fact { forge, fact: Fact::Origin { origin: reported } }] = w.facts() else {
            panic!("one origin fact for both reads")
        };
        assert_eq!(*forge, FORGE);
        assert_eq!(*reported, origin(b"third.example.test", 3002));
    }
    assert!(w.blob_records().is_empty());
    finish(&mut w, READY_REQUESTS + 2 * usize::try_from(REFUSED_COST).unwrap());
}

#[test]
fn github_unconfigured_log_origin_is_reported_again_after_connect() {
    // Five start reads recur at connect; locate stays cached for this repository.
    const RECONNECT_REQUESTS: usize = 5;
    const RECONNECT: &[&str] = &[
        JOB,
        LOG,
        JOB,
        LOG,
        "GET /app/installations?per_page=100&page=1",
        "GET /app/installations?per_page=100&page=2",
        "POST /app/installations/1/access_tokens",
        "POST /app/installations/1/access_tokens",
        "POST /graphql",
        JOB,
        LOG,
        JOB,
        LOG,
    ];
    // The new access generation starts with a refusal, then the same fixture App.
    let mut w = world(Some(THIRD), None, RECONNECT);
    for call in [2, 3] {
        read(&mut w, call, TAIL_BYTES);
        terminal(&w, call, REFUSED_COST, &Err(Error::Unavailable));
    }
    let mut invalid = fixtures::github_access();
    invalid.credential = skein_forge::credential::Credential::App { id: 7, key: Secret::new(Box::from([])) };
    w.action(Request::Connect { forge: FORGE, access: invalid });
    w.action(Request::Connect { forge: FORGE, access: fixtures::github_access() });
    w.run_for(SECOND);
    for call in [4, 5] {
        read(&mut w, call, TAIL_BYTES);
        terminal(&w, call, REFUSED_COST, &Err(Error::Unavailable));
    }
    assert_eq!(w.connected_forges(), &[FORGE, FORGE]);
    assert_eq!(w.facts().len(), 2);
    for fact in w.facts() {
        assert!(matches!(fact, Event::Fact { forge: FORGE, fact: Fact::Origin { origin: reported } }
            if *reported == origin(b"third.example.test", 3002)));
    }
    assert!(w.blob_records().is_empty());
    finish(&mut w, READY_REQUESTS + RECONNECT_REQUESTS + 4 * usize::try_from(REFUSED_COST).unwrap());
}

#[test]
fn github_blob_redirect_is_unavailable_without_another_follow() {
    // A redirect back to the configured blob origin makes an accidental follow observable.
    let mut w = world(None, Some(github::BLOB_ORIGIN), &[JOB, LOG, BLOB]);
    read(&mut w, 2, TAIL_BYTES);
    terminal(&w, 2, FOLLOW_COST, &Err(Error::Unavailable));
    assert_eq!(w.blob_records().len(), 1);
    assert!(w.facts().is_empty());
    finish(&mut w, READY_REQUESTS + usize::try_from(FOLLOW_COST).unwrap());
}

#[test]
fn github_expired_blob_signature_maps_to_forbidden() {
    let mut w = world(None, None, &[JOB, LOG, BLOB]);
    // Hold the fake's signed 302 before the client sees it; jump the world's wall
    // by its signature lifetime while the call's monotonic deadline remains open.
    w.pause_at(READY_REQUESTS + 1, Pause::Reading(0));
    read(&mut w, 2, TAIL_BYTES);
    assert!(w.answered(2).is_none());
    w.jump_wall(SIGNATURE_LIFE);
    w.release_response();
    w.run_for(SECOND);
    terminal(&w, 2, FOLLOW_COST, &Err(Error::Forbidden));
    assert_eq!(w.blob_records().len(), 1);
    finish(&mut w, READY_REQUESTS + usize::try_from(FOLLOW_COST).unwrap());
}
