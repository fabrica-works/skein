//! Only the protection fields the client keeps (forge-github.md, 5.3).
use super::{Field, Node, Read, Role, field, optional};
use crate::github::decode::protection::Node as P;
const fn container(key: &'static [u8], node: P) -> Field {
    optional(key, Read::Container(Role::Document(Node::Protection(node))))
}
pub(super) fn fields(node: P) -> &'static [Field] {
    match node {
        P::Ruleset => const { &[field(b"current_user_can_bypass", Read::Text)] },
        P::Rule => {
            const {
                &[
                    field(b"ruleset_id", Read::Number),
                    field(b"type", Read::Text),
                    container(b"parameters", P::Parameters),
                ]
            }
        }
        P::Parameters => {
            const {
                &[
                    optional(b"required_approving_review_count", Read::Number),
                    optional(b"dismiss_stale_reviews_on_push", Read::Boolean),
                    optional(b"require_code_owner_review", Read::Boolean),
                    container(b"required_status_checks", P::Checks),
                ]
            }
        }
        P::Classic => {
            const {
                &[
                    container(b"required_status_checks", P::ClassicChecks),
                    container(b"required_pull_request_reviews", P::Reviews),
                    container(b"required_signatures", P::Signatures),
                    container(b"required_linear_history", P::Linear),
                ]
            }
        }
        P::ClassicChecks => {
            const { &[field(b"contexts", Read::Container(Role::Document(Node::Protection(P::Contexts))))] }
        }
        P::Reviews => {
            const {
                &[
                    field(b"required_approving_review_count", Read::Number),
                    field(b"dismiss_stale_reviews", Read::Boolean),
                    field(b"require_code_owner_reviews", Read::Boolean),
                ]
            }
        }
        P::Signatures | P::Linear => const { &[field(b"enabled", Read::Boolean)] },
        P::Check => const { &[field(b"context", Read::Text)] },
        P::Rows | P::Checks | P::Contexts => &[],
    }
}
pub(super) fn element(node: P) -> Option<Role> {
    match node {
        P::Rows => Some(Role::Document(Node::Protection(P::Rule))),
        P::Checks => Some(Role::Document(Node::Protection(P::Check))),
        P::Contexts => Some(Role::Document(Node::Protection(P::Contexts))),
        P::Rule
        | P::Ruleset
        | P::Parameters
        | P::Classic
        | P::ClassicChecks
        | P::Reviews
        | P::Signatures
        | P::Linear
        | P::Check => None,
    }
}
