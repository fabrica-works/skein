use super::{fixtures as f, read_support as r};
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::github::{self, failure, plans::tables::Method, request::Request};
use skein_lib::Secret;

fn harness(queue: bool) -> f::Harness {
    protected(queue, 0, Box::from([]))
}
fn protected(queue: bool, approvals: u32, contexts: Box<[Box<[u8]>]>) -> f::Harness {
    let config = r::harness().env.limits;
    let mut h = f::Harness::with_protection(
        f::limits(),
        f::config(),
        config,
        Some(a::Protection { queue, branch: f::boxed(b"trunk"), contexts, approvals, dismiss_stale: false }),
    );
    fake::grant(&mut h.domain, b"acme/widgets", f::BOT, a::Permission::Write);
    h
}
fn scenario(queue: bool) -> (f::Harness, u64, u64, Secret) {
    let mut h = harness(queue);
    let (number, head) = r::pull(&mut h);
    let token = r::token(&mut h);
    (h, number, head, token)
}
fn ask(h: &mut f::Harness, token: &Secret, request: &Request) -> crate::service::Response {
    let encoded = github::request::encode(request, &h.limits.github).unwrap();
    h.ask(encoded.method, &encoded.target, Some(token.expose()), encoded.body.as_deref().unwrap_or(b""))
}
fn merge(number: u64, head: u64) -> Request {
    Request::Merge {
        owner: f::boxed(b"acme"),
        name: f::boxed(b"widgets"),
        number,
        head: r::object(head),
        method: Method::Rebase,
    }
}
#[test]
fn direct_merge_of_a_queue_branch_answers_the_two_rule_lines_without_merging() {
    let (mut h, number, head, token) = scenario(true);
    let response = ask(&mut h, &token, &merge(number, head));
    let message = f::parts(&[failure::RULES, b"\n\n", failure::QUEUE, b"\n\n"]);
    f::refusal(&h, &response, 405, &message);
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
    let a::Answer::Pull(pull) = h.domain.inspect(&h.env.limits, b"acme/widgets", &a::Read::Pull { number }).unwrap()
    else {
        panic!("pull")
    };
    assert_eq!(pull.state, a::State::Open);
    assert_eq!(pull.merged, None);
}
#[test]
fn ordinary_base_merges_by_rest_beside_a_queue_base() {
    let (mut h, queued, head, token) = scenario(true);
    let first = h.first;
    r::call(&mut h, a::Op::Write(a::Write::CreateBranch { branch: f::boxed(b"direct"), commit: first }));
    let a::Answer::Created(direct) = r::call(
        &mut h,
        a::Op::Write(a::Write::OpenPull {
            title: f::boxed(b"direct merge"),
            body: f::boxed(b"ordinary base"),
            head: f::boxed(b"feature"),
            base: f::boxed(b"direct"),
            opens_empty: false,
        }),
    ) else {
        panic!("ordinary pull")
    };
    let response = ask(&mut h, &token, &merge(queued, head));
    let message = f::parts(&[failure::RULES, b"\n\n", failure::QUEUE, b"\n\n"]);
    f::refusal(&h, &response, 405, &message);
    let skein_forge::api::Answer::Merged(sha) =
        drive_merge(&mut h, &token, direct, head, Admission::Answer, DIRECT).unwrap()
    else {
        panic!("REST merge returns its sha")
    };
    let a::Answer::Commit(landed) =
        h.domain.inspect(&h.env.limits, b"acme/widgets", &a::Read::Branch { branch: f::boxed(b"direct") }).unwrap()
    else {
        panic!("ordinary base moved")
    };
    assert_eq!(sha, r::object(landed));
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
}

#[test]
fn stale_head_is_refused_before_a_queue_requirement() {
    let (mut h, number, head, token) = scenario(true);
    let response = ask(&mut h, &token, &merge(number, head.checked_sub(1).unwrap()));
    f::refusal(&h, &response, 409, failure::MERGE_STALE);
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
}

#[test]
fn queue_refusal_takes_precedence_over_approval_and_check_rules() {
    let mut h = protected(true, 1, Box::from([f::boxed(b"required")]));
    let (number, head) = r::pull(&mut h);
    let token = r::token(&mut h);
    let response = ask(&mut h, &token, &merge(number, head));
    let message = f::parts(&[failure::RULES, b"\n\n", failure::QUEUE, b"\n\n"]);
    f::refusal(&h, &response, 405, &message);
    assert_eq!(
        drive_merge(&mut h, &token, number, head, Admission::Answer, QUEUED),
        Ok(skein_forge::api::Answer::Queued)
    );
}

// REST returns the sha directly; queue admission adds the node query and mutation.
const DIRECT: &[(&[u8], u16)] = &[(b"repository", 200), (b"merge", 200)];
const QUEUED: &[(&[u8], u16)] = &[(b"repository", 200), (b"merge", 405), (b"pull", 200), (b"enqueue", 200)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Admission {
    Answer,
    MoveHead,
    LoseAnswer,
}

fn drive_merge(
    h: &mut f::Harness,
    token: &Secret,
    number: u64,
    head: u64,
    admission: Admission,
    expected: &[(&[u8], u16)],
) -> Result<skein_forge::api::Answer, skein_forge::api::Error> {
    use skein_forge::{
        api,
        github::plans::{
            Failure, Heard,
            plan::{Plan, Step},
            tables::Tables,
        },
    };
    // Repository resolution and the three merge exchanges are the longest route.
    const EXCHANGES: u32 = 4;
    let mut plan = Plan::new(
        api::Repository { forge: 1, repository: 2 },
        api::Op::Write(api::Write::Merge { number, head: r::object(head) }),
    );
    let mut tables = Tables::new(4, 8);
    let mut step = plan.begin(r::bounds(), b"example-app", &mut tables).unwrap();
    let mut requests = skein_lib::List::with_capacity(EXCHANGES);
    for _exchange in 0..=EXCHANGES {
        let exchange = match step {
            Step::Done(result) => {
                assert_eq!(requests.as_slice(), expected);
                return result;
            }
            Step::Send(exchange) => exchange,
        };
        let label: &[u8] = if let Request::Repository { .. } = &exchange.request {
            b"repository"
        } else if let Request::Merge { .. } = &exchange.request {
            b"merge"
        } else if let Request::PullGraphql { .. } = &exchange.request {
            b"pull"
        } else if let Request::Enqueue { head: expected, .. } = &exchange.request {
            assert_eq!(*expected, r::object(head));
            if admission == Admission::MoveHead {
                fake::advance(&mut h.domain, &h.env, b"acme/widgets", b"feature", b"one", b"moved", 1).unwrap();
            }
            b"enqueue"
        } else {
            panic!("unexpected merge request")
        };
        let response = ask(h, token, &exchange.request);
        requests.push((label, response.status)).unwrap();
        let mut events = r::response_events(h, &exchange.request, response);
        if label == b"enqueue" && admission == Admission::LoseAnswer {
            // The domain admitted it; none of its HTTP answer reaches the plan.
            events.clear();
            events.push(Heard::Failed(Failure::Seen)).unwrap();
        }
        let mut next = None;
        for event in events.into_boxed() {
            next = plan.heard(event, r::bounds(), b"example-app", &mut tables).unwrap();
            if next.is_some() {
                break;
            }
        }
        step = next.expect("exchange advances plan");
    }
    panic!("merge plan settles within its four exchanges")
}

#[test]
fn merge_plan_enters_the_queue_after_rest_refusal_and_duplicate_calls_keep_one_entry() {
    let (mut h, number, head, token) = scenario(true);
    for _attempt in 0..2_u32 {
        assert_eq!(
            drive_merge(&mut h, &token, number, head, Admission::Answer, QUEUED),
            Ok(skein_forge::api::Answer::Queued)
        );
    }
    assert_eq!(h.domain.queue_entries(b"acme/widgets").unwrap(), [a::QueueEntry { number, head, position: 1 }]);
}

#[test]
fn merge_plan_observes_a_head_moved_between_the_pull_query_and_enqueue_as_stale() {
    let (mut h, number, head, token) = scenario(true);
    assert_eq!(
        drive_merge(&mut h, &token, number, head, Admission::MoveHead, QUEUED),
        Err(skein_forge::api::Error::Stale)
    );
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
}

#[test]
fn lost_enqueue_answer_recovery_and_a_repeated_merge_keep_one_admission() {
    let (mut h, number, head, token) = scenario(true);
    assert_eq!(
        drive_merge(&mut h, &token, number, head, Admission::LoseAnswer, QUEUED),
        Err(skein_forge::api::Error::Timeout)
    );
    let skein_forge::api::Answer::Pull(pull) =
        r::read_with_empty_ci(&mut h, &token, skein_forge::api::Read::Pull { number }).unwrap()
    else {
        panic!("recovery pull")
    };
    assert_eq!(pull.queued, Some(1));
    assert_eq!(pull.commit, r::object(head));
    assert_eq!(pull.merged, None);
    // Recovery reads adopt the queued entry. A fresh Merge call is idempotent too.
    assert_eq!(
        drive_merge(&mut h, &token, number, head, Admission::Answer, QUEUED),
        Ok(skein_forge::api::Answer::Queued)
    );
    assert_eq!(h.domain.queue_entries(b"acme/widgets").unwrap(), [a::QueueEntry { number, head, position: 1 }]);
}
