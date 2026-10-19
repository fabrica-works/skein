//! Requests counted at their HTTP entrance (fake-forge.md, section 4).
use skein_lib::Map;
pub(crate) const ROUTES: u32 = 64;
use skein_forge::forgejo::request::Request;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Route {
    Org,
    CreateOrg,
    Teams,
    CreateTeam,
    EditTeam,
    TeamMembers,
    AddMember,
    OrgHooks,
    CreateOrgHook,
    EditOrgHook,
    DeleteOrgHook,
    User,
    Tokens,
    CreateToken,
    DeleteToken,
    UserOf,
    CreateUser,
    ApiSettings,
    RepositoryById,
    Repository,
    UserSearch,
    Issues,
    Issue,
    Comments,
    Pull,
    PullFor,
    Pulls,
    CombinedStatus,
    Reviews,
    ReviewComments,
    Branch,
    Branches,
    Protection,
    Protections,
    Collaborators,
    Permission,
    PullFiles,
    Compare,
    Raw,
    Runs,
    Run,
    Jobs,
    Job,
    JobLog,
    CreateIssue,
    OpenPull,
    PostComment,
    PostReview,
    EditIssue,
    RequestReviewers,
    RemoveReviewers,
    Merge,
    Update,
    PostStatus,
    CreateBranch,
    DeleteBranch,
}
#[must_use]
pub fn route(request: &Request) -> Route {
    match request {
        Request::Tokens { .. } => Route::Tokens,
        Request::CreateToken { .. } => Route::CreateToken,
        Request::DeleteToken { .. } => Route::DeleteToken,
        Request::Org { .. } => Route::Org,
        Request::CreateOrg { .. } => Route::CreateOrg,
        Request::Teams { .. } => Route::Teams,
        Request::CreateTeam { .. } => Route::CreateTeam,
        Request::EditTeam { .. } => Route::EditTeam,
        Request::TeamMembers { .. } => Route::TeamMembers,
        Request::AddMember { .. } => Route::AddMember,
        Request::OrgHooks { .. } => Route::OrgHooks,
        Request::CreateOrgHook { .. } => Route::CreateOrgHook,
        Request::EditOrgHook { .. } => Route::EditOrgHook,
        Request::DeleteOrgHook { .. } => Route::DeleteOrgHook,

        Request::UserOf { .. } => Route::UserOf,
        Request::CreateUser { .. } => Route::CreateUser,
        Request::User => Route::User,
        Request::ApiSettings => Route::ApiSettings,
        Request::RepositoryById { .. } => Route::RepositoryById,
        Request::Repository { .. } => Route::Repository,
        Request::UserSearch { .. } => Route::UserSearch,
        Request::Issues { .. } => Route::Issues,
        Request::Issue { .. } => Route::Issue,
        Request::Comments { .. } => Route::Comments,
        Request::Pull { .. } => Route::Pull,
        Request::PullFor { .. } => Route::PullFor,
        Request::Pulls { .. } => Route::Pulls,
        Request::CombinedStatus { .. } => Route::CombinedStatus,
        Request::Reviews { .. } => Route::Reviews,
        Request::ReviewComments { .. } => Route::ReviewComments,
        Request::Branch { .. } => Route::Branch,
        Request::Branches { .. } => Route::Branches,
        Request::Protection { .. } => Route::Protection,
        Request::Protections { .. } => Route::Protections,
        Request::Collaborators { .. } => Route::Collaborators,
        Request::Permission { .. } => Route::Permission,
        Request::PullFiles { .. } => Route::PullFiles,
        Request::Compare { .. } => Route::Compare,
        Request::Raw { .. } => Route::Raw,
        Request::Runs { .. } => Route::Runs,
        Request::Run { .. } => Route::Run,
        Request::Job { .. } => Route::Job,
        Request::Jobs { .. } => Route::Jobs,
        Request::JobLog { .. } => Route::JobLog,
        Request::CreateIssue { .. } => Route::CreateIssue,
        Request::OpenPull { .. } => Route::OpenPull,
        Request::PostComment { .. } => Route::PostComment,
        Request::PostReview { .. } => Route::PostReview,
        Request::EditIssue { .. } => Route::EditIssue,
        Request::RequestReviewers { .. } => Route::RequestReviewers,
        Request::RemoveReviewers { .. } => Route::RemoveReviewers,
        Request::Merge { .. } => Route::Merge,
        Request::Update { .. } => Route::Update,
        Request::PostStatus { .. } => Route::PostStatus,
        Request::CreateBranch { .. } => Route::CreateBranch,
        Request::DeleteBranch { .. } => Route::DeleteBranch,
    }
}
/// Count of read requests, including unrouted and unauthorized entrances.
#[derive(Debug)]
pub struct Tally {
    pub total: u64,
    pub routes: Map<Route, u64>,
    pub users: Map<u64, u64>,
    pub unrouted: u64,
    pub unauthorized: u64,
}
impl Tally {
    #[must_use]
    pub fn new(users: u32) -> Self {
        Self {
            total: 0,
            routes: Map::with_capacity(ROUTES),
            users: Map::with_capacity(users),
            unrouted: 0,
            unauthorized: 0,
        }
    }
    pub fn count(&mut self, route: Option<Route>, user: Option<u64>) {
        self.total = self.total.saturating_add(1);
        if let Some(route) = route {
            let count = self.routes.get(&route).copied().unwrap_or(0).saturating_add(1);
            self.routes.insert(route, count).expect("route capacity");
        } else {
            self.unrouted = self.unrouted.saturating_add(1);
        }
        if let Some(user) = user {
            let count = self.users.get(&user).copied().unwrap_or(0).saturating_add(1);
            self.users.insert(user, count).expect("users bounded by configured and setup token tables");
        } else {
            self.unauthorized = self.unauthorized.saturating_add(1);
        }
    }
}
