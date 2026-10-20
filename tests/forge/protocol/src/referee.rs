//! Independent call ledger and retained forbidden sinks at public boundaries.
use skein_forge::credential::Credential;
use skein_forge_connection::{Access, Answered, Settings};
use skein_lib::Token;
use std::collections::BTreeMap;

/// Call costs and independent HTTP stream/entrance observations, excluding starts.
#[derive(Debug)]
pub struct Accounting {
    pub reported: u64,
    pub written: u64,
    pub served: u64,
    pub cut_in_head: u64,
}

struct Submitted {
    generation: u64,
    client: Token,
    terminal: Option<Answered>,
}

#[derive(Default)]
pub(crate) struct Referee {
    calls: BTreeMap<u64, Submitted>,
    needles: Vec<Vec<u8>>,
    records: Vec<Vec<u8>>,
    facts: Vec<Vec<u8>>,
    frozen: bool,
    token_handed: bool,
    secret_handed: bool,
    app_keys: Vec<Vec<u8>>,
    jwts: Vec<Vec<u8>>,
    installation_tokens: Vec<Vec<u8>>,
}

impl Referee {
    pub(crate) fn settings(&mut self, settings: &Settings) {
        for forge in &settings.forges {
            if let Some(access) = &forge.access {
                self.access(access);
            }
        }
    }

    pub(crate) fn access(&mut self, access: &Access) {
        match &access.credential {
            Credential::Token(secret) => {
                self.token_handed |= !secret.is_empty();
                self.needle(secret.expose());
            }
            Credential::App { key, .. } => {
                if !self.app_keys.iter().any(|known| known == key.expose()) {
                    self.app_keys.push(key.expose().to_vec());
                }
                self.needle(key.expose());
                let pem = skein_fake_forge_github::keys::pem(key.expose()).expect("public test key PEM");
                self.needle(&pem);
            }
        }
        if let Some(secret) = &access.webhook_secret {
            self.secret_handed |= !secret.is_empty();
            self.needle(secret.expose());
        }
    }

    pub(crate) fn bearer(&mut self, bytes: &[u8]) {
        let observed = if bytes.starts_with(b"ghs_") {
            &mut self.installation_tokens
        } else {
            assert_eq!(skein_lib::bytes::count(bytes, b".", 3), 2, "JWT observed on the stream");
            &mut self.jwts
        };
        if !observed.iter().any(|known| known == bytes) {
            observed.push(bytes.to_vec());
        }
        self.needle(bytes);
    }

    pub(crate) fn credential_counts(&self) -> (usize, usize, usize) {
        (self.app_keys.len(), self.jwts.len(), self.installation_tokens.len())
    }

    fn needle(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let debug = format!("{bytes:?}");
            let mut variants =
                vec![bytes.to_vec(), debug.trim_start_matches('[').trim_end_matches(']').as_bytes().to_vec()];
            if let Ok(text) = std::str::from_utf8(bytes) {
                variants.push(format!("{text:?}").trim_matches('"').as_bytes().to_vec());
            }
            for needle in variants {
                if !self.needles.contains(&needle) {
                    self.needles.push(needle);
                }
            }
        }
    }

    pub(crate) fn record(&mut self, value: String) {
        self.records.push(value.into_bytes());
    }

    pub(crate) fn fact(&mut self, value: String) {
        self.facts.push(value.into_bytes());
    }

    pub(crate) fn submit(&mut self, token: u64, generation: u64, client: Token) {
        assert!(!self.frozen, "no submission after settlement");
        assert!(
            self.calls.insert(token, Submitted { generation, client, terminal: None }).is_none(),
            "fresh owner identity"
        );
    }

    pub(crate) fn terminal(&mut self, answer: &Answered, generation: u64, client: Token) {
        let submitted = self.calls.get_mut(&answer.call.raw()).expect("terminal names submitted identity");
        assert_eq!(submitted.generation, generation);
        assert_eq!(submitted.client, client);
        assert!(submitted.terminal.replace(answer.clone()).is_none(), "exactly one terminal per identity");
        self.fact(format!("{answer:?}"));
    }

    pub(crate) fn outstanding(&self) -> usize {
        self.calls.values().filter(|call| call.terminal.is_none()).count()
    }

    pub(crate) fn freeze(&mut self) {
        assert_eq!(self.outstanding(), 0, "all submitted tokens have one terminal");
        // Every terminal is attached to a submitted identity, and replacement is rejected.
        assert_eq!(self.counts().0, self.counts().1, "submitted and terminal identity sets agree");
        self.frozen = true;
    }

    pub(crate) fn counts(&self) -> (u64, u64) {
        (
            u64::try_from(self.calls.len()).expect("submitted count"),
            u64::try_from(self.calls.values().filter(|call| call.terminal.is_some()).count()).expect("terminal count"),
        )
    }

    pub(crate) fn reported(&self) -> u64 {
        self.calls.values().filter_map(|call| call.terminal.as_ref()).map(|answer| u64::from(answer.cost)).sum()
    }

    pub(crate) fn secrets_absent(&self, lines: &[Vec<u8>], events: &[String], fake: &str) -> bool {
        assert!(self.frozen, "settled world");
        if self.app_keys.is_empty() {
            assert!(self.token_handed && self.secret_handed, "Forgejo world handed both secrets");
        } else {
            assert!(
                !self.jwts.is_empty() && !self.installation_tokens.is_empty(),
                "GitHub world saw a key, JWT and token"
            );
        }
        assert!(!self.records.is_empty() && !lines.is_empty() && !self.facts.is_empty(), "nonempty forbidden sinks");
        self.records
            .iter()
            .chain(&self.facts)
            .map(Vec::as_slice)
            .chain(lines.iter().map(Vec::as_slice))
            .chain(events.iter().map(String::as_bytes))
            .chain([fake.as_bytes()])
            .all(|value| self.needles.iter().all(|needle| !value.windows(needle.len()).any(|window| window == needle)))
    }
}

#[cfg(test)]
mod tests {
    use super::Referee;
    use crate::github_fixture;

    #[test]
    fn credentials_discovered_later_are_rejected_in_each_retained_sink() {
        let key = skein_fake_forge_github::keys::KEYS[1];
        let pem = skein_fake_forge_github::keys::pem(key).expect("public test key PEM");
        let jwt = b"header.payload.signature";
        let token = b"ghs_referee_calibration";
        let escaped = format!("{:?}", std::str::from_utf8(&pem).expect("PEM text"));
        for secret in [key, &pem, escaped.as_bytes(), jwt, token] {
            for sink in 0..5 {
                let mut referee = Referee::default();
                referee.records.push(b"record".to_vec());
                referee.facts.push(b"fact".to_vec());
                let mut lines = vec![b"GET / HTTP/1.1".to_vec()];
                let mut events = vec!["event".to_owned()];
                let mut fake = "fake observation".to_owned();
                match sink {
                    0 => referee.records.push(secret.to_vec()),
                    1 => referee.facts.push(secret.to_vec()),
                    2 => lines.push(secret.to_vec()),
                    3 => events.push(String::from_utf8_lossy(secret).into_owned()),
                    4 => fake = String::from_utf8_lossy(secret).into_owned(),
                    _ => unreachable!("five sinks"),
                }
                referee.access(&github_fixture::access(key));
                referee.bearer(jwt);
                referee.bearer(token);
                referee.freeze();
                // Binary DER reaches a String sink as its Debug byte representation.
                if secret == key && sink >= 3 {
                    let debug = format!("{key:?}");
                    if sink == 3 {
                        events = vec![debug];
                    } else {
                        fake = debug;
                    }
                }
                assert!(!referee.secrets_absent(&lines, &events, &fake), "sink {sink}");
            }
        }
    }
}
