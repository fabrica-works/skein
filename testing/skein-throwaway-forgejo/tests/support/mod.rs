use skein_throwaway_forgejo::{Instance, Settings, Token};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub struct Attached {
    pub instance: Instance,
    pub pid: u32,
    pub scratch: PathBuf,
}
pub fn attached(login: &str, id: u64) -> Attached {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("attached-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("scratch");
    fs::write(scratch.join("keep"), b"mine").expect("owner file");
    let mut child = Command::new(env!("CARGO_BIN_EXE_attached-peer"))
        .args([login, &id.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .expect("scripted peer");
    let pid = child.id();
    let mut port = String::new();
    BufReader::new(child.stdout.take().expect("peer stdout")).read_line(&mut port).expect("peer listening");
    let instance = Instance::attach(
        child,
        port.trim().parse().expect("port"),
        &scratch,
        Token::new("owner-secret".into()),
        &Settings::small(),
    )
    .expect("attached");
    Attached { instance, pid, scratch }
}
