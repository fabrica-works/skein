//! A find scoped to a node from before a reload is Gone.
use super::{Event, FIND, PAGE, Refusal, Request, Rig, Stage, Token, query};
use skein_browser::boundary::Go;
const RELOAD: Token = Token::new(21);
impl Rig {
    pub(super) fn scope_event(&mut self, event: Event) -> Result<(), Event> {
        match event {
            Event::Opened { page } => {
                assert_eq!(page, PAGE);
                self.stage = Stage::Finding;
                self.ask(Request::Find { page: PAGE, op: FIND, query: query(b"region", b"Scope") });
            }
            Event::Found { op, seen, more } => {
                assert_eq!((self.stage, op, seen.len(), more), (Stage::Finding, FIND, 1, 0));
                self.scope = Some(seen.get(0).expect("named region").node);
                self.stage = Stage::Reloading;
                self.ask(Request::Go { page: PAGE, op: RELOAD, to: Go::Reload });
            }
            Event::Done { op } => {
                assert_eq!((self.stage, op), (Stage::Reloading, RELOAD));
                self.stage = Stage::Expecting;
                let mut heading = query(b"heading", b"Inside");
                heading.within = Some(self.scope.expect("region found before reload"));
                self.ask(Request::Find { page: PAGE, op: FIND, query: heading });
            }
            Event::Refused { op, why } => {
                assert_eq!((self.stage, op, why), (Stage::Expecting, FIND, Refusal::Gone));
                self.close_browser();
            }
            other => return Err(other),
        }
        Ok(())
    }
}
