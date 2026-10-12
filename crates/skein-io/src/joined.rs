//! Named listeners and their waiting connectors (io.md, 3.5).
use crate::layer::{Entity, Io, Tables};
use crate::{Addr, Error, Event, Name};
use core::mem;
use skein_lib::{Id, Queue, Token};

/// A name held until its listener's Closed.
#[derive(Debug)]
pub(crate) struct Listener {
    owner: Token,
    state: ListenerState,
}
#[derive(Debug)]
enum ListenerState {
    Starting { name: Name },
    Failed,
    Listening { name: Name },
    Closing { name: Name },
    Closed,
}

/// A connector waiting in request order; no operation slot is held.
#[derive(Debug)]
pub(crate) struct End {
    owner: Token,
    state: EndState,
}
#[derive(Debug)]
enum EndState {
    Starting { name: Name, order: u64 },
    Waiting { listener: Id<Entity>, order: u64 },
    Closing { waiting: Option<(Id<Entity>, u64)>, error: Option<Error> },
    Closed,
}

impl Listener {
    pub(crate) fn is_closed(&self) -> bool {
        match self.state {
            ListenerState::Closed => true,
            ListenerState::Starting { .. }
            | ListenerState::Listening { .. }
            | ListenerState::Closing { .. }
            | ListenerState::Failed => false,
        }
    }
}
impl End {
    pub(crate) fn is_closed(&self) -> bool {
        match self.state {
            EndState::Closed => true,
            EndState::Starting { .. } | EndState::Waiting { .. } | EndState::Closing { .. } => false,
        }
    }
}
fn listener(io: &mut Io, id: Id<Entity>) -> &mut Listener {
    match io.entities.get_mut(id).expect("live named listener") {
        Entity::Named(listener) => listener,
        Entity::Joined(_)
        | Entity::Listener(_)
        | Entity::Stream(_)
        | Entity::Pipe(_)
        | Entity::Child(_)
        | Entity::Signals(_) => unreachable!("a named listener handle"),
    }
}
fn end(io: &mut Io, id: Id<Entity>) -> &mut End {
    match io.entities.get_mut(id).expect("live joined end") {
        Entity::Joined(end) => end,
        Entity::Named(_)
        | Entity::Listener(_)
        | Entity::Stream(_)
        | Entity::Pipe(_)
        | Entity::Child(_)
        | Entity::Signals(_) => unreachable!("a joined end handle"),
    }
}

pub(crate) fn listen(io: &mut Io, owner: Token, name: Name) {
    if io.entities.is_full() || io.tables.names.len() == io.tables.names.capacity() {
        io.tables.refused.push(owner);
        return;
    }
    let valid = name != Name::EMPTY && !io.tables.names.contains_key(&name);
    let state = if valid { ListenerState::Starting { name } } else { ListenerState::Failed };
    let id = io.entities.insert(Entity::Named(Listener { owner, state })).expect("a listener has a free slot");
    if valid {
        let previous = io.tables.names.insert(name, id).expect("a name has room");
        assert!(previous.is_none(), "a new listener takes an unheld name");
    }
    io.tables.ready.mark(id);
}

pub(crate) fn connect(io: &mut Io, owner: Token, name: Name) {
    if io.entities.is_full() {
        io.tables.refused.push(owner);
        return;
    }
    let order = io.tables.sequence;
    io.tables.sequence = order.checked_add(1).expect("connector order fits a u64");
    let state = EndState::Starting { name, order };
    let id = io.entities.insert(Entity::Joined(End { owner, state })).expect("a connector has a free slot");
    io.tables.ready.mark(id);
}

pub(crate) fn close_listener(listener: &mut Listener, id: Id<Entity>, tables: &mut Tables) {
    listener.state = match mem::replace(&mut listener.state, ListenerState::Closed) {
        ListenerState::Starting { name } | ListenerState::Listening { name } | ListenerState::Closing { name } => {
            ListenerState::Closing { name }
        }
        ListenerState::Failed => ListenerState::Failed,
        ListenerState::Closed => return,
    };
    tables.ready.mark(id);
}
pub(crate) fn close_end(end: &mut End, id: Id<Entity>, tables: &mut Tables) {
    end.state = match mem::replace(&mut end.state, EndState::Closed) {
        EndState::Starting { .. } => EndState::Closing { waiting: None, error: None },
        EndState::Waiting { listener, order } => EndState::Closing { waiting: Some((listener, order)), error: None },
        EndState::Closing { waiting, error } => EndState::Closing { waiting, error },
        EndState::Closed => return,
    };
    tables.ready.mark(id);
}

pub(crate) fn resume(io: &mut Io, id: Id<Entity>, up: &mut Queue<Event>) {
    match io.entities.get(id).expect("a ready entity is live") {
        Entity::Named(_) => resume_listener(io, id, up),
        Entity::Joined(_) => resume_end(io, id, up),
        Entity::Listener(_) | Entity::Stream(_) | Entity::Pipe(_) | Entity::Child(_) | Entity::Signals(_) => {
            unreachable!("a joined handler names a joined entity")
        }
    }
}
fn resume_listener(io: &mut Io, id: Id<Entity>, up: &mut Queue<Event>) {
    let current = listener(io, id);
    let owner = current.owner;
    let state = mem::replace(&mut current.state, ListenerState::Closed);
    let state = match state {
        ListenerState::Starting { name } => {
            up.push(Event::Listening { owner, listener: id.token(), addr: Addr::InProcess(name) });
            ListenerState::Listening { name }
        }
        ListenerState::Failed => {
            up.push(Event::Failed { owner, error: Error::Other });
            up.push(Event::Closed { owner });
            ListenerState::Closed
        }
        ListenerState::Listening { name } => ListenerState::Listening { name },
        ListenerState::Closing { name } => refuse_waiting(io, id, owner, name, up),
        ListenerState::Closed => ListenerState::Closed,
    };
    listener(io, id).state = state;
}
fn refuse_waiting(io: &mut Io, id: Id<Entity>, owner: Token, name: Name, up: &mut Queue<Event>) -> ListenerState {
    let mut first = None;
    if let Some((key, end)) = io.tables.waiting.range((id, 0)..=(id, u64::MAX)).next() {
        first = Some((*key, *end));
    }
    if let Some((key, end_id)) = first {
        let removed = io.tables.waiting.remove(&key);
        assert!(removed.is_some(), "a closing connector leaves the waiting table");
        let current = end(io, end_id);
        current.state = match mem::replace(&mut current.state, EndState::Closed) {
            EndState::Waiting { .. } => EndState::Closing { waiting: None, error: Some(Error::Refused) },
            EndState::Closing { error, .. } => EndState::Closing { waiting: None, error },
            EndState::Starting { .. } | EndState::Closed => unreachable!("only waiting connectors are queued"),
        };
        io.tables.ready.mark(end_id);
        io.tables.ready.mark(id);
        ListenerState::Closing { name }
    } else {
        up.push(Event::Closed { owner });
        let removed = io.tables.names.remove(&name);
        assert_eq!(removed, Some(id), "a listener frees its own name at Closed");
        ListenerState::Closed
    }
}
fn resume_end(io: &mut Io, id: Id<Entity>, up: &mut Queue<Event>) {
    let current = end(io, id);
    let owner = current.owner;
    let state = mem::replace(&mut current.state, EndState::Closed);
    let state = match state {
        EndState::Starting { name, order } => start(io, id, owner, name, order, up),
        EndState::Waiting { listener, order } => EndState::Waiting { listener, order },
        EndState::Closing { waiting, error } => {
            if let Some(key) = waiting {
                let removed = io.tables.waiting.remove(&key);
                assert!(removed.is_some(), "a closing connector leaves the waiting table");
            }
            if let Some(error) = error {
                up.push(Event::Failed { owner, error });
            }
            up.push(Event::Closed { owner });
            EndState::Closed
        }
        EndState::Closed => EndState::Closed,
    };
    end(io, id).state = state;
}
fn start(io: &mut Io, id: Id<Entity>, owner: Token, name: Name, order: u64, up: &mut Queue<Event>) -> EndState {
    up.push(Event::Connecting { owner, socket: id.token() });
    if let Some(bound) = io.tables.names.get(&name).copied() {
        let taking = match listener(io, bound).state {
            ListenerState::Starting { .. } | ListenerState::Listening { .. } => true,
            ListenerState::Closing { .. } | ListenerState::Failed | ListenerState::Closed => false,
        };
        if taking {
            let previous = io.tables.waiting.insert((bound, order), id).expect("at most sockets waiting connectors");
            assert!(previous.is_none(), "each connector has its own request order");
            return EndState::Waiting { listener: bound, order };
        }
    }
    up.push(Event::Failed { owner, error: Error::Refused });
    up.push(Event::Closed { owner });
    EndState::Closed
}
