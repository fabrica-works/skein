use crate::{Failure, Forgejo, Settings, binary::environment, instance::ini};
use std::{
    fmt::{self, Write},
    fs::{self, File},
    io::Read,
    net::TcpListener,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
/// What a Forgejo command did; invalid UTF-8 is replaced.
pub struct Output {
    /// The exit code, or none when a signal ended it.
    pub code: Option<i32>,
    /// Standard output, which may contain secrets.
    pub stdout: String,
    /// Standard error, which may contain secrets.
    pub stderr: String,
}
impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Output")
            .field("code", &self.code)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .finish()
    }
}
/// An isolated scratch directory for Forgejo's command line, without a web process.
pub struct WorkPath {
    forgejo: Forgejo,
    scratch: PathBuf,
    pub(crate) port: u16,
    pub(crate) secrets: Vec<String>,
}
impl Forgejo {
    /// Prepares the scratch, its isolated environment and app.ini.
    /// # Errors
    /// Refuses a scratch under `/tmp` or one that is not empty.
    pub fn work_path(&self, scratch: &Path, settings: &Settings) -> Result<WorkPath, Failure> {
        let scratch = prepare(scratch)?;
        let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
        let secret = random()?;
        let internal = random()?;
        fs::create_dir(scratch.join("home"))?;
        fs::set_permissions(scratch.join("home"), fs::Permissions::from_mode(0o700))?;
        fs::create_dir(scratch.join("tmp"))?;
        fs::write(scratch.join("gitconfig"), b"")?;
        fs::write(scratch.join("app.ini"), ini(&scratch, port, settings, &run_user()?, &secret, &internal))?;
        Ok(WorkPath { forgejo: self.clone(), scratch, port, secrets: vec![secret, internal] })
    }
}
impl WorkPath {
    /// Runs a command in this work path, retaining its code and both streams.
    /// # Errors
    /// Returns a spawn or pipe-read failure.
    pub fn run(&self, args: &[&str]) -> Result<Output, Failure> {
        let output = self
            .command()
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(std::process::Child::wait_with_output)
            .map_err(|e| Failure::Spawn { step: "cli", error: e.to_string() })?;
        Ok(Output {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
    /// The canonical scratch directory.
    #[must_use]
    pub fn scratch(&self) -> &Path {
        &self.scratch
    }
    pub(crate) fn command(&self) -> Command {
        let mut cmd = Command::new(self.forgejo.binary());
        environment(&mut cmd, &self.scratch);
        cmd.current_dir(&self.scratch)
            .arg("--work-path")
            .arg(&self.scratch)
            .arg("--config")
            .arg(self.scratch.join("app.ini"));
        cmd
    }
    pub(crate) fn checked(&self, args: &[&str], step: &'static str, secrets: &[String]) -> Result<(), Failure> {
        let output = self.run(args)?;
        if output.code != Some(0) {
            let mut log_tail = output.stderr;
            for secret in secrets {
                if !secret.is_empty() {
                    log_tail = log_tail.replace(secret, "MASKED");
                }
            }
            return Err(Failure::Exited { step, status: format!("{:?}", output.code), log_tail });
        }
        Ok(())
    }
}
pub(crate) fn random() -> Result<String, Failure> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut hex = String::new();
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("string write");
    }
    Ok(hex)
}
fn run_user() -> Result<String, Failure> {
    let uid = fs::metadata("/proc/self")?.uid().to_string();
    let passwd = fs::read_to_string("/etc/passwd")?;
    let user = passwd.lines().find(|line| line.split(':').nth(2) == Some(uid.as_str()));
    let missing = || Failure::Io("process uid is absent from /etc/passwd".into());
    user.and_then(|line| line.split(':').next()).map(str::to_owned).ok_or_else(missing)
}
fn prepare(path: &Path) -> Result<PathBuf, Failure> {
    let refuse = |why: &str| Failure::Scratch { path: path.into(), why: why.into() };
    let absolute = std::path::absolute(path)?;
    if absolute.starts_with("/tmp") {
        return Err(refuse("scratch is under /tmp"));
    }
    fs::create_dir_all(&absolute)?;
    let absolute = fs::canonicalize(absolute)?;
    if absolute.starts_with("/tmp") {
        return Err(refuse("scratch resolves under /tmp"));
    }
    if fs::read_dir(&absolute)?.next().is_some() {
        return Err(refuse("scratch is not empty"));
    }
    Ok(absolute)
}
