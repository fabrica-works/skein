//! One repository's access for the bot (forge-setup.md, 4.5, steps 2 and 3).
use crate::reconcile::{Event, Reply, Why, reply, unexpected};
use alloc::boxed::Box;
use skein_forge::forgejo::{
    document::{Document, Permission},
    request::{Grant, Repo, Request},
};
use skein_lib::Writer;

#[derive(Debug)]
enum Phase {
    Repository,
    Permission,
    Add,
}
#[derive(Debug)]
pub(crate) struct Access {
    id: u64,
    bot: Box<[u8]>,
    path: Option<Repo>,
    phase: Phase,
}
pub(crate) enum Move {
    Send(Request),
    Done,
}
pub(crate) enum Fail {
    Gone,
    Why(Why),
}
impl Access {
    pub fn new(id: u64, bot: &[u8]) -> Self {
        Self { id, bot: bot.into(), path: None, phase: Phase::Repository }
    }
    /// The first request: the repository by id.
    pub fn begin(&mut self) -> Request {
        Request::RepositoryById { id: self.id }
    }
    pub fn event(&mut self, event: Event) -> Result<Move, Fail> {
        let answer = match reply(event) {
            Ok(answer) => answer,
            Err(why) => return Err(Fail::Why(why)),
        };
        match self.phase {
            Phase::Repository => match answer {
                Reply::Document(Document::Repository(repo)) => {
                    if repo.id != self.id {
                        return Err(Fail::Why(Why::Unexpected));
                    }
                    let at = Repo { owner: repo.owner.login, name: repo.name };
                    self.path = Some(at.clone());
                    self.phase = Phase::Permission;
                    Ok(Move::Send(Request::Permission { at, login: self.bot.clone() }))
                }
                Reply::Status(404) => Err(Fail::Gone),
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(Fail::Why(unexpected(other)))
                }
            },
            Phase::Permission => match answer {
                Reply::Document(Document::Permission(permission)) => match permission.permission {
                    Permission::Write | Permission::Admin | Permission::Owner => Ok(Move::Done),
                    Permission::None | Permission::Read => {
                        self.phase = Phase::Add;
                        Ok(Move::Send(Request::AddCollaborator {
                            at: self.path.clone().expect("repository read before permission"),
                            login: self.bot.clone(),
                            permission: Grant::Write,
                        }))
                    }
                },
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(Fail::Why(unexpected(other)))
                }
            },
            Phase::Add => match answer {
                Reply::Empty => Ok(Move::Done),
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Status(_) | Reply::Down) => {
                    Err(Fail::Why(unexpected(other)))
                }
            },
        }
    }
    /// The id in decimal until its path is read, then `owner/name`.
    pub fn subject(&self) -> Box<[u8]> {
        match &self.path {
            Some(path) => {
                let mut out = Writer::new(
                    path.owner
                        .len()
                        .checked_add(path.name.len())
                        .expect("owner and name fit")
                        .checked_add(1)
                        .expect("repository path fits"),
                );
                out.put(&path.owner).expect("path capacity");
                out.put(b"/").expect("path capacity");
                out.put(&path.name).expect("path capacity");
                out.finish()
            }
            None => skein_lib::Decimal::of(self.id).as_bytes().into(),
        }
    }
}
