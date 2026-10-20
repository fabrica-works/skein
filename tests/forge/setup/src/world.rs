use crate::{exchange, fixtures};
use skein_fake_forge_protocol::{Service, Tally, setup::Store};
use skein_forge::forgejo::{document::Document, request::Request};
use skein_forge_setup::item::Item;
use skein_forge_setup::{
    given::Given,
    reconcile::{self, Event, Failure, Need, Outcome, Reconcile, Reply},
};
use skein_lib::{Env, Queue, Rng, Secret, Time, Wall};
use std::net::SocketAddr;
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Ended {
    Done,
    Stop(Failure),
}
pub struct World {
    given: Given,
    service: Service,
    env: Env<skein_fake_forge_protocol::Limits>,
    files: Vec<(Item, Secret)>,
    rng: Rng,
    trace: Vec<String>,
    requests: Vec<Request>,
    secrets: Vec<Secret>,
    outcome: Option<Outcome>,
}
impl World {
    #[must_use]
    pub fn managed(seed: u64) -> Self {
        Self::new(seed, false)
    }
    #[must_use]
    pub fn owned(seed: u64) -> Self {
        Self::new(seed, true)
    }
    /// A managed picker with ten repositories and the given page cap.
    #[must_use]
    pub fn managed_paged(seed: u64, cap: u32) -> Self {
        let mut config = fixtures::config_with_cap(cap);
        fixtures::picker_repositories(&mut config);
        let mut world = Self::with_config(seed, false, config);
        fixtures::picker_owners(world.store_mut());
        world
    }
    fn new(seed: u64, owned: bool) -> Self {
        Self::with_config(seed, owned, fixtures::config())
    }
    /// A scenario whose repository identities are fixed before the fake starts.
    #[must_use]
    pub fn with_config(seed: u64, owned: bool, config: skein_fake_forge_protocol::Config) -> Self {
        let (service, limits) = seeded_config(seed, owned, config);
        Self {
            given: fixtures::given(owned),
            service,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            files: vec![(Item::AdminToken, fixtures::secret(fixtures::ADMIN))],
            rng: Rng::new(seed),
            trace: Vec::new(),
            requests: Vec::new(),
            secrets: vec![fixtures::secret(fixtures::ADMIN)],
            outcome: None,
        }
    }
    pub fn listening(&mut self, address: SocketAddr) {
        self.given.listener = Some(address);
    }
    #[must_use]
    pub fn file(&self, file: Item) -> Option<&Secret> {
        self.files.iter().find(|(f, _)| *f == file).map(|(_, value)| value)
    }
    pub fn put(&mut self, file: Item, bytes: Secret) {
        if matches!(file, Item::AdminToken | Item::ApiToken | Item::GitToken | Item::WebhookSecret) {
            self.secrets.push(bytes.clone());
        }
        if let Some((_, value)) = self.files.iter_mut().find(|(f, _)| *f == file) {
            *value = bytes;
        } else {
            self.files.push((file, bytes));
        }
    }
    pub fn forget(&mut self, item: Item) {
        self.files.retain(|(file, _)| *file != item);
    }
    #[must_use]
    pub fn store(&self) -> &Store {
        self.service.setup()
    }
    pub fn store_mut(&mut self) -> &mut Store {
        self.service.setup_mut()
    }
    #[must_use]
    pub fn tally(&self) -> &Tally {
        self.service.tally()
    }
    #[must_use]
    pub fn trace(&self) -> &[String] {
        &self.trace
    }
    #[must_use]
    pub fn requests(&self) -> &[Request] {
        &self.requests
    }
    #[must_use]
    pub fn secrets_absent(&self) -> bool {
        let mut checked = Vec::new();
        for secret in self.secrets.iter().chain(self.service.setup().issued().iter().map(|token| &token.secret)) {
            if secret.is_empty() || checked.contains(&secret) {
                continue;
            }
            if self.trace.iter().any(|text| text.as_bytes().windows(secret.len()).any(|bytes| bytes == secret.expose()))
            {
                return false;
            }
            checked.push(secret);
        }
        true
    }
    #[must_use]
    pub fn adopted_ids(&self) -> &[u64] {
        &self.given.adopted
    }
    pub fn adopted(&mut self, ids: &[u64]) {
        self.given.adopted = ids.into();
    }
    /// Switches the service using this forge, with its own token files.
    pub fn service_word(&mut self, word: &[u8]) {
        self.given.word = word.into();
        self.given.bot_login = word.into();
        self.given.api_name = [word, b"-api"].concat().into();
        self.given.git_name = [word, b"-git"].concat().into();
        self.files.retain(|(item, _)| *item == Item::AdminToken);
    }
    pub fn adopt(&mut self, repository: u64) -> Ended {
        let mut machine = skein_forge_setup::adopt::Adopt::new(&self.given, repository).expect("fixture admitted");
        self.drive(|event, out| skein_forge_setup::adopt::step(&mut machine, event, out))
    }
    pub fn reconcile(&mut self) -> Ended {
        let mut machine = Reconcile::new(&self.given).expect("fixture admitted");
        self.drive(|event, out| reconcile::step(&mut machine, event, out))
    }
    pub fn check(&mut self) -> Ended {
        let mut machine = skein_forge_setup::check::Check::new(1);
        self.drive(|event, out| skein_forge_setup::check::step(&mut machine, event, out))
    }
    pub fn reachable(&mut self, installation: Option<u64>, page: u32) -> Ended {
        let mut machine =
            skein_forge_setup::reachable::Reachable::new(&self.given, installation, page).expect("fixture admitted");
        self.drive(|event, out| skein_forge_setup::reachable::step(&mut machine, event, out))
    }
    pub fn take_over(&mut self, token: &[u8]) -> Ended {
        let secret = fixtures::secret(token);
        self.secrets.push(secret.clone());
        let mut machine = skein_forge_setup::take_over::TakeOver::new(&self.given, secret).expect("fixture admitted");
        self.drive(|event, out| skein_forge_setup::take_over::step(&mut machine, event, out))
    }
    #[must_use]
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }
    fn drive(&mut self, mut step: impl FnMut(Event, &mut Queue<Need>)) -> Ended {
        self.secrets.extend(self.service.setup().issued().iter().map(|token| token.secret.clone()));
        let mut out = Queue::with_capacity(reconcile::MAX_OUT);
        let mut event = Event::Begin;
        for _ in 0..4096 {
            self.trace.push(format!("{event:?}"));
            step(event, &mut out);
            assert_eq!(out.len(), 1, "one outstanding need");
            let need = out.pop().expect("one need");
            self.trace.push(format!("{need:?}"));
            event = match need {
                Need::Send { request, token } => {
                    if let Request::CreateUser { password, .. } = &request {
                        self.secrets.push(password.clone());
                    }
                    self.requests.push(request.clone());
                    let reply = exchange::send(&mut self.service, &self.env, &request, &token, self.rng.next_u64());
                    if let Reply::Document(Document::Token(token)) = &reply
                        && let Some(secret) = &token.value
                    {
                        self.secrets.push(secret.clone());
                    }
                    Event::Reply(reply)
                }
                Need::Read(item) => Event::Read(self.file(item).cloned()),
                Need::Write { item, bytes } => {
                    self.put(item, bytes);
                    Event::Written
                }
                Need::Draw(count) => {
                    let mut bytes = Vec::new();
                    for _ in 0..count {
                        bytes.push(self.rng.next_u64().to_le_bytes()[0]);
                    }
                    Event::Drawn(Secret::new(bytes.into_boxed_slice()))
                }
                Need::Done(outcome) => {
                    self.outcome = Some(outcome);
                    return Ended::Done;
                }
                Need::Stop(failure) => return Ended::Stop(failure),
            };
        }
        panic!("plan did not end")
    }
}

pub(crate) fn seeded(seed: u64, owned: bool) -> (Service, skein_fake_forge_protocol::Limits) {
    seeded_config(seed, owned, fixtures::config())
}
fn seeded_config(
    seed: u64,
    owned: bool,
    config: skein_fake_forge_protocol::Config,
) -> (Service, skein_fake_forge_protocol::Limits) {
    let limits = skein_forge_machine_world::fixtures::fake_limits();
    let mut service = Service::new(config, &limits, seed).expect("world service");
    let admin = service.setup_mut().account(b"kit-admin", true, b"");
    service.setup_mut().token(admin, b"setup-admin", &[b"all"], fixtures::secret(fixtures::ADMIN));
    if owned {
        service.setup_mut().account(b"ada", true, b"Ada");
    }
    (service, limits)
}
