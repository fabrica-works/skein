//! The task's route subset; credentials precede body decoding.
use crate::reads;
use skein_forge::github::{
    limits::Limits,
    request::{Method, Request},
    server,
};

#[derive(Debug)]
pub(crate) enum Route {
    App(App),
    Token(Token),
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
        Request::Installations { .. }
        | Request::Reachable { .. }
        | Request::Branches { .. }
        | Request::Viewer
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::User { .. } => return None,
    })
}
