#![cfg(test)]
use skein_forge::{
    api,
    github::{
        decode::{self, Malformed},
        document::{self, Kind, Row},
        failure::SECONDARY,
        header::{self, Head, RateFields},
        limits::Limits,
        pin,
        plans::{
            Body, Failure, Heard,
            plan::{Bounds, Exchange},
            start::{Identity, InstallationPage, Installations, Listing, Minting, Page, Reachable, Step},
        },
        request::{self, Auth, Method, Request, Scope},
        server,
    },
    measure,
};
use skein_json::{tokenizer, writer};
use skein_lib::{
    Duration, Env, Intake, List, Queue, Secret, Time, Wall, Writer,
    stream::{Down, Up},
};

// The same tiny document/token bounds as the dialect's fixtures.
const DOCUMENT_BYTES: u32 = 4096;
const STRING_BYTES: u32 = 128;
const INSTALLATION: u64 = 7;
const ACCOUNT: u64 = 9;
const TOKEN: &[u8] = b"example-app-token-sentinel";
// One picker page requested at the task's example size.
const PICKER_SIZE: u32 = 20;

fn limits() -> Limits {
    Limits {
        json: tokenizer::Limits { depth: 10, string: STRING_BYTES, number: 32, chunk: 7, length: DOCUMENT_BYTES },
        writer: writer::Limits { depth: 10, length: DOCUMENT_BYTES },
        string_bytes: STRING_BYTES,
        body_bytes: STRING_BYTES,
        inner: 4,
        target_bytes: 1024,
    }
}
fn bounds() -> Bounds {
    Bounds { page_size: 2, rows: 4, answer_bytes: 256, body_bytes: DOCUMENT_BYTES }
}
fn picker_bounds() -> Bounds {
    Bounds { page_size: PICKER_SIZE, rows: PICKER_SIZE, ..bounds() }
}
fn installation(id: u64) -> document::Installation {
    document::Installation {
        id,
        login: Box::from(*b"acme"),
        account: ACCOUNT,
        kind: document::Account::Organization,
        created: Wall::from_nanos(1_000_000_000),
        suspended: false,
    }
}
fn repository(id: u64) -> document::Reachable {
    document::Reachable { id, owner: ACCOUNT, login: Box::from(*b"acme"), name: Box::from(*b"widgets"), private: true }
}
fn minted() -> document::Minted {
    document::Minted { token: Secret::new(Box::from(TOKEN)), expires: Wall::from_nanos(3_600_000_000_000) }
}
fn viewer() -> document::Viewer {
    document::Viewer { id: ACCOUNT, login: Box::from(*b"example-app[bot]") }
}
fn output() -> Writer {
    Writer::new(usize::try_from(DOCUMENT_BYTES).unwrap())
}
fn bytes(mut out: Writer) -> Box<[u8]> {
    let length = out.written();
    for _ in 0..out.room() {
        out.put(b" ").unwrap();
    }
    Box::from(&out.finish()[..length])
}
// Copied from github/tests/fixtures::decode: every plan document crosses tokens.
fn decode(kind: Kind, body: &[u8]) -> (List<Row>, Body) {
    let limits = limits();
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut decoder = decode::decoder(kind, &limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).unwrap().max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).unwrap();
    let mut rows = List::with_capacity(pin::PAGE_CAP + 1);
    tokenizer::down(&mut tokenizer, &env, tokenizer::Request::Next, &mut above, &mut below);
    for _ in 0..body.len().saturating_mul(8).saturating_add(64) {
        if let Some(event) = above.pop() {
            let (demand, step) = decoder.step(event);
            match step {
                decode::Step::Demand => {}
                decode::Step::Row(row) => rows.push(row).unwrap(),
                decode::Step::Done(document) => return (rows, Body::Document(document)),
                decode::Step::Malformed(error) => return (rows, Body::Malformed(error)),
            }
            tokenizer::down(&mut tokenizer, &env, demand, &mut above, &mut below);
        } else {
            match below.pop().expect("tokenizer demand") {
                Down::Demand { read, room: 0 } => {
                    let delivery = match intake.meet(read) {
                        Some(bytes) => Up::Bytes(bytes),
                        None => Up::End,
                    };
                    tokenizer::up(&mut tokenizer, &env, delivery, &mut above, &mut below);
                }
                Down::Demand { .. } | Down::Send(_) | Down::Finish => panic!("JSON only demands input"),
            }
        }
    }
    panic!("decoder did not settle within input's step bound")
}
fn installation_page(ids: &[u64]) -> (List<Row>, Body) {
    let mut rows = List::with_capacity(u32::try_from(ids.len()).unwrap());
    for id in ids {
        rows.push(installation(*id)).unwrap();
    }
    let mut out = output();
    server::installations(rows.as_slice(), &mut out).unwrap();
    decode(Kind::InstallationRows, &bytes(out))
}
fn reachable_page(ids: &[u64]) -> (List<Row>, Body) {
    let mut rows = List::with_capacity(u32::try_from(ids.len()).unwrap());
    for id in ids {
        rows.push(repository(*id)).unwrap();
    }
    let mut out = output();
    server::reachable(rows.as_slice(), u64::try_from(ids.len()).unwrap(), &mut out).unwrap();
    decode(Kind::ReachableRows, &bytes(out))
}
fn mint_body() -> Body {
    let mut out = output();
    server::minted(&minted(), &mut out).unwrap();
    decode(Kind::Minted, &bytes(out)).1
}
fn viewer_body() -> Body {
    let mut out = output();
    server::viewer(&viewer(), &mut out).unwrap();
    decode(Kind::Viewer, &bytes(out)).1
}
fn refusal_body(message: &[u8]) -> Body {
    let mut out = output();
    server::refusal(message, &[], &mut out).unwrap();
    decode(Kind::Error, &bytes(out)).1
}
fn response(status: u16, next: bool, request: &Request) -> Heard {
    let mut head = Head::new();
    if next {
        let target = request::encode(request, &limits()).unwrap().target;
        let mut out = output();
        server::link(&target, 2, 3, &mut out).unwrap();
        header::field(&mut head, b"Link", &bytes(out), &limits());
    }
    Heard::Head { status, head }
}
fn listing_head(plan: &mut Listing, next: bool, bounds: Bounds) {
    assert_eq!(plan.heard(response(200, next, &Request::Installations { page: 1 }), bounds), Ok(None));
}
fn installations_head(plan: &mut Installations, next: bool, bounds: Bounds) {
    assert_eq!(plan.heard(response(200, next, &Request::Installations { page: 2 }), bounds), Ok(None));
}
fn installations_plan(next: bool, bounds: Bounds) -> Installations {
    let mut plan = Installations::new(2);
    plan.begin(bounds).unwrap();
    installations_head(&mut plan, next, bounds);
    plan
}
fn reachable_head(plan: &mut Reachable, next: bool, bounds: Bounds) {
    let request = Request::Reachable { page: 2, per_page: bounds.page_size };
    assert_eq!(plan.heard(response(200, next, &request), bounds), Ok(None));
}
fn listing_page(
    plan: &mut Listing,
    ids: &[u64],
    bounds: Bounds,
) -> Result<Option<Step<Box<[document::Installation]>>>, api::Error> {
    let (rows, body) = installation_page(ids);
    for row in rows.into_boxed() {
        let result = plan.heard(Heard::Row(row), bounds);
        if result != Ok(None) {
            return result;
        }
    }
    plan.heard(Heard::Done(body), bounds)
}
fn installations_page(
    plan: &mut Installations,
    ids: &[u64],
    bounds: Bounds,
) -> Result<Option<Step<InstallationPage>>, api::Error> {
    let (rows, body) = installation_page(ids);
    for row in rows.into_boxed() {
        let result = plan.heard(Heard::Row(row), bounds);
        if result != Ok(None) {
            return result;
        }
    }
    plan.heard(Heard::Done(body), bounds)
}
fn picker_page(plan: &mut Reachable, ids: &[u64], bounds: Bounds) -> Result<Option<Step<Page>>, api::Error> {
    let (rows, body) = reachable_page(ids);
    for row in rows.into_boxed() {
        let result = plan.heard(Heard::Row(row), bounds);
        if result != Ok(None) {
            return result;
        }
    }
    plan.heard(Heard::Done(body), bounds)
}

// Failure and ordering tables erase only successful payloads, not plan events.
#[derive(Debug)]
enum TestPlan {
    Installations(Installations),
    Listing(Listing),
    Minting(Minting),
    Identity(Identity),
    Reachable(Reachable),
}
fn plans() -> [TestPlan; 5] {
    [
        TestPlan::Installations(Installations::new(2)),
        TestPlan::Listing(Listing::new()),
        TestPlan::Minting(Minting::new(INSTALLATION, Scope::Full)),
        TestPlan::Identity(Identity::new()),
        TestPlan::Reachable(Reachable::new(1, 2)),
    ]
}
fn erase<T>(step: Step<T>) -> Step<()> {
    match step {
        Step::Send(exchange) => Step::Send(exchange),
        Step::Done(Ok(_)) => Step::Done(Ok(())),
        Step::Done(Err(error)) => Step::Done(Err(error)),
    }
}
fn erase_heard<T>(result: Result<Option<Step<T>>, api::Error>) -> Result<Option<Step<()>>, api::Error> {
    match result {
        Ok(Some(step)) => Ok(Some(erase(step))),
        Ok(None) => Ok(None),
        Err(error) => Err(error),
    }
}
impl TestPlan {
    fn begin(&mut self) -> Result<Step<()>, api::Error> {
        match self {
            Self::Installations(plan) => Ok(erase(plan.begin(bounds())?)),
            Self::Listing(plan) => Ok(erase(plan.begin(bounds())?)),
            Self::Minting(plan) => Ok(erase(plan.begin(bounds())?)),
            Self::Identity(plan) => Ok(erase(plan.begin(bounds())?)),
            Self::Reachable(plan) => Ok(erase(plan.begin(bounds())?)),
        }
    }
    fn heard(&mut self, heard: Heard) -> Result<Option<Step<()>>, api::Error> {
        match self {
            Self::Installations(plan) => erase_heard(plan.heard(heard, bounds())),
            Self::Listing(plan) => erase_heard(plan.heard(heard, bounds())),
            Self::Minting(plan) => erase_heard(plan.heard(heard, bounds())),
            Self::Identity(plan) => erase_heard(plan.heard(heard, bounds())),
            Self::Reachable(plan) => erase_heard(plan.heard(heard, bounds())),
        }
    }
    fn status(&self) -> u16 {
        match self {
            Self::Minting(_) => 201,
            Self::Installations(_) | Self::Listing(_) | Self::Identity(_) | Self::Reachable(_) => 200,
        }
    }
    fn head(&mut self) {
        let status = self.status();
        assert_eq!(self.heard(Heard::Head { status, head: Head::new() }), Ok(None));
    }
    fn pending(&mut self) -> Option<Exchange> {
        match self {
            Self::Installations(plan) => plan.pending(),
            Self::Listing(plan) => plan.pending(),
            Self::Minting(plan) => plan.pending(),
            Self::Identity(plan) => plan.pending(),
            Self::Reachable(plan) => plan.pending(),
        }
    }
    fn set_pending(&mut self, exchange: Exchange) {
        match self {
            Self::Installations(plan) => plan.set_pending(exchange),
            Self::Listing(plan) => plan.set_pending(exchange),
            Self::Minting(plan) => plan.set_pending(exchange),
            Self::Identity(plan) => plan.set_pending(exchange),
            Self::Reachable(plan) => plan.set_pending(exchange),
        }
    }
}

#[test]
fn github_start_listing_follows_every_page() {
    let mut plan = Listing::new();
    let first = Exchange {
        request: Request::Installations { page: 1 },
        retention: None,
        comparison: skein_forge::github::plans::plan::Comparison::Full,
    };
    assert_eq!(plan.begin(bounds()), Ok(Step::Send(first.clone())));
    assert_eq!(plan.pending(), None);
    plan.set_pending(first.clone());
    assert_eq!(plan.pending(), Some(first));
    assert_eq!(plan.pending(), None);
    listing_head(&mut plan, true, bounds());
    let second = Exchange {
        request: Request::Installations { page: 2 },
        retention: None,
        comparison: skein_forge::github::plans::plan::Comparison::Full,
    };
    assert_eq!(listing_page(&mut plan, &[INSTALLATION], bounds()), Ok(Some(Step::Send(second.clone()))));
    let encoded = request::encode(&second.request, &limits()).unwrap();
    assert_eq!(encoded.target.as_ref(), b"/app/installations?per_page=100&page=2");
    assert_eq!(encoded.auth, Auth::App);
    plan.set_pending(second.clone());
    assert_eq!(plan.pending(), Some(second));
    listing_head(&mut plan, false, bounds());
    assert_eq!(
        listing_page(&mut plan, &[INSTALLATION + 1], bounds()),
        Ok(Some(Step::Done(Ok(Box::from([installation(INSTALLATION), installation(INSTALLATION + 1)])))))
    );
    // Every plan uses caller-retained pending, including taking it only once.
    for mut plan in plans() {
        let Step::Send(exchange) = plan.begin().unwrap() else { panic!("request") };
        assert_eq!(plan.pending(), None);
        plan.set_pending(exchange.clone());
        assert_eq!(plan.pending(), Some(exchange));
        assert_eq!(plan.pending(), None);
    }
}

#[test]
fn github_start_listing_empty() {
    // A zero row/byte capacity still admits an empty first page.
    for bounds in [bounds(), Bounds { rows: 0, answer_bytes: 0, ..bounds() }] {
        let mut plan = Listing::new();
        plan.begin(bounds).unwrap();
        listing_head(&mut plan, false, bounds);
        assert_eq!(listing_page(&mut plan, &[], bounds), Ok(Some(Step::Done(Ok(Box::from([]))))));
    }
}

#[test]
fn github_start_listing_bound_by_rows() {
    let bounds = Bounds { rows: 1, ..bounds() };
    let mut plan = Listing::new();
    plan.begin(bounds).unwrap();
    listing_head(&mut plan, true, bounds);
    assert_eq!(
        listing_page(&mut plan, &[INSTALLATION], bounds),
        Ok(Some(Step::send(Request::Installations { page: 2 })))
    );
    listing_head(&mut plan, false, bounds);
    assert_eq!(listing_page(&mut plan, &[INSTALLATION + 1], bounds), Ok(Some(Step::Done(Err(api::Error::TooLarge)))));
    assert_eq!(plan.heard(Heard::Done(installation_page(&[]).1), bounds), Err(api::Error::InvalidAnswer));
    // The fixed installation page cap is independent of the total answer bound.
    let roomy = Bounds { rows: pin::PAGE_CAP + 1, answer_bytes: 4096, ..bounds };
    let row = installation_page(&[INSTALLATION]).0.into_boxed()[0].clone();
    let mut plan = Listing::new();
    plan.begin(roomy).unwrap();
    listing_head(&mut plan, false, roomy);
    for _ in 0..pin::PAGE_CAP {
        assert_eq!(plan.heard(Heard::Row(row.clone()), roomy), Ok(None));
    }
    assert_eq!(plan.heard(Heard::Row(row), roomy), Ok(Some(Step::Done(Err(api::Error::TooLarge)))));
}

#[test]
fn github_start_installations_one_page() {
    for next in [true, false] {
        let bounds = Bounds { rows: 2, ..bounds() };
        let mut plan = Installations::new(2);
        let request = Request::Installations { page: 2 };
        assert_eq!(plan.begin(bounds), Ok(Step::send(request.clone())));
        let encoded = request::encode(&request, &limits()).unwrap();
        assert_eq!(encoded.target.as_ref(), b"/app/installations?per_page=100&page=2");
        assert_eq!(encoded.auth, Auth::App);
        assert_eq!(server::route(encoded.method, &encoded.target, &limits()), Some(request));
        installations_head(&mut plan, next, bounds);
        assert_eq!(
            installations_page(&mut plan, &[INSTALLATION, INSTALLATION + 1], bounds),
            Ok(Some(Step::Done(Ok(InstallationPage {
                rows: Box::from([installation(INSTALLATION), installation(INSTALLATION + 1)]),
                next,
            }))))
        );
        assert_eq!(plan.heard(Heard::Done(installation_page(&[]).1), bounds), Err(api::Error::InvalidAnswer));
    }
}

#[test]
fn github_start_installations_empty() {
    for bounds in [bounds(), Bounds { rows: 0, answer_bytes: 0, ..bounds() }] {
        for next in [true, false] {
            let mut plan = installations_plan(next, bounds);
            assert_eq!(
                installations_page(&mut plan, &[], bounds),
                Ok(Some(Step::Done(Ok(InstallationPage { rows: Box::from([]), next }))))
            );
        }
    }
}

#[test]
fn github_start_installations_refuses_page_zero() {
    let mut plan = Installations::new(0);
    assert_eq!(plan.begin(bounds()), Err(api::Error::Refused));
    assert_eq!(plan.pending(), None);
    assert_eq!(plan.heard(Heard::Failed(Failure::Unseen), bounds()), Err(api::Error::InvalidAnswer));
}

#[test]
fn github_start_installations_bound_by_rows() {
    let bounds = Bounds { rows: 1, ..bounds() };
    let mut plan = installations_plan(false, bounds);
    assert_eq!(
        installations_page(&mut plan, &[INSTALLATION, INSTALLATION + 1], bounds),
        Ok(Some(Step::Done(Err(api::Error::TooLarge))))
    );
}

#[test]
fn github_start_installations_first_row_exceeds_zero_bounds() {
    for bounds in [Bounds { rows: 0, ..bounds() }, Bounds { answer_bytes: 0, ..bounds() }] {
        let mut plan = installations_plan(false, bounds);
        assert_eq!(
            installations_page(&mut plan, &[INSTALLATION], bounds),
            Ok(Some(Step::Done(Err(api::Error::TooLarge))))
        );
    }
}

#[test]
fn github_start_installations_bound_by_page_cap() {
    // The fixed page cap is independent of the total answer bound.
    let roomy = Bounds { rows: pin::PAGE_CAP + 1, answer_bytes: DOCUMENT_BYTES, ..bounds() };
    let mut plan = installations_plan(false, roomy);
    let row = Row::Installation(installation(INSTALLATION));
    for _ in 0..pin::PAGE_CAP {
        assert_eq!(plan.heard(Heard::Row(row.clone()), roomy), Ok(None));
    }
    assert_eq!(plan.heard(Heard::Row(row), roomy), Ok(Some(Step::Done(Err(api::Error::TooLarge)))));
}

#[test]
fn github_start_installations_answer_bytes_bound() {
    // Two acme logins, with one STRING_BYTES overhead per row.
    let page_bytes = (u32::try_from(measure::STRING_BYTES).unwrap() + 4) * 2;
    for (answer_bytes, expected) in [
        (
            page_bytes,
            Ok(InstallationPage {
                rows: Box::from([installation(INSTALLATION), installation(INSTALLATION + 1)]),
                next: false,
            }),
        ),
        (page_bytes - 1, Err(api::Error::TooLarge)),
    ] {
        let bounds = Bounds { answer_bytes, ..bounds() };
        let mut plan = installations_plan(false, bounds);
        assert_eq!(
            installations_page(&mut plan, &[INSTALLATION, INSTALLATION + 1], bounds),
            Ok(Some(Step::Done(expected)))
        );
    }
}

#[test]
fn github_start_installations_rejects_reachable_row() {
    let mut plan = installations_plan(false, bounds());
    let row = Row::Reachable(repository(INSTALLATION));
    assert_eq!(plan.heard(Heard::Row(row), bounds()), Ok(Some(Step::Done(Err(api::Error::InvalidAnswer)))));
}

#[test]
fn github_start_installations_rejects_non_rows_document() {
    let mut plan = installations_plan(false, bounds());
    assert_eq!(plan.heard(Heard::Done(viewer_body()), bounds()), Ok(Some(Step::Done(Err(api::Error::InvalidAnswer)))));
}

#[test]
fn github_start_minting_requests() {
    for (scope, body) in
        [(Scope::Full, None), (Scope::Contents, Some(br#"{"permissions":{"contents":"write"}}"#.as_slice()))]
    {
        let mut plan = Minting::new(INSTALLATION, scope);
        let request = Request::Mint { installation: INSTALLATION, scope };
        assert_eq!(plan.begin(bounds()), Ok(Step::send(request.clone())));
        let encoded = request::encode(&request, &limits()).unwrap();
        assert_eq!(encoded.target.as_ref(), b"/app/installations/7/access_tokens");
        assert_eq!(encoded.method, Method::Post);
        assert_eq!(encoded.auth, Auth::App);
        assert_eq!(encoded.body.as_deref(), body);
    }
}

#[test]
fn github_start_minting_answers_201() {
    let mut plan = Minting::new(INSTALLATION, Scope::Full);
    // Whole documents do not use the plans' row/answer bounds.
    let tiny = Bounds { rows: 0, answer_bytes: 0, ..bounds() };
    plan.begin(tiny).unwrap();
    assert_eq!(plan.heard(Heard::Head { status: 201, head: Head::new() }, tiny), Ok(None));
    assert_eq!(plan.heard(Heard::Done(mint_body()), tiny), Ok(Some(Step::Done(Ok(minted())))));
    for status in [200, 202, 204, 299] {
        let mut plan = Minting::new(INSTALLATION, Scope::Full);
        plan.begin(bounds()).unwrap();
        assert_eq!(
            plan.heard(Heard::Head { status, head: Head::new() }, bounds()),
            Ok(Some(Step::Done(Err(api::Error::InvalidAnswer))))
        );
    }
}

#[test]
#[expect(clippy::disallowed_macros, reason = "the redaction test inspects Debug output without logging the token")]
fn github_start_minting_keeps_the_secret() {
    let mut plan = Minting::new(INSTALLATION, Scope::Full);
    plan.begin(bounds()).unwrap();
    plan.heard(Heard::Head { status: 201, head: Head::new() }, bounds()).unwrap();
    let step = plan.heard(Heard::Done(mint_body()), bounds()).unwrap();
    for printed in [format!("{step:?}"), format!("{plan:?}")] {
        assert!(skein_lib::bytes::find(printed.as_bytes(), TOKEN).is_none(), "token is redacted");
    }
}

#[test]
fn github_start_identity_reads_the_viewer() {
    let mut plan = Identity::new();
    let tiny = Bounds { rows: 0, answer_bytes: 0, ..bounds() };
    assert_eq!(plan.begin(tiny), Ok(Step::send(Request::Viewer)));
    assert_eq!(plan.heard(Heard::Head { status: 200, head: Head::new() }, tiny), Ok(None));
    assert_eq!(plan.heard(Heard::Done(viewer_body()), tiny), Ok(Some(Step::Done(Ok(viewer())))));
    let mut out = output();
    server::graphql_refusal(b"FORBIDDEN", b"example-app refused", &mut out).unwrap();
    let mut plan = Identity::new();
    plan.begin(bounds()).unwrap();
    plan.heard(Heard::Head { status: 200, head: Head::new() }, bounds()).unwrap();
    assert_eq!(
        plan.heard(Heard::Done(decode(Kind::Viewer, &bytes(out)).1), bounds()),
        Ok(Some(Step::Done(Err(api::Error::InvalidAnswer))))
    );
}

#[test]
fn github_start_reachable_one_page() {
    for next in [true, false] {
        let mut plan = Reachable::new(2, PICKER_SIZE);
        let request = Request::Reachable { page: 2, per_page: PICKER_SIZE };
        assert_eq!(plan.begin(picker_bounds()), Ok(Step::send(request.clone())));
        let encoded = request::encode(&request, &limits()).unwrap();
        assert_eq!(encoded.target.as_ref(), b"/installation/repositories?per_page=20&page=2");
        assert_eq!(encoded.auth, Auth::Installation);
        assert_eq!(server::route(encoded.method, &encoded.target, &limits()), Some(request));
        reachable_head(&mut plan, next, picker_bounds());
        assert_eq!(
            picker_page(&mut plan, &[2, 3], picker_bounds()),
            Ok(Some(Step::Done(Ok(Page { rows: Box::from([repository(2), repository(3)]), next }))))
        );
        assert_eq!(plan.heard(Heard::Done(reachable_page(&[]).1), picker_bounds()), Err(api::Error::InvalidAnswer));
    }
}

#[test]
fn github_start_reachable_admission() {
    for (page, size, bounds, error) in [
        (0, 1, bounds(), api::Error::Refused),
        (1, 0, bounds(), api::Error::TooLarge),
        (
            1,
            pin::PAGE_CAP + 1,
            Bounds { page_size: pin::PAGE_CAP + 1, rows: pin::PAGE_CAP + 1, ..bounds() },
            api::Error::TooLarge,
        ),
        (1, 3, bounds(), api::Error::TooLarge),
        (1, 2, Bounds { rows: 1, ..bounds() }, api::Error::TooLarge),
    ] {
        let mut plan = Reachable::new(page, size);
        assert_eq!(plan.begin(bounds), Err(error));
        assert_eq!(plan.pending(), None);
        assert_eq!(plan.heard(Heard::Failed(Failure::Unseen), bounds), Err(api::Error::InvalidAnswer));
    }
    for size in [0, pin::PAGE_CAP + 1] {
        assert_eq!(
            request::encode(&Request::Reachable { page: 1, per_page: size }, &limits()),
            Err(request::Refusal::Text)
        );
    }
    let cap = Bounds { page_size: pin::PAGE_CAP, rows: pin::PAGE_CAP, ..bounds() };
    assert_eq!(
        Reachable::new(1, pin::PAGE_CAP).begin(cap),
        Ok(Step::send(Request::Reachable { page: 1, per_page: pin::PAGE_CAP }))
    );
}

#[test]
fn github_start_reachable_page_past_its_size() {
    let mut plan = Reachable::new(1, 1);
    plan.begin(bounds()).unwrap();
    reachable_head(&mut plan, false, bounds());
    assert_eq!(picker_page(&mut plan, &[2, 3], bounds()), Ok(Some(Step::Done(Err(api::Error::TooLarge)))));
    let mut plan = Reachable::new(1, 1);
    plan.begin(bounds()).unwrap();
    reachable_head(&mut plan, false, bounds());
    assert_eq!(
        picker_page(&mut plan, &[2], bounds()),
        Ok(Some(Step::Done(Ok(Page { rows: Box::from([repository(2)]), next: false }))))
    );
}

#[test]
fn github_start_refusal_statuses() {
    for (status, error) in [
        (401, api::Error::Forbidden),
        (404, api::Error::Missing),
        (410, api::Error::Missing),
        (500, api::Error::Timeout),
        (503, api::Error::Timeout),
        (301, api::Error::Unavailable),
        (307, api::Error::Unavailable),
        (418, api::Error::InvalidAnswer),
        (202, api::Error::InvalidAnswer),
    ] {
        for mut plan in plans() {
            plan.begin().unwrap();
            assert_eq!(plan.heard(Heard::Head { status, head: Head::new() }), Ok(Some(Step::Done(Err(error)))));
            assert_eq!(plan.heard(Heard::Failed(Failure::Seen)), Err(api::Error::InvalidAnswer));
        }
    }
}

#[test]
fn github_start_rate_limit_and_forbidden() {
    let mut primary = Head::new();
    primary.date = Some(Wall::from_nanos(100_000_000_000));
    primary.rate = Some(RateFields { limit: 5000, remaining: 0, reset: Wall::from_nanos(107_000_000_000) });
    let mut retry = Head::new();
    header::field(&mut retry, b"Retry-After", b"3", &limits());
    let row = installation_page(&[INSTALLATION]).0.into_boxed()[0].clone();
    for (status, head, body, error) in [
        (403, primary.clone(), Body::Empty, api::Error::RateLimited { after: Duration::from_secs(7) }),
        (403, Head::new(), refusal_body(SECONDARY), api::Error::RateLimited { after: Duration::from_secs(60) }),
        (403, retry.clone(), Body::Empty, api::Error::RateLimited { after: Duration::from_secs(3) }),
        (403, Head::new(), refusal_body(b"example-app suspended"), api::Error::Forbidden),
        (429, retry, Body::Empty, api::Error::RateLimited { after: Duration::from_secs(3) }),
        // The read map has no special case for a bare 429 without limit evidence.
        (429, Head::new(), Body::Empty, api::Error::InvalidAnswer),
    ] {
        for mut plan in plans() {
            plan.begin().unwrap();
            assert_eq!(plan.heard(Heard::Head { status, head: head.clone() }), Ok(None));
            assert_eq!(plan.heard(Heard::Row(row.clone())), Ok(None), "refusal rows ignored");
            assert_eq!(plan.heard(Heard::Done(body.clone())), Ok(Some(Step::Done(Err(error)))));
        }
    }
    for (head, error) in
        [(primary, api::Error::RateLimited { after: Duration::from_secs(7) }), (Head::new(), api::Error::Forbidden)]
    {
        for failure in [Failure::Unseen, Failure::Seen] {
            for mut plan in plans() {
                plan.begin().unwrap();
                assert_eq!(plan.heard(Heard::Head { status: 403, head: head.clone() }), Ok(None));
                assert_eq!(plan.heard(Heard::Failed(failure)), Ok(Some(Step::Done(Err(error)))));
            }
        }
    }
}

#[test]
fn github_start_transport_failures() {
    for (failure, error) in [(Failure::Unseen, api::Error::Unavailable), (Failure::Seen, api::Error::Timeout)] {
        for with_head in [false, true] {
            for mut plan in plans() {
                plan.begin().unwrap();
                if with_head {
                    plan.head();
                }
                assert_eq!(plan.heard(Heard::Failed(failure)), Ok(Some(Step::Done(Err(error)))));
            }
        }
    }
}

#[test]
fn github_start_malformed_body() {
    let mut out = output();
    server::minted(&minted(), &mut out).unwrap();
    let body = bytes(out);
    let truncated = decode(Kind::Minted, &body[..body.len() - 1]).1;
    let Body::Malformed(_) = &truncated else { panic!("a truncated real body is malformed") };
    for (body, error) in [
        (Body::Malformed(Malformed::Shape { field: b"id" }), api::Error::InvalidAnswer),
        (Body::Malformed(Malformed::Bound { field: b"rows" }), api::Error::TooLarge),
        (Body::Raw { bytes: Box::from(*b"acme"), more: true }, api::Error::InvalidAnswer),
        (Body::Empty, api::Error::InvalidAnswer),
        (truncated, api::Error::InvalidAnswer),
    ] {
        for mut plan in plans() {
            plan.begin().unwrap();
            plan.head();
            assert_eq!(plan.heard(Heard::Done(body.clone())), Ok(Some(Step::Done(Err(error)))));
        }
    }
}

#[test]
fn github_start_wrong_document_kind() {
    let mut out = output();
    server::installation(&installation(INSTALLATION), &mut out).unwrap();
    let installation_body = decode(Kind::Installation, &bytes(out)).1;
    let installation_row = installation_page(&[INSTALLATION]).0.into_boxed()[0].clone();
    for (mut plan, heard) in [
        (TestPlan::Listing(Listing::new()), Heard::Done(viewer_body())),
        (TestPlan::Minting(Minting::new(INSTALLATION, Scope::Full)), Heard::Done(installation_body)),
        (TestPlan::Identity(Identity::new()), Heard::Done(mint_body())),
        (TestPlan::Reachable(Reachable::new(1, 2)), Heard::Row(installation_row)),
    ] {
        plan.begin().unwrap();
        plan.head();
        assert_eq!(plan.heard(heard), Ok(Some(Step::Done(Err(api::Error::InvalidAnswer)))));
        assert_eq!(plan.heard(Heard::Failed(Failure::Seen)), Err(api::Error::InvalidAnswer));
    }
}

#[test]
fn github_start_call_order() {
    let row = installation_page(&[INSTALLATION]).0.into_boxed()[0].clone();
    let body = viewer_body();
    for mut plan in plans() {
        let status = plan.status();
        let head = Heard::Head { status, head: Head::new() };
        for heard in [head.clone(), Heard::Row(row.clone()), Heard::Done(body.clone()), Heard::Failed(Failure::Unseen)]
        {
            assert_eq!(plan.heard(heard), Err(api::Error::InvalidAnswer));
        }
        plan.begin().unwrap();
        assert_eq!(plan.begin(), Err(api::Error::InvalidAnswer));
        assert_eq!(plan.heard(Heard::Row(row.clone())), Err(api::Error::InvalidAnswer));
        assert_eq!(plan.heard(Heard::Done(body.clone())), Err(api::Error::InvalidAnswer));
        plan.head();
        assert_eq!(plan.heard(head.clone()), Err(api::Error::InvalidAnswer));
        assert_eq!(plan.heard(Heard::Failed(Failure::Seen)), Ok(Some(Step::Done(Err(api::Error::Timeout)))));
        assert_eq!(plan.begin(), Err(api::Error::InvalidAnswer));
        for heard in [head, Heard::Row(row.clone()), Heard::Done(body.clone()), Heard::Failed(Failure::Unseen)] {
            assert_eq!(plan.heard(heard), Err(api::Error::InvalidAnswer));
        }
    }
}

#[test]
fn github_start_answer_bytes_bound() {
    let width = u32::try_from(measure::STRING_BYTES).unwrap();
    // acme is four bytes, widgets seven: one overhead per row, not per string.
    let installation_bytes = width + 4;
    let repository_bytes = width + 4 + 7;
    for (answer_bytes, expected) in [
        (installation_bytes * 2, Ok(Box::<[_]>::from([installation(INSTALLATION), installation(INSTALLATION + 1)]))),
        (installation_bytes * 2 - 1, Err(api::Error::TooLarge)),
    ] {
        let bounds = Bounds { answer_bytes, ..bounds() };
        let mut plan = Listing::new();
        plan.begin(bounds).unwrap();
        listing_head(&mut plan, true, bounds);
        listing_page(&mut plan, &[INSTALLATION], bounds).unwrap();
        listing_head(&mut plan, false, bounds);
        assert_eq!(listing_page(&mut plan, &[INSTALLATION + 1], bounds), Ok(Some(Step::Done(expected))));
    }
    for (answer_bytes, expected) in [
        (repository_bytes * 2, Ok(Page { rows: Box::from([repository(2), repository(3)]), next: false })),
        (repository_bytes * 2 - 1, Err(api::Error::TooLarge)),
    ] {
        let bounds = Bounds { answer_bytes, ..bounds() };
        let mut plan = Reachable::new(1, 2);
        plan.begin(bounds).unwrap();
        reachable_head(&mut plan, false, bounds);
        assert_eq!(picker_page(&mut plan, &[2, 3], bounds), Ok(Some(Step::Done(expected))));
    }
}
