#![cfg(test)]
use skein_forge::{
    api,
    forgejo::document::Body as Description,
    github::{
        decode::Malformed,
        document::{self, Document, Row},
        header::{Head, RateFields},
        pin,
        plans::{
            Body, Failure, Heard,
            plan::{Bounds, Plan, Step},
            tables::{self, Tables},
        },
        request::Request,
    },
    marker, measure,
};
use skein_lib::{Duration, Time, Wall};

const KEY: api::Repository = api::Repository { forge: 3, repository: 7 };
const WORD: &[u8] = b"example-app";
fn default_bounds() -> Bounds {
    Bounds { inner: 8, page_size: 8, rows: 16, answer_bytes: 4096, body_bytes: 1024 }
}
fn issue(number: u64) -> document::Issue {
    document::Issue {
        number,
        pull: false,
        merged: false,
        state: api::State::Open,
        author: 9,
        labels: Box::from([]),
        updated_at: Wall::from_nanos(2_000_000_000),
        title: Box::from(*b"widgets"),
        body: Description::Kept(Box::from([])),
    }
}
fn comment(id: u64) -> document::Comment {
    document::Comment {
        id,
        author: 10,
        created_at: Some(Wall::from_nanos(1_000_000_000)),
        updated_at: Some(Wall::from_nanos(1_000_000_000)),
        body: Description::Kept(Box::from(*b"hello")),
    }
}
fn repository() -> document::Repository {
    document::Repository {
        id: 7,
        owner: Box::from(*b"acme"),
        owner_id: 8,
        organization: true,
        name: Box::from(*b"widgets"),
        default_branch: Box::from(*b"main"),
        html_url: Box::from(*b"https://example.invalid/acme/widgets"),
        merge: true,
        squash: true,
        rebase: true,
    }
}
fn items(kind: Option<api::Kind>) -> api::Read {
    api::Read::Items { since: Time::from_nanos(u64::MAX), page: 3, kind }
}
fn issues_request(per_page: u32) -> Request {
    Request::Issues {
        owner: Box::from(*b"acme"),
        name: Box::from(*b"widgets"),
        since: Wall::from_nanos(u64::MAX),
        page: 3,
        per_page,
    }
}
fn issue_request() -> Request {
    Request::Issue { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), number: 3 }
}
fn comments_request(page: u32, per_page: u32) -> Request {
    Request::Comments { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), number: 3, page, per_page }
}
struct Call {
    plan: Plan,
    tables: Tables,
    bounds: Bounds,
}
impl Call {
    fn new(read: api::Read, bounds: Bounds, cached: bool) -> Self {
        let mut tables = Tables::new(2, 2);
        if cached {
            tables.keep_repository(KEY, tables::repository(&repository()));
        }
        Self { plan: Plan::new(KEY, api::Op::Read(read)), tables, bounds }
    }
    fn begin(&mut self) -> Result<Step, api::Error> {
        self.plan.begin(self.bounds, WORD, &mut self.tables)
    }
    fn heard(&mut self, heard: Heard) -> Result<Option<Step>, api::Error> {
        self.plan.heard(heard, self.bounds, WORD, &mut self.tables)
    }
    fn head(&mut self, next: bool) {
        let mut head = Head::new();
        head.next = next;
        head.date = Some(Wall::from_nanos(5_000_000_000));
        assert_eq!(self.heard(Heard::Head { status: 200, head }), Ok(None));
    }
    fn done(&mut self, document: Document) -> Result<Option<Step>, api::Error> {
        self.heard(Heard::Done(Body::Document(document)))
    }
    fn row(&mut self, row: Row) -> Result<Option<Step>, api::Error> {
        self.heard(Heard::Row(row))
    }
    fn answer(&mut self) -> api::Answer {
        match self.done(Document::Rows).unwrap().unwrap() {
            Step::Done(Ok(answer)) => answer,
            Step::Send(_) | Step::Done(Err(_)) => panic!("expected successful answer"),
        }
    }
}
fn started(read: api::Read, bounds: Bounds) -> Call {
    let mut call = Call::new(read, bounds, true);
    if let Ok(Step::Send(_)) = call.begin() {
    } else {
        panic!("expected request");
    }
    call.head(false);
    call
}
fn comments_started(bounds: Bounds, after: u64) -> Call {
    let mut call = started(api::Read::Item { number: 3, after }, bounds);
    let size = bounds.page_size.min(bounds.rows).min(bounds.answer_bytes / 96).min(pin::PAGE_CAP);
    assert_eq!(call.done(Document::Issue(issue(3))), Ok(Some(Step::send(comments_request(1, size)))));
    call.head(false);
    call
}
#[expect(clippy::unnecessary_wraps, reason = "verdict assertions distinguish terminal errors from caller order errors")]
fn failed(error: api::Error) -> Result<Option<Step>, api::Error> {
    Ok(Some(Step::Done(Err(error))))
}
fn item_answer(answer: api::Answer) -> (api::Summary, Box<[api::Comment]>, bool) {
    let api::Answer::Item { item, comments, more } = answer else {
        panic!("expected Item");
    };
    (item, comments, more)
}
fn items_answer(answer: api::Answer) -> (Box<[api::Summary]>, bool, Time) {
    let api::Answer::Items { items, more, now } = answer else {
        panic!("expected Items");
    };
    (items, more, now)
}
fn numbers(items: &[api::Summary], expected: &[u64]) {
    assert_eq!(items.len(), expected.len());
    for (item, number) in items.iter().zip(expected) {
        assert_eq!(item.number, *number);
    }
}

#[test]
fn github_items_first_request() {
    for (bounds, per_page) in [
        (Bounds { page_size: 2, ..default_bounds() }, 2),
        (Bounds { rows: 3, ..default_bounds() }, 3),
        (Bounds { answer_bytes: 4 * 128, ..default_bounds() }, 4),
        (Bounds { page_size: 200, rows: 200, answer_bytes: 65536, ..default_bounds() }, pin::PAGE_CAP),
    ] {
        for cached in [false, true] {
            let mut call = Call::new(items(None), bounds, cached);
            if cached {
                assert_eq!(call.begin(), Ok(Step::send(issues_request(per_page))));
            } else {
                assert_eq!(call.begin(), Ok(Step::send(Request::Repository { id: 7 })));
                call.head(false);
                assert_eq!(
                    call.done(Document::Repository(repository())),
                    Ok(Some(Step::send(issues_request(per_page))))
                );
                assert_eq!(call.tables.repository(KEY), Some(&tables::repository(&repository())));
            }
        }
    }
    for bounds in [
        Bounds { page_size: 0, ..default_bounds() },
        Bounds { rows: 0, ..default_bounds() },
        Bounds { answer_bytes: 127, ..default_bounds() },
    ] {
        assert_eq!(Call::new(items(None), bounds, false).begin(), Err(api::Error::TooLarge));
    }
    assert_eq!(
        Call::new(api::Read::Item { number: 3, after: 0 }, Bounds { answer_bytes: 95, ..default_bounds() }, false)
            .begin(),
        Err(api::Error::TooLarge)
    );
}
#[test]
fn github_items_answer() {
    for next in [false, true] {
        // Supply the listing's Link independently of whether its page is full.
        let mut call = {
            let mut c = Call::new(items(None), default_bounds(), true);
            c.begin().unwrap();
            c.head(next);
            c
        };
        let body = marker::encode(WORD, b"key", b"description", 128).unwrap();
        let mut first = issue(1);
        first.body = Description::Kept(body.clone());
        first.labels = Box::from([Box::<[u8]>::from(*b"bug"), Box::from(*b"help")]);
        let mut pull = issue(2);
        pull.pull = true;
        let mut merged = issue(3);
        merged.pull = true;
        merged.merged = true;
        for row in [first, pull, merged] {
            assert_eq!(call.row(Row::Issue(row)), Ok(None));
        }
        let (rows, more, now) = items_answer(call.answer());
        assert_eq!(more, next);
        assert_eq!(now, Time::from_nanos(5_000_000_000));
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (rows[0].number, rows[0].kind, rows[0].state, rows[0].author),
            (1, api::Kind::Issue, api::State::Open, 9)
        );
        assert_eq!(rows[0].labels.as_ref(), [Box::<[u8]>::from(*b"bug"), Box::from(*b"help")]);
        assert_eq!(rows[0].title.as_ref(), b"widgets");
        assert_eq!(rows[0].body, body);
        assert_eq!(rows[0].key, Some(Box::from(*b"key")));
        assert_eq!(rows[0].updated, Time::from_nanos(2_000_000_000));
        assert_eq!((rows[1].kind, rows[1].state), (api::Kind::Pull, api::State::Open));
        assert_eq!((rows[2].kind, rows[2].state), (api::Kind::Pull, api::State::Closed));
        assert_eq!(call.tables.user(KEY.forge, 9), None);
    }
}
#[test]
fn github_items_kind_skips_count_toward_the_page() {
    for third_pull in [false, true] {
        let mut call = started(items(Some(api::Kind::Pull)), Bounds { page_size: 2, ..default_bounds() });
        assert_eq!(call.row(Row::Issue(issue(1))), Ok(None));
        let mut pull = issue(2);
        pull.pull = true;
        assert_eq!(call.row(Row::Issue(pull)), Ok(None));
        let mut third = issue(3);
        third.pull = third_pull;
        assert_eq!(call.row(Row::Issue(third)), failed(api::Error::TooLarge));
    }
    let mut call = started(items(Some(api::Kind::Pull)), default_bounds());
    call.row(Row::Issue(issue(1))).unwrap();
    let mut pull = issue(2);
    pull.pull = true;
    call.row(Row::Issue(pull)).unwrap();
    let (rows, _, _) = items_answer(call.answer());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].number, 2);
}
#[test]
fn github_items_same_second_ordered_by_number() {
    let mut call = started(items(None), default_bounds());
    for number in [2, 9, 3, 5] {
        let mut row = issue(number);
        if number == 2 {
            row.updated_at = Wall::from_nanos(1_000_000_000);
        }
        assert_eq!(call.row(Row::Issue(row)), Ok(None));
    }
    let (rows, _, _) = items_answer(call.answer());
    numbers(&rows, &[2, 3, 5, 9]);
    // A disordered time remains visible for the client to reject.
    let mut call = started(items(None), default_bounds());
    for number in [9, 3, 5, 2] {
        let mut row = issue(number);
        if number == 2 {
            row.updated_at = Wall::from_nanos(1_000_000_000);
        }
        call.row(Row::Issue(row)).unwrap();
    }
    let (rows, _, _) = items_answer(call.answer());
    numbers(&rows, &[3, 5, 9, 2]);
}
#[test]
fn github_items_bounds() {
    for bound in [Bounds { rows: 1, ..default_bounds() }, Bounds { answer_bytes: 269, ..default_bounds() }] {
        let mut call = started(items(None), bound);
        assert_eq!(call.row(Row::Issue(issue(1))), Ok(None));
        assert_eq!(call.row(Row::Issue(issue(2))), failed(api::Error::TooLarge));
    }
    let mut call = started(items(Some(api::Kind::Pull)), Bounds { answer_bytes: 135, ..default_bounds() });
    let mut row = issue(1);
    row.body = Description::Kept(Box::from([b'x'; 4096]));
    assert_eq!(call.row(Row::Issue(row)), Ok(None));
    assert!(items_answer(call.answer()).0.is_empty());
    let mut call = started(items(None), Bounds { answer_bytes: 135, ..default_bounds() });
    call.row(Row::Issue(issue(1))).unwrap();
    assert_eq!(items_answer(call.answer()).0.len(), 1);
}
#[test]
fn github_items_without_date_is_invalid() {
    let mut call = Call::new(items(None), default_bounds(), true);
    call.begin().unwrap();
    assert_eq!(call.heard(Heard::Head { status: 200, head: Head::new() }), failed(api::Error::InvalidAnswer));
    let mut call = started(items(None), default_bounds());
    let (items, more, now) = items_answer(call.answer());
    assert!(items.is_empty());
    assert!(!more);
    assert_eq!(now, Time::from_nanos(5_000_000_000));
}
#[test]
fn github_item_reads_issue_then_comments() {
    let mut call = Call::new(api::Read::Item { number: 3, after: 2 }, default_bounds(), true);
    assert_eq!(call.begin(), Ok(Step::send(issue_request())));
    call.head(false);
    assert_eq!(call.done(Document::Issue(issue(3))), Ok(Some(Step::send(comments_request(1, 8)))));
    call.head(false);
    let marked = marker::encode(WORD, b"key", b"hello", 128).unwrap();
    for id in 1..=8 {
        let mut row = comment(id);
        if id == 3 {
            row.body = Description::Kept(marked.clone());
        }
        if id == 4 {
            row.updated_at = Some(Wall::from_nanos(2_000_000_000));
        }
        if id == 5 {
            row.created_at = None;
        }
        if id == 6 {
            row.body = Description::Long(17);
        }
        if id == 7 {
            row.updated_at = Some(Wall::EPOCH);
        }
        if id == 8 {
            row.updated_at = None;
        }
        assert_eq!(call.row(Row::Comment(row)), Ok(None));
    }
    let (item, comments, more) = item_answer(call.answer());
    assert_eq!((item.number, item.author, item.updated), (3, 9, Time::from_nanos(2_000_000_000)));
    assert_eq!(item.title.as_ref(), b"widgets");
    assert!(!more);
    assert_eq!(comments.len(), 6);
    for (comment, id) in comments.iter().zip([3, 4, 5, 6, 7, 8]) {
        assert_eq!(comment.id, id);
    }
    for (index, provenance) in [
        api::Provenance::Original,
        api::Provenance::Revised,
        api::Provenance::Unknown,
        api::Provenance::Unknown,
        api::Provenance::Unknown,
        api::Provenance::Unknown,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(comments[index].provenance, provenance);
        assert_eq!(comments[index].author, 10);
    }
    assert_eq!(comments[0].body, marked);
    assert_eq!(comments[0].key, Some(Box::from(*b"key")));
    assert_eq!(comments[0].created, Time::from_nanos(1_000_000_000));
    assert_eq!(comments[2].created, Time::ZERO);
    assert_eq!(comments[0].revision, u64::from_be_bytes(skein_lib::sha256::digest(&marked)[..8].try_into().unwrap()));
    assert_eq!(comments[1].revision, u64::from_be_bytes(skein_lib::sha256::digest(b"hello")[..8].try_into().unwrap()));
    let mut long = skein_lib::Writer::new(16);
    long.put(&17u64.to_be_bytes()).unwrap();
    long.put(&1_000_000_000u64.to_be_bytes()).unwrap();
    let long = long.finish();
    assert_eq!(comments[3].revision, u64::from_be_bytes(skein_lib::sha256::digest(&long)[..8].try_into().unwrap()));
    assert!(comments[3].body.is_empty());
    assert_eq!(comments[3].key, None);
    let mut call = started(api::Read::Item { number: 3, after: 0 }, default_bounds());
    assert_eq!(call.done(Document::Issue(issue(4))), failed(api::Error::InvalidAnswer));
    let mut call = Call::new(api::Read::Item { number: 3, after: 0 }, default_bounds(), false);
    assert_eq!(call.begin(), Ok(Step::send(Request::Repository { id: 7 })));
    call.head(false);
    assert_eq!(call.done(Document::Repository(repository())), Ok(Some(Step::send(issue_request()))));
}
#[test]
fn github_item_pages_by_link() {
    let bounds = Bounds { page_size: 2, ..default_bounds() };
    // Finish a skipped first page with a next link.
    let mut call = {
        let mut c = started(api::Read::Item { number: 3, after: 2 }, bounds);
        c.done(Document::Issue(issue(3))).unwrap();
        c.head(true);
        c
    };
    for id in [1, 2] {
        assert_eq!(call.row(Row::Comment(comment(id))), Ok(None));
    }
    assert_eq!(call.done(Document::Rows), Ok(Some(Step::send(comments_request(2, 2)))));
    call.head(false);
    assert_eq!(call.row(Row::Comment(comment(3))), Ok(None));
    let (_, comments, more) = item_answer(call.answer());
    assert!(!more);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].id, 3);
}
#[test]
fn github_item_answer_full_sets_more() {
    for bounds in [
        Bounds { rows: 2, page_size: 2, ..default_bounds() },
        Bounds { answer_bytes: 200, page_size: 1, ..default_bounds() },
    ] {
        let mut call = started(api::Read::Item { number: 3, after: 0 }, bounds);
        call.done(Document::Issue(issue(3))).unwrap();
        call.head(true);
        let size = bounds.page_size.min(bounds.rows).min(bounds.answer_bytes / 96);
        for id in 1..=u64::from(size) {
            assert_eq!(call.row(Row::Comment(comment(id))), Ok(None));
        }
        assert_eq!(call.done(Document::Rows), Ok(Some(Step::send(comments_request(2, size)))));
        call.head(true);
        for id in 1..=u64::from(size) {
            assert_eq!(call.row(Row::Comment(comment(id + u64::from(size)))), Ok(None));
        }
        let (_, comments, more) = item_answer(call.answer());
        assert!(more);
        assert_eq!(comments.len(), usize::try_from(size).unwrap());
    }
    // The byte cut ignores the later rows within this page and never follows Link.
    let bounds = Bounds { page_size: 3, answer_bytes: 288, ..default_bounds() };
    let mut call = started(api::Read::Item { number: 3, after: 0 }, bounds);
    call.done(Document::Issue(issue(3))).unwrap();
    call.head(true);
    for id in 1..=3 {
        let mut row = comment(id);
        if id == 2 {
            row.body = Description::Kept(Box::from([b'x'; 200]));
        }
        assert_eq!(call.row(Row::Comment(row)), Ok(None));
    }
    let (_, comments, more) = item_answer(call.answer());
    assert!(more);
    assert_eq!(comments.len(), 1);
    let bounds = Bounds { rows: 1, ..default_bounds() };
    let mut call = comments_started(bounds, 0);
    call.row(Row::Comment(comment(1))).unwrap();
    assert_eq!(call.row(Row::Comment(comment(2))), failed(api::Error::TooLarge));
    let mut call = started(api::Read::Item { number: 3, after: 0 }, Bounds { answer_bytes: 96, ..default_bounds() });
    let mut row = issue(3);
    row.body = Description::Kept(Box::from([b'x'; 90]));
    assert_eq!(call.done(Document::Issue(row)), failed(api::Error::TooLarge));
}
#[test]
fn github_item_failures() {
    for at_comments in [false, true] {
        let mut call = Call::new(api::Read::Item { number: 3, after: 0 }, default_bounds(), true);
        call.begin().unwrap();
        if at_comments {
            call.head(false);
            call.done(Document::Issue(issue(3))).unwrap();
        }
        assert_eq!(call.heard(Heard::Head { status: 404, head: Head::new() }), failed(api::Error::Missing));
        assert_eq!(call.tables.repository(KEY), None);
    }
    let mut call = Call::new(api::Read::Item { number: 3, after: 0 }, default_bounds(), true);
    call.begin().unwrap();
    let mut head = Head::new();
    head.date = Some(Wall::from_nanos(1_000_000_000));
    head.rate = Some(RateFields { limit: 5000, remaining: 0, reset: Wall::from_nanos(3_000_000_000) });
    assert_eq!(call.heard(Heard::Head { status: 403, head }), Ok(None));
    assert_eq!(call.heard(Heard::Done(Body::Empty)), failed(api::Error::RateLimited { after: Duration::from_secs(2) }));
    for (failure, error) in [(Failure::Unseen, api::Error::Unavailable), (Failure::Seen, api::Error::Timeout)] {
        let mut call = comments_started(default_bounds(), 0);
        assert_eq!(call.heard(Heard::Failed(failure)), failed(error));
    }
    for (body, error) in [
        (Body::Malformed(Malformed::Shape { field: b"id" }), api::Error::InvalidAnswer),
        (Body::Malformed(Malformed::Bound { field: b"labels" }), api::Error::TooLarge),
        (Body::Empty, api::Error::InvalidAnswer),
        (Body::Raw { bytes: Box::from([]), more: false }, api::Error::InvalidAnswer),
        (Body::Document(Document::Issue(issue(3))), api::Error::InvalidAnswer),
    ] {
        let mut call = comments_started(default_bounds(), 0);
        assert_eq!(call.heard(Heard::Done(body)), failed(error));
    }
    let mut call = Call::new(items(None), default_bounds(), true);
    assert_eq!(call.done(Document::Rows), Err(api::Error::InvalidAnswer));
    call.begin().unwrap();
    assert_eq!(call.done(Document::Rows), Err(api::Error::InvalidAnswer));
    call.head(false);
    assert_eq!(call.heard(Heard::Head { status: 200, head: Head::new() }), Err(api::Error::InvalidAnswer));
    call.answer();
    assert_eq!(call.done(Document::Rows), Err(api::Error::InvalidAnswer));
    assert_eq!(measure::SUMMARY_BYTES, 128);
    assert_eq!(measure::COMMENT_BYTES, 96);
}
