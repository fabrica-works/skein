use crate::forgejo::{
    decode::{
        Malformed, Step,
        walk::{Field, Kind, Move, Read, Value, Walk},
    },
    document::{self, Body, Document, Job, Row, Run, StepStatus},
    id::{ObjectFormat, ObjectId},
};
use alloc::boxed::Box;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ActionKind {
    Run,
    Runs,
    Listing,
    Job,
    Jobs,
    Step,
    Steps,
}
const fn field(key: &'static [u8], read: Read) -> Field {
    Field { key, read, required: true }
}
pub(crate) fn fields(kind: ActionKind) -> &'static [Field] {
    match kind {
        ActionKind::Run => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"index_in_repo", Read::Number),
                    field(b"commit_sha", Read::Id { empty: false }),
                ]
            }
        }
        ActionKind::Job => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"run_id", Read::Number),
                    field(b"attempt", Read::Number),
                    Field { key: b"steps", read: Read::Array(Kind::Action(ActionKind::Steps)), required: false },
                ]
            }
        }
        ActionKind::Listing => {
            const {
                &[
                    field(b"total_count", Read::Number),
                    field(b"workflow_runs", Read::Array(Kind::Action(ActionKind::Runs))),
                ]
            }
        }
        ActionKind::Step => {
            const { &[field(b"number", Read::Number), field(b"name", Read::Body), field(b"status", Read::Text)] }
        }
        ActionKind::Runs | ActionKind::Jobs | ActionKind::Steps => &[],
    }
}
pub(crate) fn element(kind: ActionKind) -> Option<Kind> {
    match kind {
        ActionKind::Runs => Some(Kind::Action(ActionKind::Run)),
        ActionKind::Jobs => Some(Kind::Action(ActionKind::Job)),
        ActionKind::Steps => Some(Kind::Action(ActionKind::Step)),
        ActionKind::Run | ActionKind::Job | ActionKind::Step | ActionKind::Listing => None,
    }
}
#[derive(Debug)]
pub(super) struct Actions {
    root: ActionKind,
    run: Run,
    job: Job,
    step: document::Step,
    total_count: u64,
}
impl Actions {
    pub(super) fn new(root: ActionKind) -> Self {
        Self {
            root,
            run: Run { id: 0, index_in_repo: 0, commit_sha: ObjectId { format: ObjectFormat::Sha1, bytes: [0; 32] } },
            job: Job { id: 0, run_id: 0, attempt: 0 },
            step: document::Step { number: 0, name: Box::from([]), status: StepStatus::Unknown },
            total_count: 0,
        }
    }
    pub(super) fn step(&mut self, movement: Move, walk: &Walk) -> Step {
        let result = match movement {
            Move::Demand(demand) => return Step::Demand(demand),
            Move::Scalar { kind, key, value } => self.scalar(kind, key, value),
            Move::End(Kind::Action(ActionKind::Run)) => {
                if self.root == ActionKind::Listing {
                    return Step::Row(Row::Run(self.run), skein_json::tokenizer::Request::Next);
                }
                Ok(())
            }
            Move::End(Kind::Action(ActionKind::Job)) => {
                if self.root == ActionKind::Jobs {
                    return Step::Row(Row::Job(self.job), skein_json::tokenizer::Request::Next);
                }
                Ok(())
            }
            Move::End(Kind::Action(ActionKind::Step)) => {
                let step = core::mem::replace(
                    &mut self.step,
                    document::Step { number: 0, name: Box::from([]), status: StepStatus::Unknown },
                );
                if self.root == ActionKind::Job {
                    return Step::Row(Row::Step(step), skein_json::tokenizer::Request::Next);
                }
                Ok(())
            }
            Move::End(Kind::Action(ActionKind::Runs | ActionKind::Jobs | ActionKind::Listing | ActionKind::Steps)) => {
                Ok(())
            }
            Move::End(
                Kind::User
                | Kind::Error
                | Kind::Write(_)
                | Kind::Settings
                | Kind::Repository
                | Kind::Search
                | Kind::Setup(_)
                | Kind::Users
                | Kind::Item(_)
                | Kind::Review(_)
                | Kind::Delivery(_)
                | Kind::Branch(_)
                | Kind::File(_),
            ) => unreachable!("action tables"),
            Move::Done => {
                return Step::Done(match self.root {
                    ActionKind::Run => Document::Run(self.run),
                    ActionKind::Listing => Document::Runs { total_count: self.total_count },
                    ActionKind::Jobs => Document::Listed { skipped: walk.skipped() },
                    ActionKind::Job => Document::Job(self.job),
                    ActionKind::Step | ActionKind::Steps | ActionKind::Runs => unreachable!("action root selection"),
                });
            }
        };
        match result {
            Ok(()) => Step::Demand(skein_json::tokenizer::Request::Next),
            Err(error) => Step::Malformed(error),
        }
    }
    fn scalar(&mut self, kind: Kind, key: &'static [u8], value: Value) -> Result<(), Malformed> {
        match value {
            Value::Number(n) => match key {
                b"id" => {
                    if kind == Kind::Action(ActionKind::Run) {
                        self.run.id = n;
                    } else {
                        self.job.id = n;
                    }
                }
                b"index_in_repo" => self.run.index_in_repo = n,
                b"run_id" => self.job.run_id = n,
                b"attempt" | b"number" => {
                    let Ok(n) = u32::try_from(n) else {
                        return Err(Malformed::Range { field: key });
                    };
                    if key == b"attempt" {
                        self.job.attempt = n;
                    } else {
                        self.step.number = n;
                    }
                }
                b"total_count" => self.total_count = n,
                _ => unreachable!("action number table"),
            },
            Value::Id(id) => self.run.commit_sha = id.expect("required run commit"),
            Value::Text(text) => {
                self.step.status = document::step_status(&text).ok_or(Malformed::Value { field: key })?;
            }
            Value::Body(body) => {
                self.step.name = match body {
                    Body::Kept(name) => name,
                    Body::Long(_) => Box::from([]),
                }
            }
            Value::Time(_) | Value::Boolean(_) => unreachable!("action scalar table"),
        }
        Ok(())
    }
}
