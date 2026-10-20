use super::{fixtures as f, read_support as r};
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::github::{self, failure, plans::tables::Method, request::Request};
use skein_lib::Secret;

fn harness(queue: bool) -> f::Harness {
    let config = r::harness().env.limits;
    let mut h = f::Harness::with_protection(
        f::limits(),
        f::config(),
        config,
        Some(a::Protection {
            queue,
            branch: f::boxed(b"trunk"),
            contexts: Box::from([]),
            approvals: 0,
            dismiss_stale: false,
        }),
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
    let message = f::parts(&[failure::RULES, b"\n", failure::QUEUE]);
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
fn queue_only_merge_handler_leaves_ordinary_merges_to_the_rest_writes_task() {
    let (mut h, number, head, token) = scenario(false);
    let response = ask(&mut h, &token, &merge(number, head));
    f::refusal(&h, &response, 404, b"Not Found");
}

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
                assert_eq!(requests.as_slice(), [b"repository".as_slice(), b"merge", b"pull", b"enqueue"]);
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
        requests.push(label).unwrap();
        let mut events = r::exchange_events(h, &exchange, token);
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
        assert_eq!(drive_merge(&mut h, &token, number, head, Admission::Answer), Ok(skein_forge::api::Answer::Queued));
    }
    assert_eq!(h.domain.queue_entries(b"acme/widgets").unwrap(), [a::QueueEntry { number, head, position: 1 }]);
}

#[test]
fn merge_plan_observes_a_head_moved_between_the_pull_query_and_enqueue_as_stale() {
    let (mut h, number, head, token) = scenario(true);
    assert_eq!(drive_merge(&mut h, &token, number, head, Admission::MoveHead), Err(skein_forge::api::Error::Stale));
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
}

#[test]
fn lost_enqueue_answer_recovery_and_a_repeated_merge_keep_one_admission() {
    let (mut h, number, head, token) = scenario(true);
    assert_eq!(drive_merge(&mut h, &token, number, head, Admission::LoseAnswer), Err(skein_forge::api::Error::Timeout));
    let skein_forge::api::Answer::Pull(pull) =
        r::read_with_empty_ci(&mut h, &token, skein_forge::api::Read::Pull { number }).unwrap()
    else {
        panic!("recovery pull")
    };
    assert_eq!(pull.queued, Some(1));
    assert_eq!(pull.commit, r::object(head));
    assert_eq!(pull.merged, None);
    // Recovery reads adopt the queued entry. A fresh Merge call is idempotent too.
    assert_eq!(drive_merge(&mut h, &token, number, head, Admission::Answer), Ok(skein_forge::api::Answer::Queued));
    assert_eq!(h.domain.queue_entries(b"acme/widgets").unwrap(), [a::QueueEntry { number, head, position: 1 }]);
}
