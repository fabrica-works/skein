use crate::github::{
    decode,
    document::Kind,
    request::{Request, Scope},
};
use alloc::boxed::Box;
use skein_lib::Wall;

fn owner() -> Box<[u8]> {
    Box::from(*b"acme")
}
fn name() -> Box<[u8]> {
    Box::from(*b"widgets")
}

#[test]
fn github_decode_kind_names_each_documented_answer() {
    let commit = [7; 32];
    for (request, kind) in [
        (Request::Installations { page: 1 }, Kind::InstallationRows),
        (Request::Installation { id: 3 }, Kind::Installation),
        (Request::Mint { installation: 3, scope: Scope::Contents }, Kind::Minted),
        (Request::Reachable { page: 1, per_page: 100 }, Kind::ReachableRows),
        (Request::Viewer, Kind::Viewer),
        (Request::App, Kind::App),
        (Request::HookConfig, Kind::HookConfig),
        (Request::SetHook { url: Box::from(*b"https://example.test/hook") }, Kind::HookConfig),
        (Request::Conversion { code: Box::from(*b"code") }, Kind::Conversion),
        (Request::BotUser { slug: Box::from(*b"example-app") }, Kind::BotUser),
        (Request::User { id: 6 }, Kind::User),
        (Request::Permission { owner: owner(), name: name(), login: Box::from(*b"alice") }, Kind::Permission),
        (Request::Repository { id: 5 }, Kind::Repository),
        (Request::Locate { owner: owner(), name: name() }, Kind::Repository),
        (Request::Issues { owner: owner(), name: name(), since: Wall::EPOCH, page: 1, per_page: 100 }, Kind::IssueRows),
        (Request::Issue { owner: owner(), name: name(), number: 9 }, Kind::Issue),
        (Request::Comments { owner: owner(), name: name(), number: 9, page: 1, per_page: 100 }, Kind::CommentRows),
        (Request::Pull { owner: owner(), name: name(), number: 9 }, Kind::Pull),
        (
            Request::PullsFor {
                owner: owner(),
                name: name(),
                head: Box::from(*b"topic"),
                base: Box::from(*b"main"),
                open: true,
            },
            Kind::PullRows,
        ),
        (
            Request::CombinedStatus { owner: owner(), name: name(), commit, page: 1, per_page: 100 },
            Kind::CombinedStatus,
        ),
        (Request::CheckRuns { owner: owner(), name: name(), commit, page: 1, per_page: 100 }, Kind::CheckRuns),
        (Request::Branch { owner: owner(), name: name(), branch: Box::from(*b"main") }, Kind::Branch),
        (Request::Branches { owner: owner(), name: name(), page: 1, per_page: 100 }, Kind::BranchRows),
    ] {
        assert_eq!(decode::kind(&request), Some(kind), "{request:?}");
    }
}

#[test]
fn github_decode_kind_is_none_for_raw_and_empty_answers() {
    for request in [
        Request::Raw { owner: owner(), name: name(), path: Box::from(*b"README.md"), commit: [7; 32] },
        Request::JobLog { owner: owner(), name: name(), id: 11 },
        Request::Uninstall { id: 3 },
    ] {
        assert_eq!(decode::kind(&request), None, "{request:?}");
    }
}

#[test]
fn github_decode_kind_is_none_for_reads_not_yet_decoded() {
    let commit = [7; 32];
    for request in [
        Request::Reviews { owner: owner(), name: name(), number: 9, page: 1, per_page: 100 },
        Request::ReviewComments { owner: owner(), name: name(), number: 9, review: 2, page: 1, per_page: 100 },
        Request::Job { owner: owner(), name: name(), id: 11 },
        Request::PullFiles { owner: owner(), name: name(), number: 9, page: 1, per_page: 100 },
        Request::Compare { owner: owner(), name: name(), before: commit, after: [8; 32], per_page: 100 },
        Request::RulesForBranch { owner: owner(), name: name(), branch: Box::from(*b"main"), page: 1 },
        Request::Ruleset { owner: owner(), name: name(), id: 4 },
        Request::ClassicProtection { owner: owner(), name: name(), branch: Box::from(*b"main") },
        Request::Collaborators { owner: owner(), name: name(), page: 1, per_page: 100 },
    ] {
        assert_eq!(decode::kind(&request), None, "{request:?}");
    }
}
