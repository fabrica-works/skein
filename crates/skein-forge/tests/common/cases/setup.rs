#![expect(clippy::disallowed_macros, reason = "the document harness formats fixture values and literal expected rows")]
#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the document test harness retains finite fixture rows and mutation buffers in ordinary Rust collections"
)]
#![expect(
    clippy::must_use_candidate,
    reason = "fixture-returning helpers are only consumed by assertions or other fixture construction"
)]
#![expect(
    clippy::std_instead_of_alloc,
    reason = "the integration harness runs under std and retains the copied fixture imports"
)]
use super::super::fixtures;
use super::super::forgejo;
use skein_forge::forgejo::{
    document::{Access, Document, Row},
    request::Request,
    server::Answer,
};
use std::boxed::Box;
pub fn answers() -> Vec<(&'static str, Answer)> {
    let bot = || Answer::User(fixtures::user(fixtures::BOT, b"bot", false, b"kit:bot"));
    let person = || Answer::User(fixtures::user(fixtures::PERSON, b"person", false, b"Person"));
    vec![
        ("setup-user-create", bot()),
        ("setup-user-of", bot()),
        ("setup-user-admin", Answer::User(fixtures::user(1, b"fixture", true, b""))),
        ("setup-user-create-person", person()),
        ("setup-user-person-cleared", person()),
        ("setup-token-make", Answer::Token(fixtures::api_token(true))),
        ("setup-token-make-git", Answer::Token(fixtures::git_token(true))),
        ("setup-token-list", Answer::Tokens(Box::new([fixtures::git_token(false)]))),
        ("setup-token-list-page-2", Answer::Tokens(Box::new([fixtures::api_token(false)]))),
        ("setup-token-as-the-bot", bot()),
        ("setup-token-git-as-the-bot", bot()),
        ("setup-scope-read-user", Answer::User(fixtures::user(1, b"fixture", true, b""))),
    ]
}
pub fn expected(answer: Answer) -> (Document, Vec<Row>) {
    if let Answer::User(user) = answer {
        return (Document::User(user), vec![]);
    }
    if let Answer::Token(token) = answer {
        return (Document::Token(token), vec![]);
    }
    let Answer::Tokens(tokens) = answer else { panic!("a setup answer") };
    (Document::Listed { skipped: 0 }, tokens.into_vec().into_iter().map(Row::Token).collect())
}
pub fn request(case: &str) -> Request {
    forgejo::setup_cases().into_iter().find(|(name, _)| *name == case).expect("setup case").1
}
#[derive(Debug)]
pub struct OrgAnswer {
    pub case: &'static str,
    pub request: Request,
    pub document: Document,
    pub rows: Vec<Row>,
    pub answer: Answer,
}
pub fn org_answers() -> Vec<OrgAnswer> {
    let mut cases = Vec::new();
    for case in ["setup-org-made", "setup-org-read"] {
        cases.push(org_answer(case, Document::Org(fixtures::org()), vec![], Answer::Org(fixtures::org())));
    }
    cases.push(org_answer("setup-team-made", Document::Team(fixtures::team()), vec![], Answer::Team(fixtures::team())));
    let mut edited = fixtures::team();
    edited.spec.permission = Access::Read;
    edited.spec.includes_all_repositories = false;
    edited.spec.units.pulls = Access::Read;
    edited.spec.units.actions = Access::None;
    cases.push(org_answer("setup-team-edit", Document::Team(edited.clone()), vec![], Answer::Team(edited)));
    for (case, teams) in [
        ("setup-team-list-new-org", vec![fixtures::owners()]),
        ("setup-team-list", vec![fixtures::owners(), fixtures::team()]),
        ("setup-team-list-page-1", vec![fixtures::owners()]),
        ("setup-team-list-page-3", vec![]),
    ] {
        let rows = teams.iter().cloned().map(Row::Team).collect();
        cases.push(org_answer(case, Document::Listed { skipped: 0 }, rows, Answer::Teams(teams.into_boxed_slice())));
    }
    for (case, users) in
        [("setup-team-members-none", vec![]), ("setup-team-members", vec![fixtures::user(2, b"writer", false, b"")])]
    {
        let rows = users.iter().cloned().map(Row::User).collect();
        cases.push(org_answer(case, Document::Listed { skipped: 0 }, rows, Answer::Members(users.into_boxed_slice())));
    }
    cases
}
pub fn org_answer(case: &'static str, document: Document, rows: Vec<Row>, answer: Answer) -> OrgAnswer {
    let request = forgejo::setup_cases().into_iter().find(|(name, _)| *name == case).expect("org case").1;
    OrgAnswer { case, request, document, rows, answer }
}
pub fn hook_answers() -> Vec<OrgAnswer> {
    let mut cases = Vec::new();
    for (case, id, other, narrow) in [
        ("setup-hook-made", 1, false, false),
        ("setup-hook-made-twin", 2, false, false),
        ("setup-hook-made-other", 3, true, false),
        ("setup-hook-patched", 1, false, false),
        ("setup-hook-patched-narrow", 1, false, true),
    ] {
        let hook = fixtures::hook(id, other, narrow);
        cases.push(org_answer(case, Document::Hook(hook.clone()), vec![], Answer::Hook(hook)));
    }
    for (case, hooks) in [
        ("setup-hook-list-none", vec![]),
        (
            "setup-hook-list",
            vec![fixtures::hook(1, false, false), fixtures::hook(2, false, false), fixtures::hook(3, true, false)],
        ),
        ("setup-hook-list-page-1", vec![fixtures::hook(1, false, false)]),
        ("setup-hook-list-page-4", vec![]),
    ] {
        let rows = hooks.iter().cloned().map(Row::Hook).collect();
        cases.push(org_answer(case, Document::Listed { skipped: 0 }, rows, Answer::Hooks(hooks.into_boxed_slice())));
    }
    cases
}
