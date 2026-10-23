//! Outside sign-in outcomes and issuer document order.
use super::{Client, Fact, ISSUED_CLIENT, Judge, Story, accounts, bytes, oauth, peer};
const SIGN_INS: usize = 2;
const FIRST_GENERATION: u64 = 1;
const SECOND_GENERATION: u64 = FIRST_GENERATION + 1;

impl Judge {
    pub(super) fn client_closed(&self, client: &Client, issuer: &peer::Peer) {
        assert_eq!(client.facts.last(), Some(&Fact::Closed));
        if self.private_failure {
            assert!(client.facts.iter().any(|fact| matches!(fact, Fact::Failed(accounts::Failure::Unloaded { .. }))));
            assert!(client.visits.is_empty(), "unsafe loading fails before Visit");
        } else {
            match self.story {
                Story::Cancel | Story::Abort => assert!(
                    client.facts.contains(&Fact::Failed(accounts::Failure::Exchange(oauth::Failure::Cancelled)))
                ),
                Story::Timeout => {
                    assert!(
                        client.facts.contains(&Fact::Failed(accounts::Failure::Exchange(oauth::Failure::TimedOut)))
                    );
                }
                Story::NotKept => {
                    assert!(client.facts.contains(&Fact::Failed(accounts::Failure::NotKept)));
                    assert!(!client.facts.iter().any(|fact| matches!(fact, Fact::SignedIn(_) | Fact::Granted(_))));
                }
                Story::RequiredScopes => {
                    assert_eq!(
                        client.facts,
                        vec![
                            Fact::Visit,
                            Fact::Keep(1),
                            Fact::SignedIn(1),
                            Fact::Missing(bytes::copy_of(b"write")),
                            Fact::Failed(accounts::Failure::NotGranted),
                            Fact::Closed,
                        ]
                    );
                }
                Story::Public => {
                    assert!(client.facts.contains(&Fact::Granted(2)));
                    assert_eq!(client.records.len(), 2);
                }
                Story::AnotherSubject => {
                    assert_eq!(
                        client.facts,
                        vec![Fact::Visit, Fact::Failed(accounts::Failure::AnotherSubject), Fact::Closed]
                    );
                    assert!(client.records.is_empty());
                }
                Story::OpenIdAgain | Story::Return(super::Return::After) => {
                    assert_eq!(client.facts.iter().filter(|fact| matches!(fact, Fact::Visit)).count(), SIGN_INS);
                    assert!(client.facts.contains(&Fact::SignedIn(SECOND_GENERATION)));
                    document_visits(issuer.observations(), SIGN_INS);
                }
                Story::IssuedClientId => {
                    assert!(
                        client
                            .records
                            .first()
                            .is_some_and(|record| record.issued_client_id.as_deref() == Some(ISSUED_CLIENT))
                    );
                }
                Story::Confidential => {
                    assert!(client.facts.contains(&Fact::SignedIn(1)));
                    assert!(client.facts.contains(&Fact::Late));
                }
                Story::Return(_)
                | Story::VaryingTaken
                | Story::TwoPorts
                | Story::WrongPath
                | Story::WrongHost
                | Story::LongHead
                | Story::CloseWaiting
                | Story::CloseConfidential => {
                    assert!(client.facts.contains(&Fact::SignedIn(1)));
                }
            }
        }
    }
}
fn document_visits(observations: &[peer::Observation], visits: usize) {
    let targets: Vec<&[u8]> = observations
        .iter()
        .filter_map(|observation| match observation {
            peer::Observation::Document { target, .. } | peer::Observation::Authorize { target, .. } => {
                Some(target.split(|byte| *byte == b'?').next().expect("path"))
            }
            peer::Observation::Accepted { .. }
            | peer::Observation::Post { .. }
            | peer::Observation::Revoke { .. }
            | peer::Observation::Redirect { .. }
            | peer::Observation::Answered { .. }
            | peer::Observation::Closed { .. } => None,
        })
        .collect();
    assert_eq!(targets, [b"/.well-known/openid-configuration".as_slice(), b"/keys", b"/authorize"].repeat(visits));
}
