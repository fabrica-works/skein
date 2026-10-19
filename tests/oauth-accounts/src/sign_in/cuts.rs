//! Private sign-in from an absent root, cut before every submission (io.md, 8).
use super::*;
use crate::private::FileSystem;
use crate::world::{Client as Recovery, Process as RecoveryProcess};
use skein_io::kernel::{Complete, Fd, Op, Submit};
use skein_lib::Token;
use skein_world::{Cut, Inherited, StartupRoot};

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
const SIGNAL: Token = Token::new(0x7fff_ffff_ffff_ffff);

enum Inner {
    SignIn(super::Process),
    Recovery(RecoveryProcess),
}
impl Inner {
    fn host(&mut self) -> &mut dyn Host {
        match self {
            Self::SignIn(process) => process,
            Self::Recovery(process) => process,
        }
    }
    fn view(&self) -> &dyn Host {
        match self {
            Self::SignIn(process) => process,
            Self::Recovery(process) => process,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Signal {
    Held,
    Closing,
    Closed,
}

struct Process {
    inner: Inner,
    root: Option<Fd>,
    start_sign_in: bool,
    signal: Fd,
    signal_state: Signal,
    ending: bool,
    issued: u32,
    opened_at: Option<u32>,
    stored_at: Option<u32>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
}
impl Process {
    fn new(inner: Inner, inherited: &Inherited) -> Self {
        Self {
            inner,
            root: inherited.roots.first().map(|(_, fd)| *fd),
            start_sign_in: false,
            signal: inherited.signal,
            signal_state: Signal::Held,
            ending: false,
            issued: 0,
            opened_at: None,
            stored_at: None,
            completions: Queue::with_capacity(ROOM),
            submissions: Queue::with_capacity(ROOM),
        }
    }
}
impl Host for Process {
    fn iterate(&mut self, now: Time, wall: Wall) {
        if self.start_sign_in {
            self.start_sign_in = false;
            let mut client = Client::with_limits(Story::Confidential, false, &limits(), 8);
            client.file_driver = Some(skein_world::files::Driver::new(
                self.root.expect("owner startup root"),
                4,
                client.limits.client.document.record_bytes,
                client.limits.file_stall,
                1234,
            ));
            self.inner = Inner::SignIn(super::Process::Client(Box::new(client)));
        }
        while let Some(complete) = self.completions.pop() {
            if complete.op == SIGNAL {
                assert!(complete.result.is_ok());
                self.signal_state = Signal::Closed;
            } else {
                self.inner.host().completions().push(complete);
            }
        }
        self.inner.host().iterate(now, wall);
        if let Inner::SignIn(super::Process::Client(client)) = &self.inner {
            if client.opened_private.is_some() && self.opened_at.is_none() {
                self.opened_at = Some(self.issued);
            }
            if client.stored_private.is_some() && self.stored_at.is_none() {
                self.stored_at = Some(self.issued);
            }
        }
        while let Some(submit) = self.inner.host().submissions().pop() {
            assert_ne!(submit.op, SIGNAL, "wrapper namespace");
            self.submissions.push(submit);
        }
        if self.inner.view().is_empty() && self.signal_state == Signal::Held && !self.inner.view().work_pending(now) {
            self.signal_state = Signal::Closing;
            self.submissions.push(Submit { op: SIGNAL, kind: Op::Close { fd: self.signal } });
        }
        self.issued += self.submissions.len();
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        !self.completions.is_empty()
            || self.inner.view().work_pending(now)
            || (self.inner.view().is_empty() && self.signal_state == Signal::Held)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.inner.view().next_deadline()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        self.inner.view().next_policy_deadline()
    }
    fn is_empty(&self) -> bool {
        self.signal_state == Signal::Closed
            && self.inner.view().is_empty()
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.inner.view().worst_case()
            + 512_000
            + Queue::<Complete>::worst_case(ROOM).expect("wrapper completions")
            + Queue::<Submit>::worst_case(ROOM).expect("wrapper submissions")
    }
    fn operations(&self) -> u32 {
        self.inner.view().operations() + 1
    }
}
fn limits() -> accounts::Limits {
    let mut limits = crate::world::limits();
    limits.accounts = 1;
    limits.exchanges = 1;
    limits.listeners = 0;
    limits.io.sockets = 1;
    limits.io.intake = skein_tls::client::LARGEST_READ;
    limits.io.receive = 256;
    limits.io.output = 2048;
    limits.io.sends = 1;
    limits.http.request = 1024;
    limits.http.head = 512;
    limits.http.headers = 8;
    limits.http.read = 128;
    limits.http.send = 128;
    limits.tls.read = 512;
    limits.tls.send = 1024;
    limits.tls.records = skein_tls::client::MAX_RECORD;
    limits.client.document.document_bytes = 512;
    limits.client.document.string_bytes = 128;
    limits.client.document.token_bytes = 64;
    limits.client.document.record_bytes = 512;
    limits.client.document.id_token_bytes = 64;
    limits.client.document.tokens = 32;
    limits
}
fn recovery(root: Fd) -> Recovery {
    let mut profile = limits();
    profile.refresh_lead = Duration::ZERO;
    let mut client = Recovery::with_limits(crate::world::Story::NotKept, Transport::Plaintext, &profile, 8);
    client.file_driver =
        Some(skein_world::files::Driver::new(root, 3, profile.client.document.record_bytes, profile.file_stall, 1234));
    client
}
fn issuer(inherited: &Inherited) -> Process {
    Process::new(Inner::SignIn(super::issuer::with_access_token(Story::Confidential, b"new")), inherited)
}
fn browser(inherited: &Inherited) -> Process {
    Process::new(Inner::SignIn(super::Process::Browser(Box::new(Browser::new(Story::Confidential)))), inherited)
}
fn owner(inherited: &Inherited) -> Process {
    let root = inherited.roots.first().expect("owner startup root").1;
    Process::new(Inner::Recovery(RecoveryProcess::Client(Box::new(recovery(root)))), inherited)
}

struct Judge {
    original: bool,
    sign_in: super::Judge,
    wake: Option<Time>,
    passed: bool,
}
impl Referee<Process> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Process]) {
        if !self.original {
            procs[1].start_sign_in = true;
            self.original = true;
        }
        let address = match &procs[0].inner {
            Inner::SignIn(super::Process::Issuer(peer)) => peer.address(),
            Inner::SignIn(super::Process::Client(_) | super::Process::Browser(_)) | Inner::Recovery(_) => {
                unreachable!("issuer slot")
            }
        };
        if let Inner::Recovery(RecoveryProcess::Client(client)) = &mut procs[1].inner {
            client.address = address;
        }
        if matches!(procs[1].inner, Inner::SignIn(_)) {
            let mut visit = [0; 1024];
            let mut visit_len = 0;
            let mut redirect = [0; 1024];
            let mut redirect_len = 0;
            if let Inner::SignIn(super::Process::Browser(browser)) = &procs[2].inner
                && let Some(uri) = &browser.redirect
            {
                redirect_len = uri.len();
                redirect[..redirect_len].copy_from_slice(uri);
            }
            if let Inner::SignIn(super::Process::Client(client)) = &mut procs[1].inner {
                client.address = address;
                if let Some(url) = &client.visit {
                    visit_len = url.len();
                    visit[..visit_len].copy_from_slice(url);
                }
                if !self.sign_in.redirected && redirect_len > 0 {
                    client.redirected[..redirect_len].copy_from_slice(&redirect[..redirect_len]);
                    client.redirected_len = redirect_len;
                    self.sign_in.redirected = true;
                }
            }
            if !self.sign_in.delivered
                && visit_len > 0
                && let Inner::SignIn(super::Process::Browser(browser)) = &mut procs[2].inner
            {
                browser.inbox[..visit_len].copy_from_slice(&visit[..visit_len]);
                browser.inbox_len = visit_len;
                self.sign_in.delivered = true;
            }
        }
        if procs[1].inner.view().is_empty() && procs[2].inner.view().is_empty() && !procs[0].ending {
            if let Inner::SignIn(super::Process::Issuer(peer)) = &mut procs[0].inner {
                peer.shutdown();
            }
            procs[0].ending = true;
        }
    }
    fn observe(&mut self, now: Time, procs: &[Process]) {
        self.passed = procs.iter().all(Host::is_empty);
        let address_ready = match &procs[0].inner {
            Inner::SignIn(super::Process::Issuer(peer)) => peer.address().is_some(),
            Inner::SignIn(super::Process::Client(_) | super::Process::Browser(_)) | Inner::Recovery(_) => {
                unreachable!("issuer slot")
            }
        };
        let owner_ready = match &procs[1].inner {
            Inner::SignIn(super::Process::Client(client)) => {
                (client.component.is_none() && address_ready) || (client.visit.is_some() && !self.sign_in.delivered)
            }
            Inner::Recovery(RecoveryProcess::Client(client)) => client.unstarted() && address_ready,
            Inner::SignIn(super::Process::Issuer(_) | super::Process::Browser(_))
            | Inner::Recovery(RecoveryProcess::Issuer(_)) => unreachable!("owner slot"),
        };
        let redirect_ready = match &procs[2].inner {
            Inner::SignIn(super::Process::Browser(browser)) => {
                browser.redirect.is_some() && !self.sign_in.redirected && matches!(procs[1].inner, Inner::SignIn(_))
            }
            Inner::SignIn(super::Process::Client(_) | super::Process::Issuer(_)) | Inner::Recovery(_) => {
                unreachable!("browser slot")
            }
        };
        let shutdown_ready = procs[1].inner.view().is_empty() && procs[2].inner.view().is_empty() && !procs[0].ending;
        self.wake = (owner_ready || redirect_ready || shutdown_ready).then_some(now);
    }
    fn next_deadline(&self) -> Option<Time> {
        self.wake.or(Some(Time::from_nanos(180_000_000_000)))
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (now.as_nanos() >= 180_000_000_000 && !self.passed).then(|| "private sign-in cut did not settle".to_owned())
    }
    fn passed(&self) -> bool {
        self.passed
    }
}
fn world() -> World<Process, Judge, FileSystem> {
    let mut world = World::new(
        0,
        skein_sim::Config::calm(),
        Judge {
            original: false,
            sign_in: super::Judge {
                delivered: false,
                redirected: false,
                shutdown: false,
                wake: None,
                passed: false,
                story: Story::Confidential,
                private_failure: false,
            },
            wake: None,
            passed: false,
        },
        Memory::Checked,
    )
    .with_machine(FileSystem::absent_for_cuts());
    world.spawn_restartable(Vec::new(), issuer);
    world.spawn_restartable(vec![StartupRoot { name: bytes::copy_of(b"root"), path: bytes::copy_of(b"root") }], owner);
    world.spawn_restartable(Vec::new(), browser);
    world
}
#[test]
fn an_accounts_private_root_starting_absent_survives_every_acknowledged_power_loss_cut() {
    let mut uncut = world().run();
    let opened = uncut.procs[1].opened_at.expect("private open answered");
    let stored = uncut.procs[1].stored_at.expect("private store answered");
    let points: Vec<_> = uncut
        .trace
        .iter()
        .filter(|entry| entry.pid.raw() == 1)
        .filter_map(|entry| if let skein_sim::Event::Submit { op, .. } = &entry.event { Some(*op) } else { None })
        .enumerate()
        .filter(|(_, op)| skein_world::files::Driver::owns(*op))
        .map(|(before, _)| u32::try_from(before).expect("bounded submissions"))
        .collect();
    for before in points {
        {
            let (label, cut) = ("power loss", Cut::PowerLoss);
            let mut world = world();
            world.cut(1, before, cut);
            let mut outcome = world.run();
            assert_eq!(outcome.machine.cuts, 1, "{label}, before {before}");
            if before >= opened {
                assert!(outcome.machine.root_at_cut, "{label}, root acknowledged before {before}");
            }
            if before >= stored {
                let record = oauth::decode_record(
                    outcome.machine.record_at_cut.as_deref().expect("acknowledged record"),
                    &crate::world::limits().client.document,
                )
                .expect("whole acknowledged record");
                assert_eq!(record.generation, 1, "{label}, store before {before}");
                assert_eq!(record.access_token.as_ref(), b"new", "{label}, store before {before}");
            }
            outcome.machine.finish();
        }
    }
    uncut.machine.finish();
}
