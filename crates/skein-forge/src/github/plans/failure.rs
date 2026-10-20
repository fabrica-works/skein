//! Read refusals, transport failures and table invalidation (forge-github.md, 7).
use super::{Failure, tables::Tables};
use crate::{
    api,
    github::{
        decode::Malformed,
        document::Refusal,
        failure::{self, Asked, Drop, Verdict},
        header::Head,
    },
};

/// Classifies a read's status, applying the dialect's repository invalidation.
pub fn read(
    status: u16,
    head: &Head,
    refusal: Option<&Refusal>,
    repository: api::Repository,
    tables: &mut Tables,
) -> api::Error {
    match failure::classify(Asked::Read, status, head, refusal) {
        Verdict::Fail(error, invalidation) => {
            match invalidation {
                Drop::Nothing => {}
                Drop::Row => tables.drop_repository(repository),
            }
            error
        }
        Verdict::Enqueue | Verdict::BranchExists => api::Error::InvalidAnswer,
    }
}
#[must_use]
pub fn transport(failure: Failure) -> api::Error {
    match failure {
        Failure::Unseen => api::Error::Unavailable,
        Failure::Seen => api::Error::Timeout,
    }
}
#[must_use]
pub fn malformed(malformed: Malformed) -> api::Error {
    match malformed {
        Malformed::Bound { .. } => api::Error::TooLarge,
        Malformed::Json(_)
        | Malformed::Shape { .. }
        | Malformed::Missing { .. }
        | Malformed::Range { .. }
        | Malformed::TooLong { .. }
        | Malformed::Id { .. }
        | Malformed::Time { .. }
        | Malformed::Value { .. } => api::Error::InvalidAnswer,
    }
}
