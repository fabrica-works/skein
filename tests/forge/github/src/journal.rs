//! Write-ahead intentions contain identifiers and times, never token values.
use crate::Result;
use skein_forge_conformance::write::{self, Secrets};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub(crate) enum Made {
    Branch { repo: String, name: String },
    Pull { repo: String, head: String, base: String },
    Issue { repo: String, title: String },
    Rules { repo: String, name: String },
    Access { installation: u64, repository: u64, owner: String, add: bool },
    Mint { installation: u64, at: u64 },
}
impl Made {
    fn line(&self) -> String {
        match self {
            Self::Branch { repo, name } => format!("branch\t{repo}\t{name}"),
            Self::Pull { repo, head, base } => format!("pull\t{repo}\t{head}\t{base}"),
            Self::Issue { repo, title } => format!("issue\t{repo}\t{title}"),
            Self::Rules { repo, name } => format!("rules\t{repo}\t{name}"),
            Self::Access { installation, repository, owner, add } => {
                format!("access\t{installation}\t{repository}\t{owner}\t{add}")
            }
            Self::Mint { installation, at } => format!("mint\t{installation}\t{at}"),
        }
    }
}
pub(crate) struct Journal {
    path: PathBuf,
    pub pending: Vec<(usize, Made)>,
    next: usize,
    pub numbers: std::collections::BTreeMap<usize, u64>,
}
impl Journal {
    pub fn create(out: &Path, live: bool, secrets: &Secrets) -> Result<Self> {
        fs::create_dir_all(out)?;
        let path = out.join("made.txt");
        let bytes = if live { b"github-conformance-v1\tlive\n".as_slice() } else { b"github-conformance-v1\tfake\n" };
        write::check("journal", bytes, b"", secrets)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::File::open(out)?.sync_all()?;
        Ok(Self { path, pending: vec![], next: 0, numbers: std::collections::BTreeMap::new() })
    }
    fn append(&self, line: &str, secrets: &Secrets) -> Result<()> {
        write::check("journal", line.as_bytes(), b"", secrets)?;
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        file.sync_all()?;
        Ok(())
    }
    pub fn before(&mut self, made: Made, secrets: &Secrets) -> Result<usize> {
        let row = made.line();
        if row.contains(['\r', '\n']) {
            return Err("invalid journal field".into());
        }
        self.next += 1;
        self.append(&format!("make\t{}\t{row}", self.next), secrets)?;
        self.pending.push((self.next, made));
        Ok(self.next)
    }
    pub fn created(&mut self, id: usize, number: u64, secrets: &Secrets) -> Result<()> {
        self.append(&format!("number\t{id}\t{number}"), secrets)?;
        self.numbers.insert(id, number);
        Ok(())
    }
    pub fn cancel(&mut self, id: usize, secrets: &Secrets) -> Result<()> {
        self.append(&format!("cancel\t{id}"), secrets)?;
        self.pending.retain(|(key, _)| *key != id);
        Ok(())
    }
    pub fn done(&mut self, id: usize, secrets: &Secrets) -> Result<()> {
        self.append(&format!("done\t{id}"), secrets)?;
        self.pending.retain(|(key, _)| *key != id);
        Ok(())
    }
    pub fn load(out: &Path) -> Result<(Self, bool)> {
        let path = out.join("made.txt");
        let mut data = vec![];
        fs::File::open(&path)?.take(1_048_577).read_to_end(&mut data)?;
        if data.len() > 1_048_576 {
            return Err("journal exceeds its bound".into());
        }
        // Only complete, synced intentions can have caused an external change.
        let complete = data.iter().rposition(|byte| *byte == b'\n').ok_or("journal header incomplete")? + 1;
        if complete < data.len() {
            let file = OpenOptions::new().write(true).open(&path)?;
            file.set_len(u64::try_from(complete)?)?;
            file.sync_all()?;
        }
        let text = std::str::from_utf8(&data[..complete])?;
        let mut lines = text.lines();
        let live = match lines.next() {
            Some("github-conformance-v1\tlive") => true,
            Some("github-conformance-v1\tfake") => false,
            _ => return Err("unknown journal format".into()),
        };
        let mut result = Self { path, pending: vec![], next: 0, numbers: std::collections::BTreeMap::new() };
        for line in lines {
            let parts: Vec<_> = line.split('\t').collect();
            match parts.as_slice() {
                ["number", id, number] => {
                    let id: usize = id.parse()?;
                    if !result.pending.iter().any(|(key, _)| *key == id) {
                        return Err("number has no pending intention".into());
                    }
                    result.numbers.insert(id, number.parse()?);
                }
                ["done" | "cancel", id] => {
                    let id: usize = id.parse()?;
                    result.pending.retain(|(key, _)| *key != id);
                }
                ["make", id, tail @ ..] => {
                    let id: usize = id.parse()?;
                    if id <= result.next {
                        return Err("journal order invalid".into());
                    }
                    result.next = id;
                    let made = match tail {
                        ["branch", repo, name] => Made::Branch { repo: (*repo).into(), name: (*name).into() },
                        ["pull", repo, head, base] => {
                            Made::Pull { repo: (*repo).into(), head: (*head).into(), base: (*base).into() }
                        }
                        ["issue", repo, title] => Made::Issue { repo: (*repo).into(), title: (*title).into() },
                        ["rules", repo, name] => Made::Rules { repo: (*repo).into(), name: (*name).into() },
                        ["access", i, r, o, add] => Made::Access {
                            installation: i.parse()?,
                            repository: r.parse()?,
                            owner: (*o).into(),
                            add: add.parse()?,
                        },
                        ["mint", i, at] => Made::Mint { installation: i.parse()?, at: at.parse()? },
                        _ => return Err("journal intention invalid".into()),
                    };
                    result.pending.push((id, made));
                }
                _ => return Err("journal record invalid".into()),
            }
        }
        Ok((result, live))
    }
}
