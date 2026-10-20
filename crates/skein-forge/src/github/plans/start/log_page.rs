//! One conditional or cursor page of the App's log (forge-github.md, 9.3).
use super::{Event, State, Step, api, document, plan};
use crate::github::{header, log, pin, plans::Heard, request::Request};
use skein_lib::List;

#[derive(Debug)]
pub struct LogPage {
    ask: log::Ask,
    state: State,
    rows: List<log::Row>,
    next: Option<log::Cursor>,
    validator: Option<api::Validator>,
    bytes: u64,
    limit: u32,
}
impl LogPage {
    #[must_use]
    pub fn new(ask: log::Ask) -> Self {
        Self { ask, state: State::new(), rows: List::with_capacity(0), next: None, validator: None, bytes: 0, limit: 0 }
    }
    pub fn begin(&mut self, bounds: plan::Bounds) -> Result<Step<log::Listed>, api::Error> {
        self.state.admission()?;
        let width = u32::try_from(size_of::<log::Row>()).or(Err(api::Error::TooLarge))?;
        self.limit = bounds
            .page_size
            .min(bounds.rows)
            .min(pin::PAGE_CAP)
            .min(bounds.answer_bytes.checked_div(width).ok_or(api::Error::TooLarge)?);
        if self.limit == 0 {
            return Err(api::Error::TooLarge);
        }
        self.rows = List::with_capacity(self.limit);
        self.state.sending();
        Ok(Step::send(Request::LogPage { ask: self.ask.clone(), per_page: self.limit }))
    }
    pub fn pending(&mut self) -> Option<plan::Exchange> {
        self.state.request.take()
    }
    pub fn set_pending(&mut self, exchange: plan::Exchange) {
        self.state.request = Some(exchange);
    }
    pub fn heard(&mut self, heard: Heard, bounds: plan::Bounds) -> Result<Option<Step<log::Listed>>, api::Error> {
        self.state.order(&heard)?;
        let result = self.advance(heard, bounds);
        Ok(Step::settle(&mut self.state, result))
    }
    fn advance(&mut self, heard: Heard, bounds: plan::Bounds) -> Result<Option<Step<log::Listed>>, api::Error> {
        match &heard {
            Heard::Head { status: 304, .. } => match self.ask {
                log::Ask::First { validator: Some(_) } => {
                    return Ok(Some(Step::terminal(&mut self.state, Ok(log::Listed::Unchanged))));
                }
                log::Ask::First { validator: None } | log::Ask::Next { .. } => return Err(api::Error::InvalidAnswer),
            },
            Heard::Head { status: 200, head } => {
                if head.cursor_invalid || (head.next && head.cursor.is_none()) {
                    return Err(api::Error::InvalidAnswer);
                }
                self.next.clone_from(&head.cursor);
                self.validator = match self.ask {
                    log::Ask::First { .. } => header::validator(head),
                    log::Ask::Next { .. } => None,
                };
                let cursor = match &self.next {
                    Some(cursor) => cursor.as_bytes().len(),
                    None => 0,
                };
                let validator = match &self.validator {
                    Some(validator) => validator.as_bytes().len(),
                    None => 0,
                };
                self.bytes = u64::try_from(cursor.checked_add(validator).ok_or(api::Error::TooLarge)?)
                    .or(Err(api::Error::TooLarge))?;
                if self.bytes > u64::from(bounds.answer_bytes) {
                    return Err(api::Error::TooLarge);
                }
            }
            Heard::Head { .. } | Heard::Row(_) | Heard::Done(_) | Heard::Failed(_) => {}
        }
        match self.state.hear(heard, 200)? {
            None => Ok(None),
            Some(Event::Row(row)) => {
                let row = match row {
                    document::Row::Log(row) => row,
                    document::Row::Rule(_)
                    | document::Row::Installation(_)
                    | document::Row::Reachable(_)
                    | document::Row::Branch(_)
                    | document::Row::SkippedBranch
                    | document::Row::Issue(_)
                    | document::Row::Comment(_)
                    | document::Row::Pull(_)
                    | document::Row::Context(_)
                    | document::Row::Run(_)
                    | document::Row::Collaborator(_)
                    | document::Row::Remark(_)
                    | document::Row::Review(_)
                    | document::Row::Commit(_)
                    | document::Row::File(_) => return Err(api::Error::InvalidAnswer),
                };
                let mut size = u64::try_from(size_of::<log::Row>()).or(Err(api::Error::TooLarge))?;
                size = size
                    .checked_add(u64::try_from(row.event.len()).or(Err(api::Error::TooLarge))?)
                    .ok_or(api::Error::TooLarge)?;
                if let Some(action) = &row.action {
                    size = size
                        .checked_add(u64::try_from(action.len()).or(Err(api::Error::TooLarge))?)
                        .ok_or(api::Error::TooLarge)?;
                }
                self.bytes = self.bytes.checked_add(size).ok_or(api::Error::TooLarge)?;
                if self.rows.len() >= self.limit
                    || self.bytes > u64::from(bounds.answer_bytes)
                    || self.rows.push(row).is_err()
                {
                    return Err(api::Error::TooLarge);
                }
                Ok(None)
            }
            Some(Event::Done { document, .. }) => {
                if document != document::Document::Rows {
                    return Err(api::Error::InvalidAnswer);
                }
                let rows = core::mem::replace(&mut self.rows, List::with_capacity(0)).into_boxed();
                let page = log::Page { rows, next: self.next.take(), validator: self.validator.take() };
                Ok(Some(Step::terminal(&mut self.state, Ok(log::Listed::Page(page)))))
            }
        }
    }
}
