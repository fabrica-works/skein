//! One bounded page of review comments.
use super::{Bounds, Plan, Read, Request, Response, Stage, Step, api, document};
use alloc::boxed::Box;
use skein_lib::List;
impl Plan {
    pub(super) fn remarks_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Remarks { number, review, page } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request = Request::ReviewComments {
            owner: row.owner.clone(),
            name: row.name.clone(),
            number,
            review,
            page,
            per_page: self.limit,
        };
        self.stage = Stage::Read;
        Ok(self.send(request))
    }
    pub(super) fn remark_row(&mut self, row: document::Row, bounds: Bounds) -> Result<(), api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let document::Row::Remark(row) = row else {
            return Err(api::Error::InvalidAnswer);
        };
        self.page_row(bounds)?;
        let body = match row.body {
            crate::forgejo::document::Body::Kept(body) => body,
            crate::forgejo::document::Body::Long(_) => Box::from([]),
        };
        let remark = api::Remark { id: row.id, author: row.author, path: row.path, line: row.line, body };
        self.keep_bytes(64, &[&remark.path, &remark.body], bounds)?;
        match self.remarks.push(remark) {
            Ok(()) => Ok(()),
            Err(_) => Err(api::Error::TooLarge),
        }
    }
    pub(super) fn page_row(&mut self, bounds: Bounds) -> Result<(), api::Error> {
        self.count = self.count.checked_add(1).ok_or(api::Error::TooLarge)?;
        if self.count > self.limit || self.count > bounds.rows {
            return Err(api::Error::TooLarge);
        }
        Ok(())
    }
    pub(super) fn keep_bytes(&mut self, width: u64, fields: &[&[u8]], bounds: Bounds) -> Result<(), api::Error> {
        let mut size = self.size.checked_add(width).ok_or(api::Error::TooLarge)?;
        for field in fields {
            size = size
                .checked_add(u64::try_from(field.len()).ok().ok_or(api::Error::TooLarge)?)
                .ok_or(api::Error::TooLarge)?;
        }
        if size > u64::from(bounds.answer_bytes) {
            return Err(api::Error::TooLarge);
        }
        self.size = size;
        Ok(())
    }
    pub(super) fn listing_next(&self) -> Result<bool, api::Error> {
        match self.response {
            Response::Body { next } => Ok(next),
            Response::None | Response::Head | Response::Refusal { .. } => Err(api::Error::InvalidAnswer),
        }
    }
    pub(super) fn remarks_done(&mut self, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let more = self.listing_next()?;
        let remarks = core::mem::replace(&mut self.remarks, List::with_capacity(0)).into_boxed();
        self.terminal(api::Answer::Remarks { remarks, more }, bounds)
    }
}

impl Plan {
    pub(super) fn reviews_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Reviews { number, page } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request =
            Request::Reviews { owner: row.owner.clone(), name: row.name.clone(), number, page, per_page: self.limit };
        self.stage = Stage::Read;
        Ok(self.send(request))
    }
    pub(super) fn review_row(&mut self, row: document::Row, bounds: Bounds, word: &[u8]) -> Result<(), api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let document::Row::Review(row) = row else {
            return Err(api::Error::InvalidAnswer);
        };
        self.page_row(bounds)?;
        let Some(review) = crate::github::plans::reading::review(word, row)? else {
            return Ok(());
        };
        if review.id <= self.last_review {
            return Err(api::Error::InvalidAnswer);
        }
        self.last_review = review.id;
        let key = match &review.key {
            Some(key) => key.as_ref(),
            None => &[],
        };
        self.keep_bytes(128, &[key, &review.body], bounds)?;
        match self.reviews.push(review) {
            Ok(()) => Ok(()),
            Err(_) => Err(api::Error::TooLarge),
        }
    }
    pub(super) fn reviews_done(&mut self, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let more = self.listing_next()?;
        let reviews = core::mem::replace(&mut self.reviews, List::with_capacity(0)).into_boxed();
        self.terminal(api::Answer::Reviews { reviews, more }, bounds)
    }
}
