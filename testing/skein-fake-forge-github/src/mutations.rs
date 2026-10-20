//! Person-side mutations decoded locally; the shared domain enforces permission and bounds.
use crate::{
    answers::{self, Body},
    auth::Access,
    reads,
    service::{Outcome, Pending, PendingKind, Response, Service},
};
use alloc::boxed::Box;
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::github::request::Method;
use skein_json::Token;
use skein_lib::{List, ReplyTo, bytes};

#[derive(Debug)]
pub(crate) enum Route {
    CreateRules { repository: Box<[u8]> },
    DeleteRules { repository: Box<[u8]>, id: u64 },
}
#[derive(Debug)]
pub(crate) struct Mutation {
    repository: Box<[u8]>,
    creating: bool,
}
pub(crate) fn route(method: Method, target: &[u8]) -> Option<Route> {
    let tail = target.strip_prefix(b"/repos/")?;
    let end = bytes::find(tail, b"/rulesets")?;
    let repository = tail.get(..end)?;
    let suffix = tail.get(end.checked_add(9)?..)?;
    if method == Method::Post && suffix.is_empty() {
        return Some(Route::CreateRules { repository: bytes::copy_of(repository) });
    }
    if method == Method::Delete {
        let id = skein_forge::forgejo::id::unsigned(suffix.strip_prefix(b"/")?)?;
        return Some(Route::DeleteRules { repository: bytes::copy_of(repository), id });
    }
    None
}
pub(crate) fn call(service: &Service, route: Route, access: Access, body: &[u8], reply_to: ReplyTo) -> Outcome {
    if let Access::Installation { .. } = access {
        return Outcome::Reply(answers::refusal(403, crate::texts::RULESET_INTEGRATION, &service.limits));
    }
    let (repository, creating, id) = match route {
        Route::CreateRules { repository } => (repository, true, service.ruleset_id),
        Route::DeleteRules { repository, id } => (repository, false, id),
    };
    let mut found = false;
    for (_, row) in &service.repositories {
        if let Some(full) = reads::full(row)
            && full == repository
        {
            found = true;
        }
    }
    if !found || id != service.ruleset_id {
        return Outcome::Reply(answers::missing(&service.limits));
    }
    let protection = if creating {
        let Some(rule) = protection(service, body) else {
            return Outcome::Reply(answers::refusal(422, b"Validation Failed", &service.limits));
        };
        Some(rule)
    } else {
        if !body.is_empty() {
            return Outcome::Reply(answers::refusal(422, b"Validation Failed", &service.limits));
        }
        None
    };
    let event = fake::Event::Call {
        reply_to,
        user: access.user(),
        repository: repository.clone(),
        op: a::Op::Write(a::Write::SetProtection { protection }),
    };
    Outcome::Call(Pending { rate: None, kind: PendingKind::Mutation(Mutation { repository, creating }) }, event)
}
pub(crate) fn finish(
    service: &Service,
    pending: Mutation,
    result: Result<a::Answer, a::Error>,
    domain: &fake::Domain,
) -> Response {
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => return crate::writes::refused_mutation(error, &service.limits),
    };
    let a::Answer::Done = answer else { return answers::failed() };
    if !pending.creating {
        return crate::auth::empty();
    }
    let Some(rule) = domain.inspect_repository_protection(&pending.repository) else { return answers::failed() };
    let bypass = if rule.bypassing_users.contains(&service.bot) {
        skein_forge::api::Bypass::Always
    } else {
        skein_forge::api::Bypass::Never
    };
    let mut response = answers::response(201, &Body::Ruleset(bypass), &service.limits);
    let id = skein_lib::Decimal::of(service.ruleset_id);
    let Some(size) = response.body.len().checked_add(id.as_bytes().len()) else { return answers::failed() };
    let Some(size) = size.checked_add(6) else { return answers::failed() };
    if size > usize::try_from(service.limits.response).expect("u32 fits") {
        return answers::failed();
    }
    let Some(tail) = response.body.strip_prefix(b"{") else { return answers::failed() };
    let mut out = skein_lib::Writer::new(size);
    for part in [b"{\"id\":".as_slice(), id.as_bytes(), b",", tail] {
        if out.put(part).is_err() {
            return answers::failed();
        }
    }
    response.body = out.finish();
    response
}

enum Key {
    Name(Box<[u8]>),
    End,
}
struct Cursor {
    tokens: List<Token>,
    at: u32,
}
impl Cursor {
    fn take(&mut self) -> Option<&Token> {
        let at = self.at;
        self.at = self.at.checked_add(1)?;
        self.tokens.get(at)
    }
    fn want(&mut self, token: &Token) -> Option<()> {
        if self.take()? != token {
            return None;
        }
        Some(())
    }
    fn text(&mut self) -> Option<Box<[u8]>> {
        if let Token::String(value) = self.take()? { Some(value.clone()) } else { None }
    }
    fn number(&mut self) -> Option<u64> {
        if let Token::Number(value) = self.take()? { skein_forge::forgejo::id::unsigned(value) } else { None }
    }
    fn key(&mut self) -> Option<Key> {
        let token = self.take()?;
        if token == &Token::ObjectEnd {
            return Some(Key::End);
        }
        if let Token::Key(value) = token {
            return Some(Key::Name(value.clone()));
        }
        None
    }
    fn end(&self) -> bool {
        self.at == self.tokens.len()
    }
}
fn once(seen: &mut u32, bit: u32) -> Option<()> {
    if *seen & bit != 0 {
        return None;
    }
    *seen |= bit;
    Some(())
}
// Inferred request subset from GitHub docs; live sessions did not record a request body.
fn protection(service: &Service, body: &[u8]) -> Option<a::Protection> {
    if !service.body_fits(body) {
        return None;
    }
    let mut cursor = Cursor { tokens: crate::manifest::tokenize(body, &service.limits.github)?, at: 0 };
    let mut rule = a::Protection {
        branch: Box::new([]),
        contexts: Box::new([]),
        approvals: 0,
        dismiss_stale: false,
        bypassing_users: Box::new([]),
        queue: false,
        linear: false,
        code_owners: false,
    };
    cursor.want(&Token::ObjectStart)?;
    let mut seen = 0_u32;
    for _ in 0..cursor.tokens.len() {
        let Key::Name(key) = cursor.key()? else { break };
        match key.as_ref() {
            b"name" => {
                once(&mut seen, 1)?;
                if cursor.text()?.is_empty() {
                    return None;
                }
            }
            b"target" => {
                once(&mut seen, 2)?;
                if cursor.text()?.as_ref() != b"branch" {
                    return None;
                }
            }
            b"enforcement" => {
                once(&mut seen, 4)?;
                if cursor.text()?.as_ref() != b"active" {
                    return None;
                }
            }
            b"conditions" => {
                once(&mut seen, 8)?;
                rule.branch = branch(&mut cursor)?;
            }
            b"bypass_actors" => {
                once(&mut seen, 16)?;
                rule.bypassing_users = actors(&mut cursor, service)?;
            }
            b"rules" => {
                once(&mut seen, 32)?;
                rules(&mut cursor, &mut rule, service.limits.github.inner)?;
            }
            _ => return None,
        }
    }
    if seen != 63 || !cursor.end() {
        return None;
    }
    Some(rule)
}
fn branch(c: &mut Cursor) -> Option<Box<[u8]>> {
    c.want(&Token::ObjectStart)?;
    c.want(&Token::Key(bytes::copy_of(b"ref_name")))?;
    c.want(&Token::ObjectStart)?;
    c.want(&Token::Key(bytes::copy_of(b"include")))?;
    c.want(&Token::ArrayStart)?;
    let branch = c.text()?;
    let branch = bytes::copy_of(branch.strip_prefix(b"refs/heads/")?);
    if branch.is_empty() {
        return None;
    }
    c.want(&Token::ArrayEnd)?;
    c.want(&Token::ObjectEnd)?;
    c.want(&Token::ObjectEnd)?;
    Some(branch)
}
fn actors(c: &mut Cursor, service: &Service) -> Option<Box<[u64]>> {
    let mut actors = List::with_capacity(service.limits.github.inner);
    c.want(&Token::ArrayStart)?;
    for _ in 0..c.tokens.len() {
        if c.tokens.get(c.at)? == &Token::ArrayEnd {
            c.want(&Token::ArrayEnd)?;
            return Some(actors.into_boxed());
        }
        c.want(&Token::ObjectStart)?;
        let (mut id, mut kind, mut mode) = (None, None, None);
        for _ in 0..c.tokens.len() {
            let Key::Name(key) = c.key()? else { break };
            match key.as_ref() {
                b"actor_id" if id.is_none() => id = Some(c.number()?),
                b"actor_type" if kind.is_none() => kind = Some(c.text()?),
                b"bypass_mode" if mode.is_none() => mode = Some(c.text()?),
                _ => return None,
            }
        }
        if mode?.as_ref() != b"always" {
            return None;
        }
        let id = id?;
        let user = match kind?.as_ref() {
            b"Integration" if id == service.app.id => service.bot,
            b"User" => id,
            _ => return None,
        };
        actors.push(user).ok()?;
    }
    None
}
fn rules(c: &mut Cursor, rule: &mut a::Protection, bound: u32) -> Option<()> {
    c.want(&Token::ArrayStart)?;
    let mut seen = 0_u32;
    for _ in 0..c.tokens.len() {
        if c.tokens.get(c.at)? == &Token::ArrayEnd {
            c.want(&Token::ArrayEnd)?;
            return Some(());
        }
        c.want(&Token::ObjectStart)?;
        let (mut kind, mut parameters) = (None, None);
        for _ in 0..c.tokens.len() {
            let Key::Name(key) = c.key()? else { break };
            match key.as_ref() {
                b"type" if kind.is_none() => kind = Some(c.text()?),
                b"parameters" if parameters.is_none() => parameters = Some(parameter(c, bound)?),
                _ => return None,
            }
        }
        match (kind?.as_ref(), parameters) {
            (b"pull_request", Some(Parameter::Approvals(count))) => {
                once(&mut seen, 1)?;
                rule.approvals = count;
            }
            (b"required_status_checks", Some(Parameter::Contexts(contexts))) => {
                once(&mut seen, 2)?;
                rule.contexts = contexts;
            }
            (b"merge_queue", None) => {
                once(&mut seen, 4)?;
                rule.queue = true;
            }
            _ => return None,
        }
    }
    None
}

#[derive(Debug)]
enum Parameter {
    Approvals(u32),
    Contexts(Box<[Box<[u8]>]>),
}
fn parameter(c: &mut Cursor, bound: u32) -> Option<Parameter> {
    c.want(&Token::ObjectStart)?;
    let Key::Name(key) = c.key()? else { return None };
    let value = match key.as_ref() {
        b"required_approving_review_count" => Parameter::Approvals(u32::try_from(c.number()?).ok()?),
        b"required_status_checks" => {
            c.want(&Token::ArrayStart)?;
            let mut contexts = List::with_capacity(bound);
            for _ in 0..c.tokens.len() {
                if c.tokens.get(c.at)? == &Token::ArrayEnd {
                    break;
                }
                c.want(&Token::ObjectStart)?;
                c.want(&Token::Key(bytes::copy_of(b"context")))?;
                contexts.push(c.text()?).ok()?;
                c.want(&Token::ObjectEnd)?;
            }
            c.want(&Token::ArrayEnd)?;
            Parameter::Contexts(contexts.into_boxed())
        }
        _ => return None,
    };
    c.want(&Token::ObjectEnd)?;
    Some(value)
}
