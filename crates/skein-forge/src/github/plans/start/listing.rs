//! Every page of the App's installations (forge-github.md, 8.3).
use super::{Event, State, Step, add, api, capacity, document, plan};
use crate::github::{pin, plans::Heard, request::Request};
use alloc::boxed::Box;
use skein_lib::List;

#[derive(Debug)]
pub struct Listing {
    state: State,
    page: u32,
    rows: List<document::Installation>,
    bytes: u32,
    count: u32,
}
impl Listing {
    #[must_use]
    #[expect(clippy::new_without_default, reason = "a plan begins explicitly; the programming model excludes Default")]
    pub fn new() -> Self {
        Self { state: State::new(), page: 1, rows: List::with_capacity(0), bytes: 0, count: 0 }
    }
    pub fn begin(&mut self, bounds: plan::Bounds) -> Result<Step<Box<[document::Installation]>>, api::Error> {
        self.state.admission()?;
        // Zero capacity still permits an empty listing; its first row is TooLarge.
        self.rows = List::with_capacity(capacity(bounds)?);
        Ok(self.send())
    }
    pub fn pending(&mut self) -> Option<plan::Exchange> {
        self.state.request.take()
    }
    pub fn set_pending(&mut self, exchange: plan::Exchange) {
        self.state.request = Some(exchange);
    }
    fn send(&mut self) -> Step<Box<[document::Installation]>> {
        self.state.sending();
        self.count = 0;
        Step::send(Request::Installations { page: self.page })
    }
    /// Call-order errors are `Err`; exchange verdicts finish the plan.
    pub fn heard(
        &mut self,
        heard: Heard,
        bounds: plan::Bounds,
    ) -> Result<Option<Step<Box<[document::Installation]>>>, api::Error> {
        self.state.order(&heard)?;
        let result = self.advance(heard, bounds);
        Ok(Step::settle(&mut self.state, result))
    }
    fn advance(
        &mut self,
        heard: Heard,
        bounds: plan::Bounds,
    ) -> Result<Option<Step<Box<[document::Installation]>>>, api::Error> {
        match self.state.hear(heard, 200)? {
            None => Ok(None),
            Some(Event::Row(row)) => {
                let row = match row {
                    document::Row::Installation(row) => row,
                    document::Row::Reachable(_)
                    | document::Row::Branch(_)
                    | document::Row::Issue(_)
                    | document::Row::Pull(_)
                    | document::Row::Context(_)
                    | document::Row::Run(_)
                    | document::Row::Comment(_) => return Err(api::Error::InvalidAnswer),
                };
                self.count = self.count.checked_add(1).ok_or(api::Error::TooLarge)?;
                if self.count > pin::PAGE_CAP || self.rows.len() >= bounds.rows {
                    return Err(api::Error::TooLarge);
                }
                self.bytes = add(self.bytes, &row.login, b"", bounds)?;
                match self.rows.push(row) {
                    Ok(()) => Ok(None),
                    Err(_) => Err(api::Error::TooLarge),
                }
            }
            Some(Event::Done { document, next }) => match document {
                document::Document::Rows => {
                    if next {
                        self.page = self.page.checked_add(1).ok_or(api::Error::TooLarge)?;
                        return Ok(Some(self.send()));
                    }
                    let rows = core::mem::replace(&mut self.rows, List::with_capacity(0));
                    Ok(Some(Step::terminal(&mut self.state, Ok(rows.into_boxed()))))
                }
                document::Document::Installation(_)
                | document::Document::Minted(_)
                | document::Document::Viewer(_)
                | document::Document::Graphql(_)
                | document::Document::App(_)
                | document::Document::HookConfig(_)
                | document::Document::Conversion(_)
                | document::Document::BotUser(_)
                | document::Document::Repository(_)
                | document::Document::Branch(_)
                | document::Document::Pull(_)
                | document::Document::Combined(_)
                | document::Document::Issue(_)
                | document::Document::Error(_) => Err(api::Error::InvalidAnswer),
            },
        }
    }
}
