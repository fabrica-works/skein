//! Ruling 132: await growth after measuring and typing in the same step.
use crate::BrowserFace;
use skein_fake_person::{Find, Found};
use skein_lib::{Duration, Time};

pub(super) struct Growth {
    find: Find,
    words: Option<Box<[u8]>>,
    before: Option<u64>,
    last: u64,
    until: Option<Time>,
    poll: Time,
}
impl Growth {
    pub(super) fn new(find: Find, words: Box<[u8]>) -> Self {
        Self { find, words: Some(words), before: None, last: 0, until: None, poll: Time::ZERO }
    }
    pub(super) fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> bool {
        if let Some(why) = self.expired(now) {
            face.fail(why);
            return true;
        }
        if now < self.poll {
            return false;
        }
        let node = match face.find_boxed(&self.find) {
            Found::Pending => return false,
            Found::Absent => {
                face.fail(format!("missing field {:?}", self.find));
                return true;
            }
            Found::Node(node) => node,
        };
        let Some(height) = face.height(node) else {
            face.fail(format!("field {:?} has no box", self.find));
            return true;
        };
        self.last = height;
        if let Some(before) = self.before {
            if height > before {
                return true;
            }
            self.poll = now.saturating_add(super::super::POLL);
        } else {
            self.before = Some(height);
            self.until = Some(now.saturating_add(Duration::from_secs(1)));
            if let Err(why) = face.text(node, self.words.take().expect("words typed once"), false) {
                face.fail(why);
                return true;
            }
        }
        false
    }
    pub(super) fn expired(&self, now: Time) -> Option<String> {
        self.until.filter(|until| now >= *until).map(|_| {
            format!(
                "field height {} did not exceed {} within 1 s",
                self.last,
                self.before.expect("measured before typing")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Growth;
    use crate::{
        BrowserFace,
        tests::{face, found, seen},
    };
    use skein_browser::boundary::{Event, Rect, Request, Seen};
    use skein_fake_person::Find;
    use skein_lib::{Duration, Time, Token};
    use skein_ui::Role;

    fn target(height: u64) -> Seen {
        let mut target = seen(b"");
        target.rect = Some(Rect { x: 0, y: 0, width: 100, height });
        target
    }
    fn growth() -> Growth {
        Growth::new(Find::exactly_named(Role::TextArea, b"Body"), b"one\ntwo\nthree".as_slice().into())
    }
    fn measured() -> (BrowserFace<'static>, Growth) {
        let (mut face, mut growth) = (face(), growth());
        growth.next(&mut face, Time::ZERO);
        found(&mut face, Some(target(20)));
        growth.next(&mut face, Time::ZERO);
        (face, growth)
    }
    fn typed() -> (BrowserFace<'static>, Growth) {
        let (mut face, growth) = measured();
        let Request::Type { op, .. } = face.requests().next().unwrap() else { panic!("type") };
        face.event(&Event::Done { op });
        (face, growth)
    }
    #[test]
    fn growth_queries_ask_for_boxes() {
        let (mut face, mut growth) = (face(), growth());
        growth.next(&mut face, Time::ZERO);
        let Request::Find { query, .. } = face.requests().next().unwrap() else { panic!("find") };
        assert_eq!(
            query,
            skein_browser::boundary::Query {
                role: b"textbox".as_slice().into(),
                name: b"Body".as_slice().into(),
                matching: skein_browser::boundary::Match::Exact,
                within: None,
                boxes: true,
            }
        );
    }
    #[test]
    fn growth_types_at_the_caret_after_measuring() {
        let (mut face, _) = measured();
        assert_eq!(
            face.requests().collect::<Vec<_>>(),
            vec![Request::Type {
                page: Token::new(7),
                op: Token::new(7_000_002),
                node: 9_000_000_000,
                text: b"one\ntwo\nthree".as_slice().into(),
            }]
        );
    }
    #[test]
    fn growth_completes_when_the_new_box_is_taller() {
        let (mut face, mut growth) = typed();
        growth.next(&mut face, Time::ZERO);
        found(&mut face, Some(target(40)));
        assert!(growth.next(&mut face, Time::ZERO));
    }
    const POLLS: &[(u64, usize)] = &[(1, 0), (9, 0), (10, 1)];
    #[test]
    fn growth_waits_for_the_poll_interval_before_querying_again() {
        for &(milliseconds, expected) in POLLS {
            let (mut face, mut growth) = typed();
            growth.next(&mut face, Time::ZERO);
            found(&mut face, Some(target(20)));
            growth.next(&mut face, Time::ZERO);
            growth.next(&mut face, Time::ZERO.saturating_add(Duration::from_millis(milliseconds)));
            assert_eq!(face.requests().count(), expected, "{milliseconds} ms");
        }
    }
    #[test]
    fn growth_times_out_naming_both_heights() {
        let (mut face, mut growth) = typed();
        growth.next(&mut face, Time::ZERO);
        found(&mut face, Some(target(18)));
        growth.next(&mut face, Time::ZERO);
        assert_eq!(
            growth.expired(Time::ZERO.saturating_add(Duration::from_secs(1))).as_deref(),
            Some("field height 18 did not exceed 20 within 1 s")
        );
    }
    #[test]
    fn the_growth_deadline_is_checked_while_a_browser_act_is_pending() {
        use super::super::{Play, Script, Step};
        let mut script = Script::new(Box::new([Step::Taller {
            find: Find::exactly_named(Role::TextArea, b"Body"),
            words: b"one".as_slice().into(),
        }]));
        let mut face = face();
        script.next(&mut face, Time::ZERO);
        script.next(&mut face, Time::ZERO);
        found(&mut face, Some(target(20)));
        script.next(&mut face, Time::ZERO);
        script.check_deadline(&mut face, Time::ZERO.saturating_add(Duration::from_secs(1)));
        assert_eq!(face.failure().as_deref(), Some("field height 20 did not exceed 20 within 1 s"));
    }
}
