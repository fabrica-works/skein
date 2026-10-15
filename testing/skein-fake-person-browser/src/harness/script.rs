//! Browser-specific script steps over one semantic face.
use super::{Play, ServerAction};
use crate::BrowserFace;
use skein_fake_person::{Next, Person};
use skein_lib::Time;
use std::collections::VecDeque;

pub enum Step {
    Person(skein_fake_person::Step<Box<[u8]>>),
    StopServer,
    StartServer,
    Title(Box<[u8]>),
    Warned(bool),
    Key(skein_browser::boundary::Key),
    Focused(skein_fake_person::Find),
    PressAt { find: skein_fake_person::Find, at: skein_browser::boundary::Point },
    Compose { find: skein_fake_person::Find, words: Box<[u8]> },
    Type { find: skein_fake_person::Find, words: Box<[u8]> },
    Back,
    Forward,
}
pub struct Script {
    steps: VecDeque<Step>,
    person: Option<Person<Box<[u8]>>>,
    at: u32,
    action: Option<ServerAction>,
    title: Option<Box<[u8]>>,
    focused: Option<skein_fake_person::Find>,
    press: Option<(skein_fake_person::Find, skein_browser::boundary::Point)>,
    text: Option<(skein_fake_person::Find, Box<[u8]>, bool)>,
}
impl Script {
    #[must_use]
    pub fn new(steps: Box<[Step]>) -> Self {
        Self {
            steps: steps.into_vec().into(),
            person: None,
            at: 0,
            action: None,
            title: None,
            focused: None,
            press: None,
            text: None,
        }
    }
    fn press_at(&mut self, face: &mut BrowserFace<'_>) -> Option<Next<Box<[u8]>>> {
        let (find, at) = self.press.take()?;
        match skein_fake_person::Face::find(face, &find) {
            skein_fake_person::Found::Pending => self.press = Some((find, at)),
            skein_fake_person::Found::Absent => {
                return Some(Next::Failed { step: self.at, why: skein_fake_person::Why::Missing { find } });
            }
            skein_fake_person::Found::Node(node) => {
                if let Err(why) = face.press_at(node, at) {
                    face.fail(why);
                }
                self.at += 1;
            }
        }
        Some(Next::Wait { until: None })
    }
    fn check_focus(&mut self, face: &mut BrowserFace<'_>) -> Option<Next<Box<[u8]>>> {
        if let Some(find) = self.focused.take() {
            match skein_fake_person::Face::find(face, &find) {
                skein_fake_person::Found::Pending => {
                    self.focused = Some(find);
                    return Some(Next::Wait { until: None });
                }
                skein_fake_person::Found::Absent => {
                    return Some(Next::Failed { step: self.at, why: skein_fake_person::Why::Missing { find } });
                }
                skein_fake_person::Found::Node(node) => {
                    if !face.focused(node) {
                        face.fail(format!("node {find:?} is not focused"));
                    }
                    self.at += 1;
                    return Some(Next::Wait { until: None });
                }
            }
        }
        None
    }
}
impl Play for Script {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        if let Some(next) = self.press_at(face) {
            return next;
        }
        if let Some((find, words, compose)) = self.text.take() {
            match skein_fake_person::Face::find(face, &find) {
                skein_fake_person::Found::Pending => self.text = Some((find, words, compose)),
                skein_fake_person::Found::Absent => {
                    return Next::Failed { step: self.at, why: skein_fake_person::Why::Missing { find } };
                }
                skein_fake_person::Found::Node(node) => {
                    if let Err(why) = face.text(node, words, compose) {
                        face.fail(why);
                    }
                    self.at += 1;
                }
            }
            return Next::Wait { until: None };
        }
        if let Some(next) = self.check_focus(face) {
            return next;
        }
        if let Some(title) = self.title.take() {
            match face.title(&title) {
                skein_fake_person::Found::Pending => {
                    self.title = Some(title);
                    return Next::Wait { until: None };
                }
                skein_fake_person::Found::Absent => {
                    return Next::Failed {
                        step: self.at,
                        why: skein_fake_person::Why::Missing {
                            find: skein_fake_person::Find::exactly_named(skein_ui::Role::Page, &title),
                        },
                    };
                }
                skein_fake_person::Found::Node(_) => {
                    self.at += 1;
                }
            }
        }
        if let Some(person) = &mut self.person {
            let next = person.next(face, now);
            match next {
                Next::Done => {
                    self.at += person.at();
                    self.person = None;
                }
                Next::Failed { step, why } => return Next::Failed { step: self.at + step, why },
                other @ (Next::Do(_) | Next::Wait { .. }) => return other,
            }
        }
        let Some(step) = self.steps.pop_front() else {
            return Next::Done;
        };
        match step {
            Step::Key(key) => {
                face.key(key);
                self.at += 1;
            }
            Step::PressAt { find, at } => self.press = Some((find, at)),
            Step::Focused(find) => self.focused = Some(find),
            Step::Compose { find, words } => self.text = Some((find, words, true)),
            Step::Type { find, words } => self.text = Some((find, words, false)),
            Step::Back | Step::Forward => {
                face.history(matches!(step, Step::Forward));
                self.at += 1;
            }
            Step::Person(step) => {
                let mut steps = vec![step];
                while matches!(self.steps.front(), Some(Step::Person(_))) {
                    let Some(Step::Person(step)) = self.steps.pop_front() else { unreachable!("person step") };
                    steps.push(step);
                }
                self.person = Some(Person::new(steps.into()));
            }
            Step::Warned(expected) => {
                let warned = face.warned();
                if warned != expected {
                    face.fail(format!("last go warned {warned}, expected {expected}"));
                }
                self.at += 1;
            }
            Step::Title(words) => {
                self.title = Some(words);
            }
            Step::StopServer => {
                self.action = Some(ServerAction::Stop);
                self.at += 1;
            }
            Step::StartServer => {
                self.action = Some(ServerAction::Start);
                self.at += 1;
            }
        }
        Next::Wait { until: None }
    }
    fn at(&self) -> u32 {
        self.at + self.person.as_ref().map_or(0, Person::at)
    }
    fn server_action(&mut self) -> Option<ServerAction> {
        self.action.take()
    }
}

#[cfg(test)]
mod tests {
    use super::{Play, Script, Step};
    use crate::tests::{face, found, seen};
    use skein_browser::boundary::{Key, Request};
    use skein_fake_person::Find;
    use skein_lib::{Time, Token};
    use skein_ui::Role;

    #[test]
    fn a_key_step_sends_the_kits_key() {
        let mut script = Script::new(Box::new([Step::Key(Key::Escape)]));
        let mut face = face();
        script.next(&mut face, Time::ZERO);
        assert_eq!(
            face.requests().collect::<Vec<_>>(),
            vec![Request::Key { page: Token::new(7), op: Token::new(7_000_001), key: Key::Escape }]
        );
    }
    #[test]
    fn a_point_press_uses_the_found_figures_node() {
        let mut script = Script::new(Box::new([Step::PressAt {
            find: Find::exactly_named(Role::Drawing, b"Notes by label"),
            at: skein_browser::boundary::Point { x: 128, y: 60 },
        }]));
        let mut face = face();
        script.next(&mut face, Time::ZERO);
        script.next(&mut face, Time::ZERO);
        found(&mut face, Some(seen(b"")));
        script.next(&mut face, Time::ZERO);
        assert_eq!(
            face.requests().collect::<Vec<_>>(),
            vec![Request::Press {
                page: Token::new(7),
                op: Token::new(7_000_002),
                node: 9_000_000_000,
                at: Some(skein_browser::boundary::Point { x: 128, y: 60 }),
            }]
        );
    }
    const FOCUS: &[(bool, Option<&str>)] = &[
        (true, None),
        (
            false,
            Some(
                "node Find { within: [], target: ExactRole { role: TextField, name: [84, 105, 116, 108, 101] } } is not focused",
            ),
        ),
    ];
    #[test]
    fn the_focus_step_checks_the_found_nodes_focus() {
        for &(focused, expected) in FOCUS {
            let mut script = Script::new(Box::new([Step::Focused(Find::exactly_named(Role::TextField, b"Title"))]));
            let mut face = face();
            script.next(&mut face, Time::ZERO);
            script.next(&mut face, Time::ZERO);
            let mut target = seen(b"");
            target.states.focused = focused;
            found(&mut face, Some(target));
            script.next(&mut face, Time::ZERO);
            assert_eq!(face.failure().as_deref(), expected, "focused {focused}");
        }
    }
}
