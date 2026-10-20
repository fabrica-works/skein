//! The task's route subset; credentials precede body decoding.
use crate::reads;
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
    Viewer,
    User(u64),
    Read(reads::Read),
}

pub(crate) fn decode(method: Method, target: &[u8], limits: &Limits) -> Option<Route> {
    if target.len() > usize::try_from(limits.target_bytes).ok()? {
        return None;
    }
    if method == Method::Post && target == b"/graphql" {
        return Some(Route::Token(Token::Viewer));
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
        | Request::Reopen { .. }
        | Request::Edit { .. }
        | Request::Status { .. }
        | Request::CreateBranch { .. }
        | Request::Reviewers { .. }
        | Request::Update { .. }
        | Request::Review { .. }
        | Request::Post { .. }
        | Request::CreateIssue { .. }
        | Request::OpenPull { .. }
        | Request::Close { .. }
        | Request::DeleteBranch { .. }
        | Request::Enqueue { .. }
        | Request::Merge { .. }
        | Request::PullGraphql { .. } => return None,
    })
}
