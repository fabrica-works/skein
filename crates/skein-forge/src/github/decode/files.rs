//! Comparison rows stream with independent inner bounds on commits and files.
use super::{Malformed, walk::Node};
use crate::{
    api,
    forgejo::id::unsigned,
    github::{
        document::{self, Row},
        id,
    },
};
use alloc::boxed::Box;
use skein_json::Token;
#[derive(Debug)]
pub(super) struct State {
    comparison: document::Compare,
    commit: api::Commit,
    path: Box<[u8]>,
    status: document::FileStatus,
    previous: Option<Box<[u8]>>,
    commits: u32,
    files: u32,
    inner: u32,
}
impl State {
    pub(super) fn new(inner: u32) -> Self {
        Self {
            comparison: document::Compare { total_commits: 0, behind_by: 0, merge_base: None },
            commit: [0; 32],
            path: Box::from([]),
            status: document::FileStatus::Modified,
            previous: None,
            commits: 0,
            files: 0,
            inner,
        }
    }
    pub(super) fn document(&self) -> document::Compare {
        self.comparison.clone()
    }
    pub(super) fn end(&mut self, node: Node) -> Result<Option<Row>, Malformed> {
        if node == Node::CompareCommit {
            self.commits = self.commits.checked_add(1).ok_or(Malformed::Bound { field: b"commits" })?;
            if self.commits > self.inner {
                return Err(Malformed::Bound { field: b"commits" });
            }
            return Ok(Some(Row::Commit(self.commit)));
        }
        if node == Node::CompareFile || node == Node::FileRow {
            self.files = self.files.checked_add(1).ok_or(Malformed::Bound { field: b"files" })?;
            // GitHub caps a comparison's file list at 300, so that length cannot prove completeness.
            if self.files > self.inner || (node == Node::CompareFile && self.files >= 300) {
                return Err(Malformed::Bound { field: b"files" });
            }
            if node == Node::FileRow && self.status == document::FileStatus::Renamed && self.previous.is_none() {
                return Err(Malformed::Missing { field: b"previous_filename" });
            }
            return Ok(Some(Row::File(document::File {
                path: core::mem::replace(&mut self.path, Box::from([])),
                status: core::mem::replace(&mut self.status, document::FileStatus::Modified),
                previous: self.previous.take(),
            })));
        }
        Ok(None)
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let value = unsigned(&text).ok_or(Malformed::Range { field: key })?;
                if key == b"total_commits" {
                    self.comparison.total_commits = value;
                } else {
                    self.comparison.behind_by = value;
                }
            }
            Token::String(text) => {
                if key == b"filename" {
                    self.path = text;
                } else if key == b"previous_filename" {
                    self.previous = Some(text);
                } else if key == b"status" {
                    self.status = match text.as_ref() {
                        b"added" => document::FileStatus::Added,
                        b"removed" => document::FileStatus::Removed,
                        b"renamed" => document::FileStatus::Renamed,
                        b"modified" => document::FileStatus::Modified,
                        b"changed" => document::FileStatus::Changed,
                        b"copied" => document::FileStatus::Copied,
                        b"unchanged" => document::FileStatus::Unchanged,
                        _ => return Err(Malformed::Value { field: key }),
                    };
                } else {
                    let commit = id::commit(&text).ok_or(Malformed::Id { field: key })?;
                    if node == Node::MergeBase {
                        self.comparison.merge_base = Some(commit);
                    } else {
                        self.commit = commit;
                    }
                }
            }
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::True
            | Token::False
            | Token::Null => return Err(Malformed::Shape { field: key }),
        }
        Ok(())
    }
}
