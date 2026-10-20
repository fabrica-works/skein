//! Write plans use the same in-process exchanges and readback as read plans.
use super::{fixtures as f, read_support as r};
use skein_forge::{
    api,
    github::plans::{plan::Step, tables::Tables, writes::Plan},
};
use skein_lib::Secret;

pub fn harness() -> f::Harness {
    let mut h = r::harness();
    skein_fake_forge_domain::grant(
        &mut h.domain,
        b"acme/widgets",
        f::BOT,
        skein_fake_forge_domain::api::Permission::Write,
    );
    h
}
pub fn write(h: &mut f::Harness, token: &Secret, write: api::Write) -> Result<api::Answer, api::Error> {
    let mut plan = Plan::new(api::Repository { forge: 1, repository: 2 }, write);
    let mut tables = Tables::new(4, 8);
    let mut step = plan.begin(r::bounds(), b"example-app", &mut tables).expect("begin write");
    for _exchange in 0..32_u32 {
        let exchange = match step {
            Step::Done(result) => return result,
            Step::Send(exchange) => exchange,
        };
        let events = r::exchange_events(h, &exchange, token);
        let mut next = None;
        for event in events.into_boxed() {
            next = plan.heard(event, r::bounds(), b"example-app", &mut tables).expect("ordered write exchange");
            if next.is_some() {
                break;
            }
        }
        step = next.expect("exchange advances");
    }
    panic!("bounded write plan settles")
}
