//! Signed webhook requests for a world to write (fake-forge.md, section 4).
use crate::{Limits, Service, routes};
use alloc::boxed::Box;
use skein_fake_forge_domain::{
    self as domain,
    api::{Change, Check},
};
use skein_forge::forgejo::{delivery, document::Body, request::ReviewEvent, server};
use skein_lib::{Decimal, Secret, Writer, sha256::Hmac};
/// One forge's destination and signing secret; no secret crosses into its domain.
#[derive(Debug)]
pub struct Endpoint {
    pub forge: u16,
    pub host: Box<[u8]>,
    pub secret: Secret,
}
/// A complete request, written head first and then body by the world.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Post {
    pub head: Box<[u8]>,
    pub body: Box<[u8]>,
}
impl Service {
    /// Renders a configured repository's hook; replies send nothing.
    #[must_use]
    pub fn delivery(&self, hook: &domain::Request, limits: &Limits) -> Option<Post> {
        let endpoint = self.config.hooks.as_ref()?;
        let (repository, change, number, branch, commit, by, body, ended) = match hook {
            domain::Request::Hook { repository, change, number, branch, commit, by, body, ended } => {
                (repository, change, number, branch, commit, by, body, ended)
            }
            domain::Request::Reply { .. } => return None,
        };
        let mut id = None;
        for known in &self.config.repositories {
            if known.full == *repository {
                id = Some(u64::from(known.id));
            }
        }
        let repository = id?;
        let value = match change {
            Change::Issue => {
                delivery::Delivery::Issue { repository, number: (*number)?, body: Body::Kept(body.as_ref()?.clone()) }
            }
            Change::Pull => delivery::Delivery::Pull { repository, number: (*number)? },
            Change::Comment => {
                delivery::Delivery::Comment { repository, number: (*number)?, body: Body::Kept(body.as_ref()?.clone()) }
            }
            Change::Review => delivery::Delivery::Review {
                event: ReviewEvent::Approved,
                repository,
                number: (*number)?,
                body: Body::Kept(body.as_ref()?.clone()),
            },
            Change::Push => {
                let branch = branch.as_ref()?;
                let mut reference = Writer::new(11usize.checked_add(branch.len())?);
                reference.put(b"refs/heads/").expect("prefix fits");
                reference.put(branch).expect("branch fits");
                match commit {
                    Some(commit) => delivery::Delivery::Push {
                        repository,
                        reference: reference.finish(),
                        after: routes::object(*commit),
                        pusher: (*by)?,
                    },
                    None => delivery::Delivery::Ref {
                        created: false,
                        repository,
                        reference: reference.finish(),
                        ref_type: delivery::RefType::Branch,
                    },
                }
            }
            Change::Job => {
                let ended = ended.as_ref()?;
                let end = match ended.state {
                    Check::Passed => delivery::JobEnd::Success,
                    Check::Failed => delivery::JobEnd::Failure,
                    Check::Pending => return None,
                };
                delivery::Delivery::Job { end, repository, commit: routes::object(ended.commit) }
            }
        };
        let sent = server::delivery::encode(&value, &limits.documents).ok()?;
        let target = delivery::hook_target(endpoint.forge);
        let length = Decimal::of(u64::try_from(sent.body.len()).ok()?);
        let signature = sign(&endpoint.secret, &sent.body);
        let parts: [&[u8]; 16] = [
            b"POST ",
            &target,
            b" HTTP/1.1\r\nHost: ",
            &endpoint.host,
            b"\r\nContent-Type: application/json\r\nContent-Length: ",
            length.as_bytes(),
            b"\r\n",
            delivery::EVENT_HEADER,
            b": ",
            sent.event,
            b"\r\n",
            delivery::SIGNATURE_HEADER,
            b": ",
            &signature,
            b"\r\n",
            b"\r\n",
        ];
        let mut size = 0usize;
        for part in parts {
            size = size.checked_add(part.len())?;
        }
        let mut head = Writer::new(size);
        for part in parts {
            head.put(part).expect("measured head");
        }
        Some(Post { head: head.finish(), body: sent.body })
    }
}
/// Forgejo's lowercase hexadecimal HMAC-SHA256 signature.
#[must_use]
pub fn sign(secret: &Secret, body: &[u8]) -> [u8; 64] {
    let mut hmac = Hmac::new(secret.expose());
    hmac.update(body);
    let mut signature = [0; 64];
    for (out, byte) in signature.as_chunks_mut::<2>().0.iter_mut().zip(hmac.finish()) {
        for (digit, nibble) in out.iter_mut().zip([byte / 16, byte % 16]) {
            *digit =
                if nibble < 10 { b'0'.saturating_add(nibble) } else { b'a'.saturating_add(nibble.saturating_sub(10)) };
        }
    }
    signature
}
