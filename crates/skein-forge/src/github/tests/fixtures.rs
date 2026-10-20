use super::super::{
    decode::{self, Malformed, Step},
    document::{Document, Kind, Row},
    limits::Limits,
    request::Request,
};
use skein_json::{tokenizer, writer};
use skein_lib::{
    Env, Intake, List, Queue, Time, Wall, Writer,
    stream::{Down, Up},
};
// GitHub's depth is 10; this fixture holds the first documents in 4096 bytes.
pub const DOCUMENT_BYTES: u32 = 4096;
pub const STRING_BYTES: u32 = 128;
pub const INNER: u32 = 4;
pub const SHA: &[u8; 40] = b"0123456789abcdef0123456789abcdef01234567";
pub const OTHER_SHA: &[u8; 40] = b"abcdef0123456789abcdef0123456789abcdef01";
pub const REPOSITORY: &[u8] = br#"{"id":18446744073709551615,"owner":{"login":"o","id":9007199254740993,"type":"Organization"},"name":"n","default_branch":"main","html_url":"https://github.com/o/n","allow_merge_commit":true,"allow_squash_merge":false,"allow_rebase_merge":true}"#;
pub const BRANCH: &[u8] = br#"{"name":"feat/x y","commit":{"sha":"0123456789abcdef0123456789abcdef01234567"}}"#;
pub const BRANCHES: &[u8] = br#"[{"name":"main"},{"name":"feat/x y"}]"#;
pub const REFUSAL: &[u8] = br#"{"message":"Repository rule violations found\nChanges must be made through the merge queue","errors":[{"message":"detail"}]}"#;
pub fn limits() -> Limits {
    Limits {
        json: tokenizer::Limits { depth: 10, string: STRING_BYTES, number: 32, chunk: 7, length: DOCUMENT_BYTES },
        writer: writer::Limits { depth: 10, length: DOCUMENT_BYTES },
        string_bytes: STRING_BYTES,
        body_bytes: STRING_BYTES,
        inner: INNER,
        target_bytes: 1024,
    }
}
pub fn repository() -> super::super::document::Repository {
    super::super::document::Repository {
        id: u64::MAX,
        owner: Box::from(*b"o"),
        owner_id: 9_007_199_254_740_993,
        organization: true,
        name: Box::from(*b"n"),
        default_branch: Box::from(*b"main"),
        html_url: Box::from(*b"https://github.com/o/n"),
        merge: true,
        squash: false,
        rebase: true,
    }
}
pub fn branch() -> super::super::document::Branch {
    super::super::document::Branch { name: Box::from(*b"feat/x y"), commit: super::super::id::commit(SHA).unwrap() }
}
pub fn refusal() -> super::super::document::Refusal {
    super::super::document::Refusal {
        message: Box::from(*b"Repository rule violations found\nChanges must be made through the merge queue"),
        errors: Box::from([Box::<[u8]>::from(*b"detail")]),
    }
}
pub fn wall() -> Wall {
    crate::forgejo::time::read(b"2026-10-20T08:00:00Z").unwrap()
}
pub fn decode(kind: Kind, body: &[u8], chunk: u32, limits: &Limits) -> (List<Row>, Result<Document, Malformed>) {
    let mut limits = *limits;
    limits.json.chunk = chunk;
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut decoder = decode::decoder(kind, &limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).unwrap().max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).unwrap();
    let mut rows = List::with_capacity(16);
    tokenizer::down(&mut tokenizer, &env, tokenizer::Request::Next, &mut above, &mut below);
    for _ in 0..body.len().saturating_mul(8).saturating_add(64) {
        if let Some(event) = above.pop() {
            let (demand, step) = decoder.step(event);
            match step {
                Step::Demand => {}
                Step::Row(row) => rows.push(row).unwrap(),
                Step::Done(document) => return (rows, Ok(document)),
                Step::Malformed(error) => return (rows, Err(error)),
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
pub fn bytes(writer: Writer) -> Box<[u8]> {
    let length = writer.written();
    // Test output writers reserve a fixture cap, then are padded so finish is full.
    let mut writer = writer;
    for _ in 0..writer.room() {
        writer.put(b" ").unwrap();
    }
    let bytes = writer.finish();
    skein_lib::bytes::copy_of(&bytes[..length])
}
pub struct TargetCase {
    pub label: &'static str,
    pub request: Request,
    pub target: &'static [u8],
    pub raw: bool,
}
pub fn requests() -> [TargetCase; 25] {
    let owner: Box<[u8]> = Box::from(*b"o");
    let name: Box<[u8]> = Box::from(*b"n");
    let commit = super::super::id::commit(SHA).unwrap();
    let other = super::super::id::commit(OTHER_SHA).unwrap();
    [
        TargetCase { label:"repository",request:Request::Repository { id:u64::MAX },target:b"/repositories/18446744073709551615",raw:false },
        TargetCase { label:"locate",request:Request::Locate { owner:owner.clone(),name:name.clone() },target:b"/repos/o/n",raw:false },
        TargetCase { label:"issues",request:Request::Issues { owner:owner.clone(),name:name.clone(),since:wall(),page:2,per_page:20 },target:b"/repos/o/n/issues?state=all&since=2026-10-20T08%3A00%3A00Z&sort=updated&direction=asc&per_page=20&page=2",raw:false },
        TargetCase { label:"issue",request:Request::Issue { owner:owner.clone(),name:name.clone(),number:3 },target:b"/repos/o/n/issues/3",raw:false },
        TargetCase { label:"comments",request:Request::Comments { owner:owner.clone(),name:name.clone(),number:3,page:2,per_page:20 },target:b"/repos/o/n/issues/3/comments?per_page=20&page=2",raw:false },
        TargetCase { label:"pull",request:Request::Pull { owner:owner.clone(),name:name.clone(),number:3 },target:b"/repos/o/n/pulls/3",raw:false },
        TargetCase { label:"open pulls",request:Request::PullsFor { owner:owner.clone(),name:name.clone(),head:Box::from(*b"b"),base:Box::from(*b"main"),open:true },target:b"/repos/o/n/pulls?state=open&head=o%3Ab&base=main&per_page=1",raw:false },
        TargetCase { label:"all pulls",request:Request::PullsFor { owner:owner.clone(),name:name.clone(),head:Box::from(*b"feat/x y"),base:Box::from(*b"main"),open:false },target:b"/repos/o/n/pulls?state=all&head=o%3Afeat%2Fx%20y&base=main&sort=created&direction=asc&per_page=1",raw:false },
        TargetCase { label:"reviews",request:Request::Reviews { owner:owner.clone(),name:name.clone(),number:3,page:2,per_page:20 },target:b"/repos/o/n/pulls/3/reviews?per_page=20&page=2",raw:false },
        TargetCase { label:"review comments",request:Request::ReviewComments { owner:owner.clone(),name:name.clone(),number:3,review:4,page:2,per_page:20 },target:b"/repos/o/n/pulls/3/reviews/4/comments?per_page=20&page=2",raw:false },
        TargetCase { label:"combined status",request:Request::CombinedStatus { owner:owner.clone(),name:name.clone(),commit,page:2,per_page:20 },target:b"/repos/o/n/commits/0123456789abcdef0123456789abcdef01234567/status?per_page=20&page=2",raw:false },
        TargetCase { label:"check runs",request:Request::CheckRuns { owner:owner.clone(),name:name.clone(),commit,page:2,per_page:20 },target:b"/repos/o/n/commits/0123456789abcdef0123456789abcdef01234567/check-runs?filter=latest&per_page=20&page=2",raw:false },
        TargetCase { label:"job",request:Request::Job { owner:owner.clone(),name:name.clone(),id:4 },target:b"/repos/o/n/actions/jobs/4",raw:false },
        TargetCase { label:"job log",request:Request::JobLog { owner:owner.clone(),name:name.clone(),id:4 },target:b"/repos/o/n/actions/jobs/4/logs",raw:false },
        TargetCase { label:"branch",request:Request::Branch { owner:owner.clone(),name:name.clone(),branch:Box::from(*b"feat/x y") },target:b"/repos/o/n/branches/feat/x%20y",raw:false },
        TargetCase { label:"branches",request:Request::Branches { owner:owner.clone(),name:name.clone(),page:2,per_page:20 },target:b"/repos/o/n/branches?per_page=20&page=2",raw:false },
        TargetCase { label:"files",request:Request::PullFiles { owner:owner.clone(),name:name.clone(),number:3,page:2,per_page:20 },target:b"/repos/o/n/pulls/3/files?per_page=20&page=2",raw:false },
        TargetCase { label:"compare",request:Request::Compare { owner:owner.clone(),name:name.clone(),before:commit,after:other,per_page:20 },target:b"/repos/o/n/compare/0123456789abcdef0123456789abcdef01234567...abcdef0123456789abcdef0123456789abcdef01?per_page=20",raw:false },
        TargetCase { label:"raw",request:Request::Raw { owner:owner.clone(),name:name.clone(),path:Box::from(*b"dir/x y"),commit },target:b"/repos/o/n/contents/dir/x%20y?ref=0123456789abcdef0123456789abcdef01234567",raw:true },
        TargetCase { label:"rules",request:Request::RulesForBranch { owner:owner.clone(),name:name.clone(),branch:Box::from(*b"feat/x y"),page:2 },target:b"/repos/o/n/rules/branches/feat/x%20y?per_page=100&page=2",raw:false },
        TargetCase { label:"ruleset",request:Request::Ruleset { owner:owner.clone(),name:name.clone(),id:4 },target:b"/repos/o/n/rulesets/4",raw:false },
        TargetCase { label:"classic",request:Request::ClassicProtection { owner:owner.clone(),name:name.clone(),branch:Box::from(*b"feat/x y") },target:b"/repos/o/n/branches/feat/x%20y/protection",raw:false },
        TargetCase { label:"collaborators",request:Request::Collaborators { owner:owner.clone(),name:name.clone(),page:2,per_page:20 },target:b"/repos/o/n/collaborators?affiliation=all&per_page=20&page=2",raw:false },
        TargetCase { label:"permission",request:Request::Permission { owner,name,login:Box::from(*b"slug[bot]") },target:b"/repos/o/n/collaborators/slug%5Bbot%5D/permission",raw:false },
        TargetCase { label:"user",request:Request::User { id:9_007_199_254_740_993 },target:b"/user/9007199254740993",raw:false },
    ]
}

/// Exercises the streaming delivery entry points independently of the whole-body driver.
pub fn delivery_decode(
    event: &[u8],
    body: &[u8],
    chunk: u32,
    limits: &Limits,
) -> Result<super::super::Delivery, Malformed> {
    use super::super::delivery;
    let mut limits = *limits;
    limits.json.chunk = chunk;
    let Some(mut decoder) = delivery::decoder(event, &limits) else {
        return Ok(delivery::Delivery::Other);
    };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).unwrap().max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).unwrap();
    tokenizer::down(&mut tokenizer, &env, delivery::first(&decoder), &mut above, &mut below);
    for _ in 0..body.len().saturating_mul(8).saturating_add(64) {
        if let Some(event) = above.pop() {
            match delivery::step(&mut decoder, &limits, &event) {
                delivery::Step::Demand(demand) => tokenizer::down(&mut tokenizer, &env, demand, &mut above, &mut below),
                delivery::Step::Done(value) => return Ok(value),
                delivery::Step::Malformed(error) => return Err(error),
            }
        } else {
            let read = match below.pop().expect("tokenizer demand") {
                Down::Demand { read, room: 0 } => read,
                Down::Demand { .. } | Down::Send(_) | Down::Finish => panic!("JSON only demands input"),
            };
            let event = match intake.meet(read) {
                Some(bytes) => Up::Bytes(bytes),
                None => Up::End,
            };
            tokenizer::up(&mut tokenizer, &env, event, &mut above, &mut below);
        }
    }
    panic!("delivery did not settle within input's step bound")
}

pub fn pull(number: u64) -> super::super::document::Pull {
    super::super::document::Pull {
        number,
        closed: false,
        merged: false,
        merge_commit: None,
        mergeable: true,
        head: Box::from(*b"feature"),
        base: Box::from(*b"main"),
        head_commit: super::super::id::commit(SHA).unwrap(),
        base_commit: super::super::id::commit(OTHER_SHA).unwrap(),
        reviewers: Box::from([9, 10]),
    }
}
pub fn context(check: crate::api::Check) -> super::super::document::Context {
    super::super::document::Context {
        updated: Some(wall()),
        context: Box::from(*b"build"),
        check,
        description: Box::from(*b"widgets built"),
        url: Box::from(*b"https://example.invalid/build/1"),
    }
}
