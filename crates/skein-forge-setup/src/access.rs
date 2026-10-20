//! One repository's access for the bot (forge-setup.md, 4.5, steps 2 to 4).
use crate::{
    item::Item,
    reconcile::{Event, Reply, Why, reply, unexpected},
};
use alloc::boxed::Box;
use skein_forge::forgejo::{
    delivery::SUBSCRIBED,
    document::{Document, Permission, Row},
    request::{Grant, HookSpec, Page, Repo, Request},
};
use skein_lib::{List, Secret, Writer};

/// What the repositories of one run share (forge-setup.md, 4.4 and 4.5).
#[derive(Debug)]
pub(crate) struct Shared {
    pub cap: Option<u32>,
    pub secret: Option<Secret>,
}

#[derive(Debug)]
enum Phase {
    Repository,
    Permission,
    Add,
    Secret,
    Draw,
    Write,
    Settings,
    Listing,
    Delete,
    Save,
}
#[derive(Debug)]
pub(crate) struct Access {
    id: u64,
    bot: Box<[u8]>,
    path: Option<Repo>,
    phase: Phase,
    url: Option<Box<[u8]>>,
    page: u32,
    kept: Option<u64>,
}
pub(crate) enum Move {
    Send(Request),
    Read(Item),
    Draw(u32),
    Write { item: Item, bytes: Secret },
    Done,
}
pub(crate) enum Fail {
    Gone,
    Why(Why),
}
impl Access {
    pub fn new(id: u64, bot: &[u8], url: Option<Box<[u8]>>) -> Self {
        Self { id, bot: bot.into(), path: None, phase: Phase::Repository, url, page: 1, kept: None }
    }
    /// The first request: the repository by id.
    pub fn begin(&mut self) -> Request {
        Request::RepositoryById { id: self.id }
    }
    pub fn event(&mut self, shared: &mut Shared, event: Event) -> Result<Move, Fail> {
        if let (Phase::Repository, Event::Reply(Reply::Status(404))) = (&self.phase, &event) {
            return Err(Fail::Gone);
        }
        match self.advance(shared, event) {
            Ok(movement) => Ok(movement),
            Err(why) => Err(Fail::Why(why)),
        }
    }
    fn advance(&mut self, shared: &mut Shared, event: Event) -> Result<Move, Why> {
        match self.phase {
            Phase::Secret => {
                return match event {
                    Event::Read(Some(secret)) => {
                        shared.secret = Some(secret);
                        Ok(self.settings(shared))
                    }
                    Event::Read(None) => {
                        self.phase = Phase::Draw;
                        Ok(Move::Draw(32))
                    }
                    Event::Begin | Event::Reply(_) | Event::Written | Event::Drawn(_) => Err(Why::Unexpected),
                };
            }
            Phase::Draw => {
                return match event {
                    Event::Drawn(value) if value.len() == 32 => {
                        let secret = crate::reconcile::hex(&value);
                        shared.secret = Some(secret.clone());
                        self.phase = Phase::Write;
                        Ok(Move::Write { item: Item::WebhookSecret, bytes: secret })
                    }
                    Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                        Err(Why::Unexpected)
                    }
                };
            }
            Phase::Write => {
                return match event {
                    Event::Written => Ok(self.settings(shared)),
                    Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Drawn(_) => Err(Why::Unexpected),
                };
            }
            Phase::Repository
            | Phase::Permission
            | Phase::Add
            | Phase::Settings
            | Phase::Listing
            | Phase::Delete
            | Phase::Save => (),
        }
        self.answer(shared, reply(event)?)
    }
    fn answer(&mut self, shared: &mut Shared, answer: Reply) -> Result<Move, Why> {
        match self.phase {
            Phase::Repository => match answer {
                Reply::Document(Document::Repository(repo)) => {
                    if repo.id != self.id {
                        return Err(Why::Unexpected);
                    }
                    let at = Repo { owner: repo.owner.login, name: repo.name };
                    self.path = Some(at.clone());
                    self.phase = Phase::Permission;
                    Ok(Move::Send(Request::Permission { at, login: self.bot.clone() }))
                }
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            Phase::Permission => match answer {
                Reply::Document(Document::Permission(permission)) => match permission.permission {
                    Permission::Write | Permission::Admin | Permission::Owner => Ok(self.hook(shared)),
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
                    Err(unexpected(other))
                }
            },
            Phase::Add => match answer {
                Reply::Empty => Ok(self.hook(shared)),
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            Phase::Settings => match answer {
                Reply::Document(Document::ApiSettings(settings)) if settings.max_response_items > 0 => {
                    shared.cap = Some(settings.max_response_items);
                    Ok(self.listing(shared))
                }
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            Phase::Listing => self.rows(shared, answer),
            Phase::Delete => match answer {
                Reply::Empty | Reply::Status(404) => Ok(self.listing(shared)),
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            Phase::Save => match answer {
                Reply::Document(Document::Hook(_)) => Ok(Move::Done),
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            Phase::Secret | Phase::Draw | Phase::Write => unreachable!("host effects handled before replies"),
        }
    }
    fn hook(&mut self, shared: &Shared) -> Move {
        if self.url.is_none() {
            return Move::Done;
        }
        if shared.secret.is_some() {
            self.settings(shared)
        } else {
            self.phase = Phase::Secret;
            Move::Read(Item::WebhookSecret)
        }
    }
    fn settings(&mut self, shared: &Shared) -> Move {
        if shared.cap.is_some() {
            self.listing(shared)
        } else {
            self.phase = Phase::Settings;
            Move::Send(Request::ApiSettings)
        }
    }
    fn at(&self) -> Repo {
        self.path.clone().expect("repository read before hook")
    }
    fn listing(&mut self, shared: &Shared) -> Move {
        self.phase = Phase::Listing;
        Move::Send(Request::RepoHooks { at: self.at(), page: Page { page: Some(self.page), limit: shared.cap } })
    }
    fn save(&mut self, shared: &Shared) -> Move {
        let mut events = List::with_capacity(u32::try_from(SUBSCRIBED.len()).expect("events fit"));
        for event in SUBSCRIBED {
            events.push(event.into()).expect("event capacity");
        }
        let hook = HookSpec {
            url: self.url.clone().expect("loopback hook"),
            secret: shared.secret.clone().expect("secret read"),
            events: events.into_boxed(),
            active: true,
        };
        self.phase = Phase::Save;
        Move::Send(match self.kept {
            Some(id) => Request::EditRepoHook { at: self.at(), id, hook },
            None => Request::CreateRepoHook { at: self.at(), hook },
        })
    }
    fn rows(&mut self, shared: &Shared, answer: Reply) -> Result<Move, Why> {
        let Reply::Rows(rows) = answer else {
            return Err(unexpected(answer));
        };
        for row in &rows {
            let Row::Hook(hook) = row else {
                return Err(Why::Unexpected);
            };
            if Some(&hook.url) != self.url.as_ref() {
                continue;
            }
            if let Some(kept) = self.kept {
                if kept == hook.id {
                    continue;
                }
                self.kept = Some(kept.min(hook.id));
                self.phase = Phase::Delete;
                return Ok(Move::Send(Request::DeleteRepoHook { at: self.at(), id: kept.max(hook.id) }));
            }
            self.kept = Some(hook.id);
        }
        if rows.len() < usize::try_from(shared.cap.expect("cap read before listing")).expect("u32 fits") {
            Ok(self.save(shared))
        } else {
            self.page = self.page.checked_add(1).ok_or(Why::Unexpected)?;
            Ok(self.listing(shared))
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
