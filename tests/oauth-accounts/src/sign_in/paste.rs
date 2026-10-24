//! The person's pasted address and the ordering of its browser callback.
use super::{Client, Env, Fact, Story, accounts};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Callback {
    Waiting,
    Accepted,
    Finished,
}

pub(super) fn story(story: Story) -> bool {
    matches!(
        story,
        Story::Paste
            | Story::PasteThenListener
            | Story::ListenerThenPaste
            | Story::PasteTwice
            | Story::PasteWrong
            | Story::StateRefusals
            | Story::PasteExpired
            | Story::PasteCancelled
            | Story::PasteClose
    )
}

pub(super) fn stops_at_issuer(story: Story) -> bool {
    super::confidential(story)
        || matches!(
            story,
            Story::Paste
                | Story::PasteTwice
                | Story::PasteWrong
                | Story::PasteExpired
                | Story::PasteCancelled
                | Story::PasteClose
        )
}

pub(super) fn delivery_ready(story: Story, callback: Callback, browser_done: bool, facts: &[Fact]) -> bool {
    if story == Story::PasteThenListener {
        callback == Callback::Accepted
    } else if story == Story::ListenerThenPaste {
        browser_done && facts.contains(&Fact::Granted(1))
    } else {
        true
    }
}

pub(super) fn callback_permitted(story: Story, facts: &[Fact]) -> bool {
    if story == Story::PasteThenListener {
        facts.contains(&Fact::Taken)
    } else if story == Story::StateRefusals {
        facts.contains(&Fact::Refused(accounts::Refusal::Full { bound: accounts::STATE_REFUSALS }))
    } else {
        true
    }
}

pub(super) fn wrong_state(uri: &[u8]) -> Box<[u8]> {
    let split = uri.iter().position(|byte| *byte == b'?').expect("redirect query");
    let mut edited = uri[..=split].to_vec();
    let mut changed = 0;
    for (index, field) in uri[split + 1..].split(|byte| *byte == b'&').enumerate() {
        if index > 0 {
            edited.push(b'&');
        }
        if field.starts_with(b"state=") {
            edited.extend_from_slice(b"state=another-state");
            changed += 1;
        } else {
            edited.extend_from_slice(field);
        }
    }
    assert_eq!(changed, 1, "one state field edited");
    edited.into_boxed_slice()
}

enum Wrong {
    Host,
    Port,
    Path,
    Fragment,
    DuplicateState,
    Long,
}
impl Wrong {
    fn edit(&self, uri: &[u8], url_bytes: u32) -> Box<[u8]> {
        let (mut address, _, target) = super::browser::url_parts(uri);
        match self {
            Self::Host => {
                let mut edited = format!("http://localhost:{}", address.port()).into_bytes();
                edited.extend_from_slice(target);
                edited.into_boxed_slice()
            }
            Self::Port => {
                address.set_port(address.port().checked_add(1).expect("fixture port"));
                let mut edited = format!("http://{address}").into_bytes();
                edited.extend_from_slice(target);
                edited.into_boxed_slice()
            }
            Self::Path => {
                let query = target.iter().position(|byte| *byte == b'?').expect("redirect query");
                let mut edited = format!("http://{address}/wrong").into_bytes();
                edited.extend_from_slice(&target[query..]);
                edited.into_boxed_slice()
            }
            Self::Fragment | Self::DuplicateState | Self::Long => {
                let mut edited = uri.to_vec();
                match self {
                    Self::Fragment => edited.extend_from_slice(b"#fragment"),
                    Self::DuplicateState => edited.extend_from_slice(b"&state=another-state"),
                    Self::Long => edited.resize(usize::try_from(url_bytes).expect("URL bound") + 1, b'x'),
                    Self::Host | Self::Port | Self::Path => unreachable!("address edits returned"),
                }
                edited.into_boxed_slice()
            }
        }
    }
}

impl Client {
    pub(super) fn paste(&mut self, env: &Env<accounts::Limits>, uri: Box<[u8]>) {
        match self.story {
            Story::PasteExpired => {
                self.expired_paste = Some(uri);
                return;
            }
            Story::PasteCancelled => self.ask(env, accounts::Request::Cancel { account: 0 }),
            Story::PasteWrong => {
                for wrong in
                    [Wrong::Host, Wrong::Port, Wrong::Path, Wrong::Fragment, Wrong::DuplicateState, Wrong::Long]
                {
                    self.ask(
                        env,
                        accounts::Request::Redirected {
                            account: 0,
                            uri: wrong.edit(&uri, self.limits.client.url_bytes),
                        },
                    );
                }
            }
            Story::StateRefusals => {
                for _ in 0..accounts::STATE_REFUSALS {
                    self.ask(env, accounts::Request::Redirected { account: 0, uri: wrong_state(&uri) });
                }
            }
            Story::Return(_)
            | Story::OpenIdAgain
            | Story::IssuedClientId
            | Story::AnotherSubject
            | Story::RequiredScopes
            | Story::Paste
            | Story::PasteThenListener
            | Story::ListenerThenPaste
            | Story::PasteTwice
            | Story::ListenerWrongState
            | Story::PasteClose
            | Story::Public
            | Story::VaryingTaken
            | Story::TwoPorts
            | Story::Confidential
            | Story::WrongPath
            | Story::WrongHost
            | Story::LongHead
            | Story::CloseWaiting
            | Story::CloseConfidential
            | Story::Cancel
            | Story::Abort
            | Story::NotKept
            | Story::Timeout => {}
        }
        self.ask(env, accounts::Request::Redirected { account: 0, uri: uri.clone() });
        if self.story == Story::PasteTwice {
            self.ask(env, accounts::Request::Redirected { account: 0, uri });
        }
    }
}
