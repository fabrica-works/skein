//! The dialect's plans against the in-process fake; no transport or live forge.
use super::fixtures as f;
use crate::service::Response;
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::{
    api,
    github::{
        self,
        document::Kind,
        header::Head,
        plans::{
            Body, Heard,
            plan::{Bounds, Step},
            reads::Plan,
            tables::Tables,
        },
        request::Request,
    },
};
use skein_lib::{List, ReplyTo, Secret, Token};

pub fn harness() -> f::Harness {
    let mut config = f::domain_config();
    config.limits.items = 8;
    config.limits.comments = 4;
    config.limits.reviews = 4;
    config.limits.files = 4;
    f::Harness::with_domain(f::limits(), f::config(), config)
}
pub fn call(h: &mut f::Harness, op: a::Op) -> a::Answer {
    let result = h
        .drive(fake::Event::Call {
            reply_to: ReplyTo::new(Token::new(77)),
            user: 1,
            repository: f::boxed(b"acme/widgets"),
            op,
        })
        .expect("scenario call succeeds");
    h.hooks = 0;
    result
}
pub fn issue(h: &mut f::Harness, title: &[u8]) -> u64 {
    let a::Answer::Created(number) = call(
        h,
        a::Op::Write(a::Write::CreateIssue { title: f::boxed(title), body: f::boxed(b"body"), labels: Box::from([]) }),
    ) else {
        panic!("created issue")
    };
    number
}
pub fn token(h: &mut f::Harness) -> Secret {
    f::bearer(h.mint(1, false).token.expose())
}
pub fn bounds() -> Bounds {
    Bounds { page_size: 2, rows: 8, answer_bytes: 8192, body_bytes: 4096 }
}
pub fn events(h: &mut f::Harness, request: &Request, token: &Secret) -> List<Heard> {
    let encoded = github::request::encode(request, &h.limits.github).expect("dialect encodes request");
    let response = h.ask(encoded.method, &encoded.target, Some(token.expose()), encoded.body.as_deref().unwrap_or(b""));
    response_events(h, request, response)
}
fn response_events(h: &f::Harness, request: &Request, response: Response) -> List<Heard> {
    let mut events = List::with_capacity(32);
    let mut head = Head::new();
    head.date = Some(skein_lib::Wall::from_nanos(fake::time(&h.env.limits, h.env.now).as_nanos()));
    if let Some(link) = &response.link {
        github::header::field(&mut head, b"Link", link, &h.limits.github);
    }
    let kind = if response.status == 200 { github::decode::kind(request).expect("JSON read") } else { Kind::Error };
    let (rows, document) = h.decode(kind, &response);
    events.push(Heard::Head { status: response.status, head }).expect("head room");
    for row in rows.into_boxed() {
        events.push(Heard::Row(row)).expect("row room");
    }
    events.push(Heard::Done(Body::Document(document))).expect("terminal room");
    events
}
pub fn read(h: &mut f::Harness, token: &Secret, read: api::Read) -> Result<api::Answer, api::Error> {
    let mut plan = Plan::new(api::Repository { forge: 1, repository: 2 }, read);
    let mut tables = Tables::new(4, 8);
    let mut step = plan.begin(bounds(), b"example-app", &mut tables).expect("begin read plan");
    for _exchange in 0..64 {
        let exchange = match step {
            Step::Done(result) => return result,
            Step::Send(exchange) => exchange,
        };
        let events = events(h, &exchange.request, token);
        let mut next = None;
        for event in events.into_boxed() {
            next = plan.heard(event, bounds(), b"example-app", &mut tables).expect("ordered exchange");
            if next.is_some() {
                break;
            }
        }
        step = next.expect("exchange advances plan");
    }
    panic!("plan settles within fixture bound")
}
