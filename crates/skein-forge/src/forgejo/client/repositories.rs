//! The organization repository creation answer (forge-setup.md, 4.3).
use crate::forgejo::{
    decode::{
        Malformed, Step,
        walk::{Field, Move, Read, Value},
    },
    document::{self, Document},
};
use alloc::boxed::Box;
use skein_json::{tokenizer, writer::Encoder};

pub(crate) fn fields() -> &'static [Field] {
    const {
        &[
            Field { key: b"id", read: Read::Number, required: true },
            Field { key: b"full_name", read: Read::Text, required: true },
            Field { key: b"default_branch", read: Read::Text, required: true },
            Field { key: b"clone_url", read: Read::Text, required: true },
            Field { key: b"ssh_url", read: Read::Text, required: true },
        ]
    }
}
fn empty() -> document::CreatedRepository {
    document::CreatedRepository {
        id: 0,
        full_name: Box::from([]),
        default_branch: Box::from([]),
        clone_url: Box::from([]),
        ssh_url: Box::from([]),
    }
}
#[derive(Debug)]
pub(super) struct CreatedRepository {
    repository: document::CreatedRepository,
}
impl CreatedRepository {
    pub(super) fn new() -> Self {
        Self { repository: empty() }
    }
    pub(super) fn step(&mut self, movement: Move) -> Step {
        match movement {
            Move::Demand(demand) => return Step::Demand(demand),
            Move::Scalar { key, value, .. } => match (key, value) {
                (b"id", Value::Number(id)) => self.repository.id = id,
                (b"full_name", Value::Text(text)) => self.repository.full_name = text,
                (b"default_branch", Value::Text(text)) => self.repository.default_branch = text,
                (b"clone_url", Value::Text(text)) => self.repository.clone_url = text,
                (b"ssh_url", Value::Text(text)) => self.repository.ssh_url = text,
                _ => return Step::Malformed(Malformed::Shape { field: key }),
            },
            Move::End(_) => {}
            Move::Done => {
                return Step::Done(Document::CreatedRepository(core::mem::replace(&mut self.repository, empty())));
            }
        }
        Step::Demand(tokenizer::Request::Next)
    }
}
pub(crate) fn write(json: &mut Encoder, repository: &document::CreatedRepository) {
    json.object_start();
    json.key(b"id");
    json.unsigned(repository.id);
    json.key(b"full_name");
    json.string(&repository.full_name);
    json.key(b"default_branch");
    json.string(&repository.default_branch);
    json.key(b"clone_url");
    json.string(&repository.clone_url);
    json.key(b"ssh_url");
    json.string(&repository.ssh_url);
    json.object_end();
}
