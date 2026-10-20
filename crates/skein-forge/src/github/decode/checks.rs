//! Check-run display fields and Actions job identities (forge-github.md, 4.4).
use super::{Malformed, walk::Node};
use crate::{
    api,
    forgejo::{id::unsigned, time},
    github::{document, id},
};
use skein_json::Token;

#[derive(Debug)]
pub(super) struct State {
    run: document::Run,
    job: document::Job,
    completed: bool,
    conclusion: Option<api::Check>,
}
impl State {
    pub(super) fn new() -> Self {
        Self {
            run: document::Run::new(api::Check::Pending),
            job: document::Job { id: 0, run: 0, attempt: 0, head: [0; 32] },
            completed: false,
            conclusion: None,
        }
    }
    pub(super) fn job(&self) -> document::Job {
        self.job.clone()
    }
    pub(super) fn end(&mut self) -> Result<document::Run, Malformed> {
        let completed = core::mem::replace(&mut self.completed, false);
        let conclusion = self.conclusion.take();
        self.run.check =
            if completed { conclusion.ok_or(Malformed::Value { field: b"conclusion" })? } else { api::Check::Pending };
        Ok(core::mem::replace(&mut self.run, document::Run::new(api::Check::Pending)))
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let number = unsigned(&text).ok_or(Malformed::Range { field: key })?;
                if node == Node::RunItem {
                    self.run.id = Some(number);
                } else if key == b"id" {
                    self.job.id = number;
                } else if key == b"run_id" {
                    self.job.run = number;
                } else {
                    self.job.attempt = u32::try_from(number).ok().ok_or(Malformed::Range { field: key })?;
                }
            }
            Token::String(text) => {
                if node == Node::Job {
                    self.job.head = id::commit(&text).ok_or(Malformed::Id { field: key })?;
                } else if node == Node::RunOutput {
                    self.run.title = text;
                } else if node == Node::RunApp {
                    self.run.app = Some(text);
                } else if key == b"head_sha" {
                    self.run.head = Some(id::commit(&text).ok_or(Malformed::Id { field: key })?);
                } else if key == b"name" {
                    self.run.name = text;
                } else if key == b"html_url" {
                    self.run.url = text;
                } else if key == b"completed_at" {
                    self.run.completed = Some(time::read(&text).ok_or(Malformed::Time { field: key })?);
                } else if key == b"started_at" {
                    self.run.started = Some(time::read(&text).ok_or(Malformed::Time { field: key })?);
                } else if key == b"status" {
                    self.completed = match text.as_ref() {
                        b"completed" => true,
                        b"queued" | b"in_progress" | b"waiting" | b"requested" | b"pending" => false,
                        _ => return Err(Malformed::Value { field: key }),
                    };
                } else {
                    // Pending runs need no conclusion, regardless of member order.
                    self.conclusion = match text.as_ref() {
                        b"success" | b"neutral" | b"skipped" => Some(api::Check::Passed),
                        b"failure" | b"timed_out" | b"cancelled" | b"action_required" | b"startup_failure"
                        | b"stale" => Some(api::Check::Failed),
                        _ => None,
                    };
                }
            }
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::True
            | Token::False
            | Token::Null => unreachable!("checks field table"),
        }
        Ok(())
    }
}
