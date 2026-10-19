//! Sign-in owner setup and outside request/terminal decisions.
use super::{Client, Env, Fact, List, Story, accounts, bytes, registration};
impl Client {
    fn visited(&mut self, env: &Env<accounts::Limits>) {
        match self.story {
            Story::CloseWaiting | Story::CloseConfidential => {
                self.close(env);
                assert!(!self.closed);
            }
            Story::Cancel => {
                self.ask(env, accounts::Request::Cancel { account: 0 });
                self.close(env);
            }
            Story::Abort => {
                self.close(env);
                self.ask(env, accounts::Request::Abort);
            }
            Story::RequiredScopes
            | Story::Public
            | Story::VaryingTaken
            | Story::TwoPorts
            | Story::Confidential
            | Story::WrongPath
            | Story::WrongHost
            | Story::LongHead
            | Story::NotKept
            | Story::Timeout => {}
        }
    }

    pub(super) fn start(&mut self, env: &Env<accounts::Limits>) {
        let count = if self.story == Story::TwoPorts { 2 } else { 1 };
        let mut configured = List::with_capacity(count);
        for _ in 0..count {
            assert!(
                configured
                    .push(accounts::Account::SignIn {
                        registration: registration(self.story),
                        endpoint: accounts::Endpoint {
                            address: self.address.expect("issuer address"),
                            transport: accounts::Transport::Plaintext
                        },
                        keeper: match &self.file_driver {
                            Some(driver) => accounts::Keeper::Private {
                                root: driver.root(),
                                directory: bytes::copy_of(b"secret"),
                                file: bytes::copy_of(b"record")
                            },
                            None => accounts::Keeper::Owner { kept: None },
                        }
                    })
                    .is_ok()
            );
        }
        self.component = Some(
            accounts::Component::new(
                configured,
                &self.limits,
                skein_lib::CryptoRng::new(&skein_world::crypto_seed(17), 0).expect("32-byte world seed"),
            )
            .expect("accounts"),
        );
        for account in 0..count {
            self.ask(env, accounts::Request::SignIn { account });
        }
    }
    pub(super) fn ask(&mut self, env: &Env<accounts::Limits>, request: accounts::Request) {
        self.component.as_mut().expect("component").down(
            env,
            request,
            &mut self.above,
            &mut self.requests,
            &mut self.files,
        );
    }
    pub(super) fn close(&mut self, env: &Env<accounts::Limits>) {
        if !self.closing {
            self.closing = true;
            self.ask(env, accounts::Request::Close);
        }
    }
    fn keep(&mut self, env: &Env<accounts::Limits>, account: u32, record: skein_oauth::SavedToken) {
        let generation = record.generation;
        self.contracts[usize::try_from(account).expect("account index")].keep(generation);
        self.facts.push(Fact::Keep(generation));
        let yes = self.story != Story::NotKept;
        let size = if self.large { 256 } else { 16 };
        assert_eq!(record.access_token.as_ref(), vec![if generation == 1 { b'a' } else { b'b' }; size]);
        self.contracts[usize::try_from(account).expect("account index")].kept(generation, yes);
        self.ask(
            env,
            accounts::Request::Kept {
                account,
                generation,
                keeping: if yes { accounts::Keeping::Kept } else { accounts::Keeping::NotKept },
            },
        );
        self.records.push(record);
    }
    fn signed_in(&mut self, env: &Env<accounts::Limits>, account: u32, generation: u64, missing: Box<[u8]>) {
        self.contracts[usize::try_from(account).expect("account index")].signed_in(generation);
        self.facts.push(Fact::SignedIn(generation));
        assert_eq!(generation, 1);
        if !missing.is_empty() {
            self.facts.push(Fact::Missing(missing));
        }
        if !self.closing {
            self.ask(env, accounts::Request::Grant { account });
        }
        if self.story == Story::Confidential {
            self.ask(
                env,
                accounts::Request::Redirected {
                    account,
                    uri: bytes::copy_of(b"https://service.example/callback?state=late&code=late"),
                },
            );
        }
    }
    pub(super) fn observe(&mut self, env: &Env<accounts::Limits>) {
        if let Some(event) = self.above.pop() {
            assert!(!self.closed, "Closed once and last");
            match event {
                accounts::Event::Visit { url, .. } => {
                    self.visits.push(url);
                    self.facts.push(Fact::Visit);
                    self.visited(env);
                }
                accounts::Event::Keep { account, record } => {
                    self.keep(env, account, record);
                }
                accounts::Event::SignedIn { account, generation, missing } => {
                    self.signed_in(env, account, generation, missing);
                }
                accounts::Event::Granted { account, generation, .. } => {
                    self.contracts[usize::try_from(account).expect("account index")].granted(generation);
                    self.facts.push(Fact::Granted(generation));
                    if generation == 1 && self.story == Story::Public {
                        self.ask(env, accounts::Request::Rejected { account, generation });
                    } else if self.story != Story::TwoPorts
                        || self.facts.iter().filter(|fact| matches!(fact, Fact::Granted(_))).count() == 2
                    {
                        self.close(env);
                    }
                }
                accounts::Event::Failed { account, ends, failure } => {
                    let expected = if self.story == Story::RequiredScopes {
                        accounts::Ends::Grant
                    } else {
                        accounts::Ends::SignIn
                    };
                    assert_eq!(ends, expected);
                    if ends == accounts::Ends::SignIn {
                        self.contracts[usize::try_from(account).expect("account index")].failed();
                    }
                    self.facts.push(Fact::Failed(failure));
                    self.close(env);
                }
                accounts::Event::Refused {
                    asked: accounts::Asked::Redirected,
                    why: accounts::Refusal::NotWaiting,
                    ..
                } => self.facts.push(Fact::Late),
                accounts::Event::Closed => {
                    for contracts in &mut self.contracts {
                        contracts.closed();
                    }
                    self.facts.push(Fact::Closed);
                    self.closed = true;
                    if let Some(driver) = &mut self.file_driver {
                        driver.close_root(env.now);
                    }
                }
                accounts::Event::Removed { .. }
                | accounts::Event::Refused { .. }
                | accounts::Event::SignedOut { .. }
                | accounts::Event::Expiring { .. } => {
                    panic!("unexpected sign-in owner event")
                }
            }
        }
    }
}
