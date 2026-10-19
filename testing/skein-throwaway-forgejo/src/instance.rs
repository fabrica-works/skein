use crate::{Account, Exchange, Failure, Forgejo, Init, Made, Outgoing, Settings, Token, WorkPath, http};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Child,
};
pub struct Instance {
    child: Option<Child>,
    port: u16,
    scratch: PathBuf,
    work: Option<WorkPath>,
    admin: Account,
    settings: Settings,
    secrets: Vec<String>,
}
impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Instance").field(&self.port).field(&self.scratch).field(&self.admin).finish()
    }
}
impl Instance {
    pub(crate) fn start(forgejo: &Forgejo, scratch: &Path, settings: &Settings) -> Result<Self, Failure> {
        let work = forgejo.work_path(scratch, settings)?;
        let scratch = work.scratch().to_owned();
        let port = work.port;
        let mut instance = Self {
            child: None,
            work: Some(work),
            port,
            scratch,
            admin: Account {
                login: "fixture".into(),
                id: 0,
                token: Token::new(String::new()),
                password: crate::cli::random()?,
            },
            settings: settings.clone(),
            secrets: Vec::new(),
        };
        instance.secrets.extend(instance.work.as_ref().expect("prepared work path").secrets.clone());
        instance.secrets.push(instance.admin.password.clone());
        instance.work.as_ref().expect("prepared work path").checked(&["migrate"], "migrate", &instance.secrets)?;
        instance.work.as_ref().expect("prepared work path").checked(
            &[
                "admin",
                "user",
                "create",
                "--username",
                "fixture",
                "--password",
                &instance.admin.password,
                "--email",
                "fixture@example.invalid",
                "--admin",
                "--must-change-password=false",
            ],
            "admin",
            &instance.secrets,
        )?;
        let log = File::create(instance.scratch.join("server.log"))?;
        instance.child = Some(
            instance
                .work
                .as_ref()
                .expect("prepared work path")
                .command()
                .arg("web")
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()
                .map_err(|e| Failure::Spawn { step: "web", error: e.to_string() })?,
        );
        instance.ready()?;
        let password = instance.admin.password.clone();
        instance.admin.token = instance.token("fixture", &password)?;
        let user = instance.exchange(&Outgoing::get("/api/v1/user", Some(&instance.admin.token)))?;
        instance.admin.id = number(&value(&user)?, b"id").ok_or_else(|| Failure::Io("admin id missing".into()))?;
        Ok(instance)
    }
    /// Attaches an owned Forgejo child without taking ownership of its files.
    /// # Errors
    /// Returns a readiness or administrator lookup failure; the child is reaped.
    pub fn attach(child: Child, port: u16, scratch: &Path, admin: Token, settings: &Settings) -> Result<Self, Failure> {
        let secrets = vec![admin.value().to_owned()];
        let mut instance = Self {
            child: Some(child),
            port,
            scratch: scratch.into(),
            work: None,
            admin: Account { login: String::new(), id: 0, token: admin, password: String::new() },
            settings: settings.clone(),
            secrets,
        };
        instance.ready()?;
        let user = instance.exchange(&Outgoing::get("/api/v1/user", Some(&instance.admin.token)))?;
        instance.expect("/api/v1/user", &user, 200)?;
        instance.admin.login =
            text(&value(&user)?, b"login").ok_or_else(|| Failure::Io("admin login missing".into()))?;
        instance.admin.id = number(&value(&user)?, b"id").ok_or_else(|| Failure::Io("admin id missing".into()))?;
        Ok(instance)
    }
    fn ready(&mut self) -> Result<(), Failure> {
        for _ in 0..self.settings.ready_polls {
            if let Some(status) = self.child.as_mut().expect("web started").try_wait()? {
                return Err(Failure::Exited { step: "web", status: status.to_string(), log_tail: self.log_tail() });
            }
            if self.exchange(&Outgoing::get("/api/v1/version", None)).is_ok_and(|x| x.status == 200) {
                return Ok(());
            }
            http::host::wait(skein_lib::Duration::from_millis(50))?;
        }
        Err(Failure::NotReady { polls: self.settings.ready_polls, log_tail: self.log_tail() })
    }
    /// Runs Forgejo's command line on the running instance's work path.
    /// # Errors
    /// Returns a spawn failure.
    pub fn cli(&self, args: &[&str]) -> Result<crate::Output, Failure> {
        self.work.as_ref().ok_or_else(|| Failure::Io("attached instance has no offline work path".into()))?.run(args)
    }
    fn log_tail(&self) -> String {
        let bytes = fs::read(self.scratch.join("server.log")).unwrap_or_default();
        self.mask(&String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(2000)..]))
    }
    fn mask(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for secret in &self.secrets {
            if !secret.is_empty() {
                text = text.replace(secret, "MASKED");
            }
        }
        text
    }
    /// Creates a token through the user's Basic-authenticated token route.
    /// # Errors
    /// Returns a masked provisioning or exchange failure.
    pub fn token(&mut self, login: &str, password: &str) -> Result<Token, Failure> {
        let target = format!("/api/v1/users/{login}/tokens");
        let auth = format!("Basic {}", http::basic(&format!("{login}:{password}")));
        self.secrets.push(auth.clone());
        let response = http::exchange(
            self.port,
            &Outgoing::json("POST", &target, None, b"{\"name\":\"throwaway\",\"scopes\":[\"all\"]}"),
            Some(&auth),
            self.settings.answer_bytes,
        )?;
        self.expect(&target, &response, 201)?;
        let token = text(&value(&response)?, b"sha1").ok_or_else(|| Failure::Io("token missing".into()))?;
        self.secrets.push(token.clone());
        self.secrets.push(http::basic(&format!("{login}:{token}")));
        Ok(Token::new(token))
    }
    fn expect(&self, target: &str, response: &Exchange, status: u16) -> Result<(), Failure> {
        if response.status != status {
            return Err(Failure::Status {
                target: target.into(),
                status: response.status,
                body: self.mask(&String::from_utf8_lossy(&response.body)),
            });
        }
        Ok(())
    }
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
    #[must_use]
    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    #[must_use]
    pub fn scratch(&self) -> &Path {
        &self.scratch
    }
    #[must_use]
    pub fn admin(&self) -> &Account {
        &self.admin
    }
    /// # Errors
    /// Returns the API failure without any credential in its diagnostic.
    pub fn create_user(&mut self, login: &str) -> Result<Account, Failure> {
        let password = crate::cli::random()?;
        self.secrets.push(password.clone());
        let email = format!("{login}@example.invalid");
        let body = encode(|w| {
            w.key(b"username");
            w.string(login.as_bytes());
            w.key(b"password");
            w.string(password.as_bytes());
            w.key(b"email");
            w.string(email.as_bytes());
            w.key(b"must_change_password");
            w.boolean(false);
        })?;
        let response = self.exchange(&Outgoing::json("POST", "/api/v1/admin/users", Some(&self.admin.token), &body))?;
        self.expect("/api/v1/admin/users", &response, 201)?;
        let id = number(&value(&response)?, b"id").ok_or_else(|| Failure::Io("user id missing".into()))?;
        Ok(Account { login: login.into(), id, token: self.token(login, &password)?, password })
    }
    /// # Errors
    /// Returns the repository provisioning failure.
    pub fn create_repository(&mut self, owner: &Account, name: &str, init: Init) -> Result<Made, Failure> {
        let target = "/api/v1/user/repos";
        let body = encode(|w| {
            w.key(b"name");
            w.string(name.as_bytes());
            w.key(b"auto_init");
            w.boolean(init != Init::Empty);
            w.key(b"default_branch");
            w.string(b"main");
            w.key(b"object_format_name");
            w.string(if init == Init::Sha256 { b"sha256" } else { b"sha1" });
        })?;
        let response = self.exchange(&Outgoing::json("POST", target, Some(&owner.token), &body))?;
        self.expect(target, &response, 201)?;
        let id = number(&value(&response)?, b"id").ok_or_else(|| Failure::Io("repository id missing".into()))?;
        Ok(Made { id, owner: owner.login.clone(), name: name.into() })
    }
    /// # Errors
    /// Returns the collaborator provisioning failure.
    pub fn add_collaborator(
        &mut self,
        owner: &Account,
        repository: &str,
        user: &Account,
        permission: &str,
    ) -> Result<(), Failure> {
        let target = format!("/api/v1/repos/{}/{repository}/collaborators/{}", owner.login, user.login);
        let body = encode(|w| {
            w.key(b"permission");
            w.string(permission.as_bytes());
        })?;
        let response = self.exchange(&Outgoing::json("PUT", &target, Some(&owner.token), &body))?;
        self.expect(&target, &response, 204)
    }
    /// # Errors
    /// Returns a connection, framing or bounded-read failure.
    pub fn exchange(&self, outgoing: &Outgoing<'_>) -> Result<Exchange, Failure> {
        let auth = outgoing.token.map(|t| format!("token {}", t.value()));
        http::exchange(self.port, outgoing, auth.as_deref(), self.settings.answer_bytes)
    }
    /// One basic-auth exchange with the credential masked in the recording.
    /// # Errors
    /// Returns a connection, framing or bounded-read failure.
    pub fn exchange_basic(&self, outgoing: &Outgoing<'_>, login: &str, secret: &str) -> Result<Exchange, Failure> {
        let auth = format!("Basic {}", http::basic(&format!("{login}:{secret}")));
        http::exchange(self.port, outgoing, Some(&auth), self.settings.answer_bytes)
    }
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        self.secrets.clone()
    }
    /// # Errors
    /// Returns a child or scratch removal failure after attempting both.
    pub fn stop(mut self) -> Result<(), Failure> {
        self.cleanup()
    }
    fn cleanup(&mut self) -> Result<(), Failure> {
        let child_result = self.child.take().map_or(Ok(()), |mut child| {
            let kill = child.kill();
            let wait = child.wait();
            kill.and(wait.map(|_| ()))
        });
        let remove = if self.work.is_some() { fs::remove_dir_all(&self.scratch) } else { Ok(()) };
        child_result?;
        remove?;
        Ok(())
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _result = self.cleanup();
    }
}
fn value(response: &Exchange) -> Result<skein_json::Document, Failure> {
    use skein_json::collector::{self, Collector, Filter, Keep};
    use skein_lib::stream::{Down, Up};
    use skein_lib::{Env, Intake, Queue, Time, Wall};
    let cap = u32::try_from(response.body.len()).map_err(|_| Failure::Io("provisioning JSON too long".into()))?.max(1);
    let limits = collector::Limits {
        tokenizer: skein_json::tokenizer::Limits { depth: 32, string: cap, number: 32, chunk: 4096, length: cap },
        tokens: cap,
        text: cap,
        skip: u64::from(cap),
    };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut machine = Collector::new(Filter { root: Keep::Value }, &limits, &[]).expect("whole document filter");
    let mut above = Queue::with_capacity(8);
    let mut below = Queue::with_capacity(8);
    let mut input = Intake::with_capacity(cap.max(limits.tokenizer.chunk));
    input.append(&response.body).expect("admitted answer");
    collector::down(&mut machine, &env, collector::Request::Collect, &mut above, &mut below);
    loop {
        if let Some(event) = above.pop() {
            return match event {
                collector::Event::Collected(document) => Ok(document),
                collector::Event::Failed(_) | collector::Event::Closed => {
                    Err(Failure::Io("malformed provisioning JSON".into()))
                }
            };
        }
        let Some(Down::Demand { read, .. }) = below.pop() else {
            return Err(Failure::Io("malformed provisioning JSON".into()));
        };
        let up = input.meet(read).map_or(Up::End, Up::Bytes);
        collector::up(&mut machine, &env, up, &mut above, &mut below);
    }
}
fn field<'a>(document: &'a skein_json::Document, name: &[u8]) -> Option<&'a skein_json::Compact> {
    for index in 0..document.len() {
        let token = document.token(index)?;
        if token.kind == skein_json::Kind::Key && document.text(token)? == name {
            return document.token(index + 1);
        }
    }
    None
}
fn number(document: &skein_json::Document, name: &[u8]) -> Option<u64> {
    let token = field(document, name)?;
    if token.kind != skein_json::Kind::Number {
        return None;
    }
    std::str::from_utf8(document.text(token)?).ok()?.parse().ok()
}
fn text(document: &skein_json::Document, name: &[u8]) -> Option<String> {
    let token = field(document, name)?;
    if token.kind != skein_json::Kind::String {
        return None;
    }
    String::from_utf8(document.text(token)?.to_vec()).ok()
}
fn encode(fields: impl Fn(&mut skein_json::writer::Encoder)) -> Result<Box<[u8]>, Failure> {
    use skein_json::writer::{Encoder, Limits};
    let limits = Limits { depth: 8, length: 16 * 1024 * 1024 };
    let write = |w: &mut Encoder| {
        w.object_start();
        fields(w);
        w.object_end();
    };
    let mut measure = Encoder::measure(&limits);
    write(&mut measure);
    let size = measure.measured().map_err(|_| Failure::Io("provisioning JSON too long".into()))?;
    let mut writer = Encoder::write(size, &limits);
    write(&mut writer);
    Ok(writer.finish())
}
pub(crate) fn ini(scratch: &Path, port: u16, settings: &Settings, user: &str, secret: &str, internal: &str) -> String {
    let path = scratch.display();
    let cap = settings.max_response_items.map_or_else(String::new, |n| format!("[api]\nMAX_RESPONSE_ITEMS = {n}\n"));
    let hooks = settings
        .webhook_hosts
        .as_ref()
        .map_or_else(String::new, |hosts| format!("[webhook]\nALLOWED_HOST_LIST = {hosts}\n"));
    format!(
        "APP_NAME = kit throwaway\nRUN_USER = {user}\nWORK_PATH = {path}\n[server]\nPROTOCOL = http\nHTTP_ADDR = 127.0.0.1\nHTTP_PORT = {port}\nROOT_URL = http://127.0.0.1:{port}/\nAPP_DATA_PATH = {path}/data\nDISABLE_SSH = true\nOFFLINE_MODE = true\n[database]\nDB_TYPE = sqlite3\nPATH = {path}/forgejo.db\n[repository]\nROOT = {path}/repositories\nDEFAULT_BRANCH = main\n[security]\nINSTALL_LOCK = true\nSECRET_KEY = {secret}\nINTERNAL_TOKEN = {internal}\n[service]\nDISABLE_REGISTRATION = true\n[log]\nMODE = console\nLEVEL = Error\n[git]\nPATH = {}\n[actions]\nENABLED = {}\n{cap}{hooks}",
        settings.git.display(),
        settings.actions
    )
}
