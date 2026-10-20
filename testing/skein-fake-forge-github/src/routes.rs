//! The task's route subset; credentials precede body decoding.
use crate::{reads, writes};
use alloc::boxed::Box;
use skein_forge::github::{
    limits::Limits,
    request::{Method, Request},
    server,
};

#[derive(Debug)]
pub(crate) enum Route {
    App(App),
    Token(Token),
    Conversion(skein_lib::Secret),
    BotUser(Box<[u8]>),
}
#[derive(Debug)]
pub(crate) enum App {
    Identity,
    Hook,
    SetHook,
    Installations(u32),
    Installation(u64),
    Mint(u64),
    Uninstall(u64),
}
#[derive(Debug)]
pub(crate) enum Token {
    Reachable(u32),
    Graphql,
    User(u64),
    Read(reads::Read),
    QueueMerge { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Write(writes::Route),
}

#[expect(clippy::too_many_lines, reason = "exhaustive selection of the served dialect routes")]
pub(crate) fn decode(method: Method, target: &[u8], limits: &Limits) -> Option<Route> {
    if target.len() > usize::try_from(limits.target_bytes).ok()? {
        return None;
    }
    if method == Method::Post && target == b"/graphql" {
        return Some(Route::Token(Token::Graphql));
    }
    // Task 23 owns the shared REST write dispatcher. A queue refusal needs
    // only the canonical pull target beneath this one PUT suffix.
    if method == Method::Put {
        let pull_target = target.strip_suffix(b"/merge")?;
        let Request::Pull { owner, name, number } = server::route(Method::Get, pull_target, limits)? else {
            return None;
        };
        return Some(Route::Token(Token::QueueMerge { owner, name, number }));
    }
    // REST keeps server::route's canonical targets, including explicit paging.
    Some(match server::route(method, target, limits)? {
        Request::BotUser { slug } => Route::BotUser(slug),
        Request::Conversion { code } => Route::Conversion(skein_lib::Secret::new(code)),
        Request::App => Route::App(App::Identity),
        Request::HookConfig => Route::App(App::Hook),
        Request::SetHook { .. } => Route::App(App::SetHook),
        Request::Installations { page } if page > 0 => Route::App(App::Installations(page)),
        Request::Installation { id } => Route::App(App::Installation(id)),
        Request::Mint { installation, .. } => Route::App(App::Mint(installation)),
        Request::Uninstall { id } => Route::App(App::Uninstall(id)),
        Request::Reachable { page, .. } if page > 0 => Route::Token(Token::Reachable(page)),
        Request::Repository { id } => Route::Token(Token::Read(reads::Read::Repository(id))),
        Request::Locate { owner, name } => Route::Token(Token::Read(reads::Read::Locate { owner, name })),
        Request::Branch { owner, name, branch } => {
            Route::Token(Token::Read(reads::Read::Branch { owner, name, branch }))
        }
        Request::Branches { owner, name, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Branches { owner, name, page, per_page }))
        }
        Request::Issues { owner, name, since, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Issues { owner, name, since, page, per_page }))
        }
        Request::Issue { owner, name, number } => Route::Token(Token::Read(reads::Read::Issue { owner, name, number })),
        Request::Comments { owner, name, number, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Comments { owner, name, number, page, per_page }))
        }
        Request::Pull { owner, name, number } => Route::Token(Token::Read(reads::Read::Pull { owner, name, number })),
        Request::PullsFor { owner, name, head, base, open } => {
            Route::Token(Token::Read(reads::Read::PullsFor { owner, name, head, base, open }))
        }
        Request::PullFiles { owner, name, number, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::PullFiles { owner, name, number, page, per_page }))
        }
        Request::Compare { owner, name, before, after, per_page } if per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Compare { owner, name, before, after, per_page }))
        }
        Request::Raw { owner, name, commit, path } => {
            Route::Token(Token::Read(reads::Read::Raw { owner, name, commit, path }))
        }
        Request::User { id } => Route::Token(Token::User(id)),
        Request::Collaborators { owner, name, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Collaborators { owner, name, page, per_page }))
        }
        Request::Permission { owner, name, login } => {
            Route::Token(Token::Read(reads::Read::Permission { owner, name, login }))
        }
        Request::Reviews { owner, name, number, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::Reviews { owner, name, number, page, per_page }))
        }
        Request::ReviewComments { owner, name, number, review, page, per_page } if page > 0 && per_page > 0 => {
            Route::Token(Token::Read(reads::Read::ReviewComments { owner, name, number, review }))
        }
        Request::Merge { owner, name, number, .. } => Route::Token(Token::QueueMerge { owner, name, number }),
        Request::CreateBranch { owner, name, branch, commit } => {
            Route::Token(Token::Write(writes::Route::CreateBranch { owner, name, branch, commit }))
        }
        Request::DeleteBranch { owner, name, branch } => {
            Route::Token(Token::Write(writes::Route::DeleteBranch { owner, name, branch }))
        }
        Request::Status { owner, name, commit, context, check } => {
            Route::Token(Token::Write(writes::Route::Status { owner, name, commit, context, check }))
        }
        Request::OpenPull { owner, name, title, body, head, base } => {
            Route::Token(Token::Write(writes::Route::OpenPull { owner, name, title, body, head, base }))
        }
        Request::Update { owner, name, number, head } => {
            Route::Token(Token::Write(writes::Route::Update { owner, name, number, head }))
        }
        Request::Review { owner, name, number, verdict, body } => {
            Route::Token(Token::Write(writes::Route::Review { owner, name, number, verdict, body }))
        }
        Request::Reviewers { owner, name, number, add, logins } => {
            Route::Token(Token::Write(writes::Route::Reviewers { owner, name, number, add, logins }))
        }
        Request::Reopen { owner, name, number } => {
            Route::Token(Token::Write(writes::Route::Reopen { owner, name, number }))
        }
        Request::Close { owner, name, number } => {
            Route::Token(Token::Write(writes::Route::Close { owner, name, number }))
        }
        Request::Edit { owner, name, number, title, body } => {
            Route::Token(Token::Write(writes::Route::Edit { owner, name, number, title, body }))
        }
        Request::Post { owner, name, number, body } => {
            Route::Token(Token::Write(writes::Route::Post { owner, name, number, body }))
        }
        Request::CreateIssue { owner, name, title, body } => {
            Route::Token(Token::Write(writes::Route::CreateIssue { owner, name, title, body }))
        }
        Request::Installations { .. }
        | Request::Reachable { .. }
        | Request::Branches { .. }
        | Request::Viewer
        | Request::Issues { .. }
        | Request::Comments { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Enqueue { .. }
        | Request::PullGraphql { .. } => return None,
    })
}
