#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![expect(
    clippy::std_instead_of_alloc,
    reason = "the integration harness runs under std and retains the copied fixture imports"
)]
#![cfg(test)]
use skein_forge::forgejo::document::{AccessToken, User};
use std::boxed::Box;
pub const BOT: u64 = 4;
pub const PERSON: u64 = 5;
pub const API_TOKEN: u64 = 6;
pub const GIT_TOKEN: u64 = 7;
#[must_use]
pub fn user(id: u64, login: &[u8], is_admin: bool, full_name: &[u8]) -> User {
    User { id, login: Box::from(login), is_admin, full_name: Box::from(full_name) }
}
#[must_use]
pub fn api_token(value: bool) -> AccessToken {
    AccessToken {
        id: API_TOKEN,
        name: Box::from(*b"kit-api"),
        scopes: Box::new([Box::from(*b"write:issue"), Box::from(*b"write:repository"), Box::from(*b"read:user")]),
        last_eight: Box::from(*b"00000003"),
        value: if value {
            Some(skein_lib::Secret::new(Box::from(*b"0000000000000000000000000000000000000004")))
        } else {
            None
        },
    }
}
#[must_use]
pub fn git_token(value: bool) -> AccessToken {
    let mut token = api_token(value);
    token.id = GIT_TOKEN;
    token.name = Box::from(*b"kit-git");
    token.scopes = Box::new([Box::from(*b"write:repository"), Box::from(*b"read:user")]);
    token.last_eight = Box::from(*b"00000005");
    token.value = if value {
        Some(skein_lib::Secret::new(Box::from(*b"0000000000000000000000000000000000000006")))
    } else {
        None
    };
    token
}

use skein_forge::forgejo::document::{Access, Org, Team, TeamSpec, Units};
pub const ORG: u64 = 6;
pub const TEAM: u64 = 4;
#[must_use]
pub fn org() -> Org {
    Org { id: ORG, name: Box::from(*b"ensemble") }
}
#[must_use]
pub fn team() -> Team {
    Team {
        id: TEAM,
        spec: TeamSpec {
            name: Box::from(*b"writer"),
            permission: Access::Write,
            includes_all_repositories: true,
            units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
        },
    }
}
#[must_use]
pub fn owners() -> Team {
    Team {
        id: 1,
        spec: TeamSpec {
            name: Box::from(*b"Owners"),
            permission: Access::Owner,
            includes_all_repositories: true,
            units: Units { code: Access::Owner, issues: Access::Owner, pulls: Access::Owner, actions: Access::Owner },
        },
    }
}
#[must_use]
pub fn hook(id: u64, other: bool, narrow: bool) -> skein_forge::forgejo::document::Hook {
    skein_forge::forgejo::document::Hook {
        id,
        url: Box::from(if other {
            b"http://127.0.0.1:3000/other".as_slice()
        } else {
            b"http://127.0.0.1:3000/hooks/1"
        }),
        events: if other {
            Box::from([Box::<[u8]>::from(*b"push")])
        } else if narrow {
            ["push", "workflow_job_cancelled", "workflow_job_failure", "workflow_job_skipped", "workflow_job_success"]
                .iter()
                .map(|event| Box::from(event.as_bytes()))
                .collect()
        } else {
            hook_events()
        },
        active: !narrow,
    }
}

fn hook_events() -> Box<[Box<[u8]>]> {
    [
        "create",
        "delete",
        "push",
        "issues",
        "issue_assign",
        "issue_label",
        "issue_milestone",
        "issue_comment",
        "pull_request",
        "pull_request_assign",
        "pull_request_label",
        "pull_request_milestone",
        "pull_request_comment",
        "pull_request_review_approved",
        "pull_request_review_rejected",
        "pull_request_review_comment",
        "pull_request_sync",
        "pull_request_review_request",
        "workflow_job_cancelled",
        "workflow_job_failure",
        "workflow_job_skipped",
        "workflow_job_success",
    ]
    .iter()
    .map(|event| Box::from(event.as_bytes()))
    .collect()
}
