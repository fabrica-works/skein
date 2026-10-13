//! Shared private sign-in issuer fixtures (oauth.md, 6.8).
use super::{Duration, Ipv4Addr, Process, ROOM, Story, Transport, bytes, fake, oauth, peer, registration};

pub(super) fn issuer(story: Story, large: bool) -> Process {
    make(story, large, None)
}

#[cfg(test)]
pub(super) fn with_access_token(story: Story, access: &[u8]) -> Process {
    make(story, false, Some(access))
}

fn make(story: Story, large: bool, access_token: Option<&[u8]>) -> Process {
    let registration = registration(story);
    let profile = crate::world::limits();
    let mut issuer = peer::Peer::new(
        (Ipv4Addr::LOCALHOST, 31000).into(),
        Transport::Plaintext,
        skein_fake_peers::Limits {
            io: profile.io,
            connections: 2,
            queue: ROOM,
            plaintext: 32_768,
            ciphertext: 32_768,
            observations: 16,
            observation_bytes: 4096,
        },
        fake::Config {
            authorization_url: registration.authorization_url,
            token_endpoint: registration.token_endpoint,
            client_id: registration.client_id,
            client_secret: registration.client_secret,
            redirect_uri: registration.redirect_uri,
            refresh_token: if large { vec![b's'; 256].into_boxed_slice() } else { bytes::copy_of(b"seed") },
        },
        fake::Limits {
            document: profile.client.document,
            uri_bytes: 256,
            request_bytes: 1024,
            codes: 1,
            rotations: 2,
            plans: 2,
        },
        profile.server,
    )
    .expect("issuer");
    for (access, refresh) in [(b'a', b"r1".as_slice()), (b'b', b"r2".as_slice())] {
        issuer
            .queue(fake::Plan {
                status: 200,
                body: fake::Body::Token(oauth::TokenResponse {
                    access_token: access_token
                        .map_or_else(|| vec![access; if large { 256 } else { 16 }].into_boxed_slice(), bytes::copy_of),
                    refresh_token: Some(if large {
                        vec![if access == b'a' { b'r' } else { b't' }; 256].into_boxed_slice()
                    } else {
                        bytes::copy_of(refresh)
                    }),
                    expires_in: 30,
                }),
                delay: Duration::from_millis(5),
                retry_after: Duration::ZERO,
            })
            .expect("plan");
    }
    Process::Issuer(Box::new(issuer))
}
