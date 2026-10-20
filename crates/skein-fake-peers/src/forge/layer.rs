//! The two HTTP faces use the same connection lifecycle and shared-forge host.
pub(super) use forgejo::Event;
use skein_fake_forge_domain as domain;
use skein_fake_forge_github as github;
use skein_fake_forge_protocol as forgejo;
use skein_lib::{
    Env, Queue, Token,
    stream::{Down, Up},
};
pub(super) const MAX_UP: u32 = forgejo::MAX_UP;
pub(super) const MAX_DOWN: u32 = forgejo::MAX_DOWN;

#[derive(Clone, Copy)]
pub(super) enum Limits {
    Forgejo(forgejo::Limits),
    Github(github::Limits),
}
impl Limits {
    pub fn http(self) -> skein_http::server::Limits {
        match self {
            Self::Forgejo(limits) => limits.http,
            Self::Github(limits) => limits.http,
        }
    }
    pub fn calls(self, hosted: u32) -> u32 {
        match self {
            Self::Forgejo(limits) => limits.calls,
            Self::Github(_) => hosted,
        }
    }
    pub fn connections(self, hosted: u32) -> u32 {
        match self {
            Self::Forgejo(limits) => limits.connections,
            Self::Github(_) => hosted,
        }
    }
}
pub(super) enum Service {
    Forgejo(forgejo::Service),
    Github(github::Service),
}
impl Service {
    pub fn is_github(&self) -> bool {
        matches!(self, Self::Github(_))
    }
    pub fn set_hooks(
        &mut self,
        endpoint: Option<forgejo::delivery::Endpoint>,
    ) -> Result<Option<forgejo::delivery::Endpoint>, forgejo::Error> {
        match self {
            Self::Forgejo(service) => service.set_hooks(endpoint),
            Self::Github(_) => Ok(endpoint),
        }
    }
    pub fn reclaim(&mut self) {
        match self {
            Self::Forgejo(service) => service.reclaim(),
            Self::Github(_) => {}
        }
    }
    pub fn target(&mut self, reply: domain::Request) -> Option<(Token, domain::Request)> {
        match self {
            Self::Forgejo(service) => service.target(reply).ok(),
            Self::Github(_) => match reply {
                domain::Request::Reply { to, result } => {
                    let owner = to.into_token();
                    Some((owner, domain::Request::Reply { to: skein_lib::ReplyTo::new(owner), result }))
                }
                domain::Request::Hook { .. } => None,
            },
        }
    }
    pub fn delivery(&mut self, hook: &domain::Request, limits: &Limits) -> Option<forgejo::delivery::Post> {
        match (self, limits) {
            (Self::Forgejo(service), Limits::Forgejo(limits)) => service.delivery(hook, limits),
            (Self::Github(_), Limits::Github(_)) => None,
            (Self::Forgejo(_), Limits::Github(_)) | (Self::Github(_), Limits::Forgejo(_)) => {
                unreachable!("paired layer")
            }
        }
    }
}
#[expect(clippy::large_enum_variant, reason = "shared::bound counts the full inline server slot for each connection")]
pub(super) enum Server {
    Forgejo(forgejo::Server),
    Github(github::Server),
}
impl Server {
    pub fn new(owner: Token, limits: &Limits) -> Result<Self, crate::Error> {
        match limits {
            Limits::Forgejo(limits) => {
                forgejo::Server::new(owner, limits).map(Self::Forgejo).map_err(|_| crate::Error::Limits)
            }
            Limits::Github(limits) => {
                github::Server::new(owner, limits).map(Self::Github).map_err(|_| crate::Error::Limits)
            }
        }
    }
    pub fn take_head(&mut self) -> Option<skein_http::server::Call> {
        match self {
            Self::Forgejo(server) => server.take_head(),
            Self::Github(server) => server.take_head(),
        }
    }
    pub fn is_settled(&self) -> bool {
        match self {
            Self::Forgejo(server) => server.is_settled(),
            Self::Github(server) => server.is_settled(),
        }
    }
    pub fn has_work(&self) -> bool {
        match self {
            Self::Forgejo(server) => server.has_work(),
            Self::Github(server) => server.has_work(),
        }
    }
}
// Each adapter drains at most MAX_UP translated records. shared::bound counts
// the scratch queue's fixed allocation beside the hosted layer.
macro_rules! entry {
    ($name:ident $(, $arg:ident : $ty:ty)*) => {
        pub(super) fn $name(server: &mut Server, service: &mut Service, env: &Env<Limits>, $($arg: $ty,)* above: &mut Queue<Event>, below: &mut Queue<Down>) {
            match (server, service, &env.limits) {
                (Server::Forgejo(server), Service::Forgejo(service), Limits::Forgejo(limits)) => forgejo::$name(server, service, &Env { now: env.now, wall: env.wall, limits: *limits }, $($arg,)* above, below),
                (Server::Github(server), Service::Github(service), Limits::Github(limits)) => {
                    let mut events = Queue::with_capacity(github::MAX_UP);
                    github::$name(server, service, &Env { now: env.now, wall: env.wall, limits: *limits }, $($arg,)* &mut events, below);
                    translate(&mut events, above);
                }
                (Server::Forgejo(_), Service::Forgejo(_) | Service::Github(_), Limits::Github(_))
                | (Server::Forgejo(_), Service::Github(_), Limits::Forgejo(_))
                | (Server::Github(_), Service::Forgejo(_) | Service::Github(_), Limits::Forgejo(_))
                | (Server::Github(_), Service::Forgejo(_), Limits::Github(_)) => unreachable!("paired layer"),
            }
        }
    }
}
entry!(start);
entry!(up, event: Up);
entry!(resume);
entry!(close);
entry!(closed);
pub(super) fn down(
    server: &mut Server,
    service: &mut Service,
    env: &Env<Limits>,
    reply: domain::Request,
    domain: &domain::Domain,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) {
    match (server, service, &env.limits) {
        (Server::Forgejo(server), Service::Forgejo(service), Limits::Forgejo(limits)) => {
            forgejo::down(server, service, &Env { now: env.now, wall: env.wall, limits: *limits }, reply, above, below);
        }
        (Server::Github(server), Service::Github(service), Limits::Github(limits)) => {
            let mut events = Queue::with_capacity(github::MAX_UP);
            github::down(
                server,
                service,
                &Env { now: env.now, wall: env.wall, limits: *limits },
                reply,
                domain,
                &mut events,
                below,
            );
            translate(&mut events, above);
        }
        (Server::Forgejo(_), Service::Forgejo(_) | Service::Github(_), Limits::Github(_))
        | (Server::Forgejo(_), Service::Github(_), Limits::Forgejo(_))
        | (Server::Github(_), Service::Forgejo(_) | Service::Github(_), Limits::Forgejo(_))
        | (Server::Github(_), Service::Forgejo(_), Limits::Github(_)) => unreachable!("paired layer"),
    }
}
fn translate(events: &mut Queue<github::Event>, above: &mut Queue<Event>) {
    while let Some(event) = events.pop() {
        above.push(match event {
            github::Event::Domain(event) => Event::Domain(event),
            github::Event::Close => Event::Close,
            github::Event::Closed => Event::Closed,
        });
    }
}
