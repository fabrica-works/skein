//! The owner-only live boundary. Tests exercise refusals without constructing a connector.
use crate::{
    Result,
    wire::{Request, Response},
};
use skein_forge::github::request::Method;
use skein_forge_conformance::{
    json::Json,
    write::{SecretKind, Secrets},
};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(crate) struct Fixtures {
    pub app: u64,
    pub slug: String,
    pub version: String,
    pub key: Vec<u8>,
    pub webhook: String,
    pub people: BTreeMap<String, String>,
    pub installations: Vec<Installation>,
    pub spare: String,
    pub source: Option<PathBuf>,
    pub pem: String,
}
#[derive(Clone)]
pub(crate) struct Installation {
    pub id: u64,
    pub owner: String,
    pub repositories: Vec<String>,
}
fn safe_name(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
fn safe_repository(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn file(dir: &Path, name: &str, bound: u64) -> Result<Vec<u8>> {
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.file_type().is_file() || meta.permissions().mode() & 0o7777 != 0o600 {
        return Err(format!("fixture {name} must be a regular 0600 file").into());
    }
    if meta.len() > bound {
        return Err(format!("fixture {name} exceeds its size bound").into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(bound + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())? > bound {
        return Err("fixture changed beyond its size bound".into());
    }
    Ok(bytes)
}
impl Fixtures {
    pub fn load(dir: &Path) -> Result<Self> {
        let meta = fs::symlink_metadata(dir)?;
        if !meta.file_type().is_dir() || meta.permissions().mode() & 0o7777 != 0o700 {
            return Err("live fixtures directory must be a regular 0700 directory".into());
        }
        let data = file(dir, "app.json", 65536)?;
        let json = Json::parse(&data)?;
        let root = json.root();
        let app = root.get("id").u64().ok_or("app id absent")?;
        let slug = root.get("slug").str().filter(|v| safe_name(v)).ok_or("invalid App slug")?.to_owned();
        let version = root.get("api_version").str().ok_or("API version absent")?.to_owned();
        if version != skein_forge::github::pin::API_VERSION {
            return Err("fixture API version differs from the dialect pin".into());
        }
        for field in ["name", "client_id", "node_id", "html_url", "created_at", "hook_url"] {
            if root.get(field).str().is_none() {
                return Err("App metadata is incomplete".into());
            }
        }
        if root.get("owner").kind().is_none()
            || root.get("permissions").kind() != Some(skein_json::Kind::ObjectStart)
            || root.get("events").array().is_none()
        {
            return Err("App metadata kinds are invalid".into());
        }
        let spare = root.get("spare_repository").str().ok_or("spare repository absent")?.to_owned();
        let mut installations = vec![];
        let mut people = BTreeMap::new();
        for entry in root.get("installations").array().ok_or("installation array absent")? {
            let owner =
                entry.get("account").str().filter(|v| safe_name(v)).ok_or("invalid installation account")?.to_owned();
            let id = entry.get("id").u64().ok_or("installation id absent")?;
            let mut repositories = vec![];
            for repo in entry.get("repositories").array().ok_or("repository array absent")? {
                let repo = repo.str().ok_or("invalid repository")?;
                let (o, n) = repo.split_once('/').ok_or("invalid full repository name")?;
                if o != owner || !safe_repository(n) {
                    return Err("repository must belong to its installation account".into());
                }
                repositories.push(repo.to_owned());
            }
            if repositories.is_empty()
                || installations.iter().any(|row: &Installation| row.owner == owner || row.id == id)
            {
                return Err("empty or duplicate installation".into());
            }
            let token = file(dir, &format!("person-{owner}.token"), 4096)?;
            let token =
                std::str::from_utf8(&token).map_err(|_| "person token is not text")?.trim_end_matches('\n').to_owned();
            if token.is_empty() || token.bytes().any(|b| b.is_ascii_whitespace()) {
                return Err("invalid person token bytes".into());
            }
            people.insert(owner.clone(), token);
            installations.push(Installation { id, owner, repositories });
        }
        if installations.len() < 2 {
            return Err("two installation accounts are required".into());
        }
        let (owner, name) = spare.split_once('/').ok_or("invalid spare repository")?;
        if !safe_name(owner)
            || !safe_repository(name)
            || !people.contains_key(owner)
            || installations.iter().any(|r| r.repositories.contains(&spare))
        {
            return Err("spare must be ungranted in a configured account".into());
        }
        let pem = file(dir, "app.pem", 16384)?;
        let key = pem_key(&pem)?;
        let pem = String::from_utf8(pem).map_err(|_| "App PEM is not text")?;
        let webhook = file(dir, "webhook_secret", 4096)?;
        let webhook =
            std::str::from_utf8(&webhook).map_err(|_| "webhook secret is not text")?.trim_end_matches('\n').to_owned();
        if webhook.is_empty() {
            return Err("empty webhook secret".into());
        }
        Ok(Self { app, slug, version, key, webhook, people, installations, spare, source: Some(dir.to_owned()), pem })
    }
    pub fn secrets(&self) -> Secrets {
        let mut secrets = Secrets::of(&[]);
        register(&mut secrets, SecretKind::HookSecret, &self.webhook);
        for token in self.people.values() {
            register(&mut secrets, SecretKind::PersonToken, token);
        }
        register(&mut secrets, SecretKind::AppKey, &self.pem);
        for line in self.pem.lines().filter(|line| line.len() > 32 && !line.starts_with("-----")) {
            secrets.add(SecretKind::AppKey, line);
        }
        secrets
    }
}
pub(crate) fn register(secrets: &mut Secrets, kind: SecretKind, value: &str) {
    secrets.add(kind, value);
    // JSON escaping must not disguise an unexpected secret field from the byte scanner.
    if let Ok(encoded) = skein_forge_conformance::json::object(&[("value", &value)])
        && let Some(escaped) = encoded.strip_prefix(b"{\"value\":\"").and_then(|v| v.strip_suffix(b"\"}"))
        && let Ok(escaped) = std::str::from_utf8(escaped)
    {
        secrets.add(kind, escaped);
    }
}
fn pem_key(pem: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(pem).map_err(|_| "invalid App PEM")?;
    let mut data = String::new();
    let mut inside = false;
    let mut ended = false;
    for line in text.lines() {
        if line == "-----BEGIN RSA PRIVATE KEY-----" {
            if inside {
                return Err("invalid App PEM".into());
            }
            inside = true;
            continue;
        }
        if line == "-----END RSA PRIVATE KEY-----" {
            ended = true;
            break;
        }
        if inside {
            data.push_str(line.trim());
        }
    }
    if !inside || !ended {
        return Err("App PEM must contain one RSA private key".into());
    }
    let mut out = Vec::new();
    let mut bits = 0_u32;
    let mut count = 0;
    for byte in data.bytes() {
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("invalid App PEM encoding".into()),
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 255)?);
        }
    }
    if skein_tls::sign_rs256(&out, b"fixture validation").is_none() {
        return Err("App key cannot sign RS256".into());
    }
    Ok(out)
}
/// # Errors
/// Rejects every App mutation before invoking the supplied connector.
pub fn guard(method: Method, target: &str) -> Result<()> {
    let path = target.split('?').next().ok_or("invalid target")?;
    let mint = method == Method::Post
        && path
            .strip_prefix("/app/installations/")
            .and_then(|tail| tail.strip_suffix("/access_tokens"))
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()));
    let read = method == Method::Get && (path == "/app" || path.starts_with("/app/"));
    if mint || read {
        return Ok(());
    }
    if path.starts_with("/app")
        || path.starts_with("/settings/")
        || path.contains("/settings/apps")
        || !(path.starts_with("/repos/")
            || path.starts_with("/repositories/")
            || path.starts_with("/users/")
            || path.starts_with("/user")
            || path == "/graphql"
            || path == "/installation/token"
            || path.starts_with("/installation/repositories"))
    {
        return Err("live target refuses App changes before the connector".into());
    }
    Ok(())
}
/// # Errors
/// Enforces the live boundary before a connector can write any byte.
pub fn guarded<T>(method: Method, target: &str, connector: impl FnOnce() -> Result<T>) -> Result<T> {
    guard(method, target)?;
    connector()
}

pub(crate) struct Connector {
    tls: skein_tls::Config,
}
impl Connector {
    pub fn new() -> Result<Self> {
        let roots = skein_shell::read_trust_roots(
            &skein_shell::Trust::Machine { ssl_cert_file: None },
            &skein_shell::RootBounds { bundle_bytes: 4_194_304, certificates: 1024, certificate_bytes: 65536 },
        )
        .map_err(|_| "cannot read machine trust roots")?;
        let (tls, _) =
            skein_tls::Config::from_der(&roots.certificates, &[b"http/1.1"]).map_err(|_| "cannot configure TLS")?;
        Ok(Self { tls })
    }
    #[expect(clippy::too_many_lines, reason = "one TLS step loop owns the socket and both stream directions")]
    pub fn exchange(&self, request: &Request) -> Result<(Vec<u8>, Response)> {
        use skein_lib::{
            Env, Intake, Queue, Time,
            stream::{Down, Read as Demand, Up},
        };
        use skein_tls::client::{self, Event, Request as TlsRequest};
        use std::{
            io::{Read, Write},
            net::{TcpStream, ToSocketAddrs},
        };
        let (host, target) = if request.blob {
            let tail = request.target.strip_prefix("https://").ok_or("live blob redirect must use HTTPS")?;
            let (host, tail) = tail.split_once('/').ok_or("invalid blob redirect")?;
            if host.contains(['@', ':', '%']) {
                return Err("invalid blob host".into());
            }
            (host.to_owned(), format!("/{tail}"))
        } else {
            guard(request.method, &request.target)?;
            ("api.github.com".into(), request.target.clone())
        };
        let mut outgoing = request.clone();
        outgoing.target = target;
        if request.blob {
            outgoing.authorization = None;
        }
        let wire = outgoing.wire(&host)?;
        let addr = (host.as_str(), 443).to_socket_addrs()?.next().ok_or("live host has no address")?;
        let mut socket = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(30))?;
        socket.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
        socket.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
        let limits = client::Limits { read: 16384, send: 16384, records: 131_072 };
        let env = Env { now: Time::ZERO, wall: skein_shell::Clock::new().now().wall, limits };
        let name = skein_tls::Name::new(&host).ok_or("invalid TLS server name")?;
        let mut tls = client::Client::new(&self.tls, name, &limits);
        let mut above = Queue::with_capacity(16);
        let mut below = Queue::with_capacity(16);
        let mut intake = Intake::with_capacity(131_072);
        client::down(&mut tls, &env, TlsRequest::Handshake, &mut above, &mut below);
        let mut sent = 0;
        let mut pending = 0;
        let mut received = Vec::new();
        for _ in 0..100_000 {
            if let Some(event) = above.pop() {
                match event {
                    Event::Ready(_) => {
                        pending = wire.len().min(16384);
                        client::down(
                            &mut tls,
                            &env,
                            TlsRequest::Stream(Down::Demand { read: Demand::Nothing, room: u32::try_from(pending)? }),
                            &mut above,
                            &mut below,
                        );
                    }
                    Event::Stream(Up::Room) => {
                        let end = sent + pending;
                        client::down(
                            &mut tls,
                            &env,
                            TlsRequest::Stream(Down::Send(wire[sent..end].into())),
                            &mut above,
                            &mut below,
                        );
                        sent = end;
                        pending = (wire.len() - sent).min(16384);
                        let demand = if pending == 0 {
                            Down::Demand { read: Demand::Arrived { max: 16384 }, room: 0 }
                        } else {
                            Down::Demand { read: Demand::Nothing, room: u32::try_from(pending)? }
                        };
                        client::down(&mut tls, &env, TlsRequest::Stream(demand), &mut above, &mut below);
                    }
                    Event::Stream(Up::Bytes(bytes)) => {
                        received.extend_from_slice(&bytes);
                        if received.len() > 4_194_304 {
                            return Err("live answer exceeds its bound".into());
                        }
                        if let Some(response) = crate::wire::parse(&received)? {
                            return Ok((wire, response));
                        }
                        client::down(
                            &mut tls,
                            &env,
                            TlsRequest::Stream(Down::Demand { read: Demand::Arrived { max: 16384 }, room: 0 }),
                            &mut above,
                            &mut below,
                        );
                    }
                    Event::Stream(Up::End | Up::Failed(_)) | Event::Failed(_) | Event::Closed => {
                        return Err("live TLS exchange ended before its answer".into());
                    }
                }
                continue;
            }
            let effect = below.pop().ok_or("TLS made no progress")?;
            let up = match effect {
                Down::Send(bytes) => {
                    socket.write_all(&bytes)?;
                    continue;
                }
                Down::Finish => return Err("TLS finished before its answer".into()),
                Down::Demand { room, .. } if room > 0 => Up::Room,
                Down::Demand { read, .. } => loop {
                    if let Some(bytes) = intake.meet(read) {
                        break Up::Bytes(bytes);
                    }
                    let mut bytes = [0; 16384];
                    let count = socket.read(&mut bytes)?;
                    if count == 0 {
                        break Up::End;
                    }
                    intake.append(&bytes[..count]).map_err(|_| "TLS intake bound")?;
                },
            };
            client::up(&mut tls, &env, up, &mut above, &mut below);
        }
        Err("live exchange exceeded its progress bound".into())
    }
}
