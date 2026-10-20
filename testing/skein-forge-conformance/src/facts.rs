//! The facts file vocabulary shared by the forge tools.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Holds,
    Observed(String),
    Fails(String),
    Unchecked(String),
}
#[derive(Debug)]
pub struct Fact {
    pub name: &'static str,
    pub outcome: Outcome,
}
impl Fact {
    #[must_use]
    pub fn line(&self) -> String {
        match &self.outcome {
            Outcome::Holds => format!("holds {}\n", self.name),
            Outcome::Observed(seen) => {
                format!("holds {}\n# {}: {}\n", self.name, self.name, seen.replace(['\r', '\n'], " "))
            }
            Outcome::Fails(why) => format!("fails {}: {}\n", self.name, why.replace(['\r', '\n'], " ")),
            Outcome::Unchecked(why) => format!("unchecked {}: {}\n", self.name, why.replace(['\r', '\n'], " ")),
        }
    }
}
pub fn add(facts: &mut Vec<Fact>, name: &'static str, holds: bool, seen: &str) {
    facts.push(Fact { name, outcome: if holds { Outcome::Holds } else { Outcome::Fails(seen.into()) } });
}
