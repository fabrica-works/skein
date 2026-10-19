use crate::{Exchange, Failure, Outgoing, Range};
use skein_http::{
    Header, Method,
    client::{self, Body, Call, Client},
};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Env, Queue, Time, Wall};
pub(crate) mod host;

const PIECE: u32 = 16_384;
fn limits() -> client::Limits {
    client::Limits { request: 65_536, head: 65_536, headers: 256, read: PIECE, send: PIECE }
}
fn call(port: u16, outgoing: &Outgoing<'_>, auth: Option<&str>) -> Call {
    let mut headers =
        vec![Header { name: b"Host".as_slice().into(), value: format!("127.0.0.1:{port}").into_bytes().into() }];
    if let Some(auth) = auth {
        headers.push(Header { name: b"Authorization".as_slice().into(), value: auth.as_bytes().into() });
    }
    if outgoing.body.is_some() {
        headers.push(Header { name: b"Content-Type".as_slice().into(), value: b"application/json".as_slice().into() });
    }
    let range = match outgoing.range {
        Some(Range::Head(end)) => Some(format!("bytes=0-{end}")),
        Some(Range::Tail(length)) => Some(format!("bytes=-{length}")),
        None => None,
    };
    if let Some(range) = range {
        headers.push(Header { name: b"Range".as_slice().into(), value: range.into_bytes().into() });
    }
    let method = Method::from_bytes(outgoing.method.as_bytes()).expect("supported HTTP method");
    Call {
        method,
        target: outgoing.target.as_bytes().into(),
        headers: headers.into(),
        body: outgoing.body.map_or(Body::None, |body| Body::Length(u64::try_from(body.len()).expect("body fits"))),
        close: true,
    }
}

/// One decoder for socket exchanges and the framing tests. Raw bytes are kept
/// only through the head's final CRLF; skein-http supplies status and body.
pub(super) struct ExchangeHost {
    client: Client,
    above: Queue<client::Event>,
    pub below: Queue<Down>,
    payload: Vec<u8>,
    uploaded: usize,
    auth: Option<String>,
    pub exchange: Exchange,
    raw: usize,
    cap: usize,
    head_done: bool,
    pub done: bool,
    pub error: Option<Failure>,
}
impl ExchangeHost {
    fn new(port: u16, outgoing: &Outgoing<'_>, auth: Option<&str>, cap: usize) -> Self {
        let mut this = Self {
            client: Client::new(&limits()),
            above: Queue::with_capacity(64),
            below: Queue::with_capacity(64),
            payload: outgoing.body.unwrap_or_default().to_vec(),
            uploaded: 0,
            auth: auth.map(str::to_owned),
            exchange: Exchange { sent: Vec::new(), status: 0, head: Vec::new(), body: Vec::new() },
            raw: 0,
            cap,
            head_done: false,
            done: false,
            error: None,
        };
        this.down(client::Request::Call(call(port, outgoing, auth)));
        if outgoing.body.is_some() {
            this.upload();
        }
        this.pump();
        this
    }
    fn env() -> Env<client::Limits> {
        Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits() }
    }
    fn down(&mut self, request: client::Request) {
        client::down(&mut self.client, &Self::env(), request, &mut self.above, &mut self.below);
    }
    fn upload(&mut self) {
        if self.uploaded == self.payload.len() {
            self.down(client::Request::Upload(Down::Finish));
        } else {
            let room = u32::try_from(self.payload.len() - self.uploaded).unwrap_or(u32::MAX).min(PIECE);
            self.down(client::Request::Upload(Down::Demand { read: Read::Nothing, room }));
        }
    }
    fn demand(&mut self) {
        self.down(client::Request::Body(Down::Demand { read: Read::Arrived { max: PIECE }, room: 0 }));
    }
    pub fn up(&mut self, up: Up) {
        if self.done {
            return;
        }
        if let Up::Bytes(bytes) = &up {
            if bytes.len() > self.cap.saturating_sub(self.raw) {
                self.fail("answer exceeds answer_bytes".into());
                return;
            }
            self.raw += bytes.len();
            if !self.head_done {
                for &byte in bytes {
                    self.exchange.head.push(byte);
                    if self.exchange.head.ends_with(b"\r\n\r\n") {
                        self.exchange.head.truncate(self.exchange.head.len() - 2);
                        self.head_done = true;
                        break;
                    }
                }
            }
        }
        client::up(&mut self.client, &Self::env(), up, &mut self.above, &mut self.below);
        self.pump();
    }
    fn pump(&mut self) {
        while let Some(event) = self.above.pop() {
            match event {
                client::Event::Response(response) => {
                    self.exchange.status = response.status;
                    self.demand();
                }
                client::Event::Body(Up::Bytes(bytes)) => {
                    self.exchange.body.extend_from_slice(&bytes);
                    self.demand();
                }
                client::Event::Upload(Up::Room) => {
                    let end = self.payload.len().min(self.uploaded + usize::try_from(PIECE).expect("piece fits"));
                    let bytes = self.payload[self.uploaded..end].into();
                    self.uploaded = end;
                    self.down(client::Request::Upload(Down::Send(bytes)));
                    self.upload();
                }
                client::Event::Done(_) => self.done = true,
                client::Event::Failed(error) => self.fail(format!("malformed HTTP answer: {error:?}")),
                client::Event::Body(Up::Failed(_)) => self.fail("malformed HTTP answer".into()),
                client::Event::Body(Up::End) | client::Event::Upload(Up::Failed(_)) | client::Event::Closed => {}
                client::Event::Body(Up::Room) | client::Event::Upload(Up::Bytes(_) | Up::End) => {
                    unreachable!("HTTP stream direction")
                }
            }
        }
    }
    pub fn fail(&mut self, error: String) {
        self.error = Some(Failure::Io(error));
        self.done = true;
    }
    pub fn capture(&mut self, down: &Down) {
        if let Down::Send(bytes) = down {
            // The HTTP head is one send. Uploads remain byte-for-byte identical.
            if self.exchange.sent.is_empty() {
                let text = std::str::from_utf8(bytes).expect("HTTP request head");
                let text = match &self.auth {
                    Some(auth) => text.replace(
                        &format!("Authorization: {auth}\r\n"),
                        if auth.starts_with("Basic ") {
                            "Authorization: Basic MASKED\r\n"
                        } else {
                            "Authorization: token MASKED\r\n"
                        },
                    ),
                    None => text.to_owned(),
                };
                self.exchange.sent.extend_from_slice(text.as_bytes());
            } else {
                self.exchange.sent.extend_from_slice(bytes);
            }
        }
    }
    fn result(self) -> Result<Exchange, Failure> {
        self.error.map_or(Ok(self.exchange), Err)
    }
}

#[cfg(test)]
pub(crate) fn request(port: u16, outgoing: &Outgoing<'_>, auth: Option<&str>) -> Vec<u8> {
    let mut exchange = ExchangeHost::new(port, outgoing, auth, usize::MAX);
    while let Some(down) = exchange.below.pop() {
        exchange.capture(&down);
        if let Down::Demand { room, .. } = down
            && room > 0
        {
            exchange.up(Up::Room);
        }
    }
    exchange.exchange.sent
}
pub(crate) fn exchange(
    port: u16,
    outgoing: &Outgoing<'_>,
    auth: Option<&str>,
    cap: usize,
) -> Result<Exchange, Failure> {
    let mut exchange = ExchangeHost::new(port, outgoing, auth, cap);
    host::exchange(port, &mut exchange)?;
    exchange.result()
}
#[cfg(test)]
pub(crate) fn parse(bytes: &[u8], sent: Vec<u8>) -> Result<Exchange, Failure> {
    let mut exchange = ExchangeHost::new(3000, &Outgoing::get("/api/v1/user", None), None, bytes.len());
    let mut input = skein_lib::Intake::with_capacity(
        u32::try_from(bytes.len()).expect("test answer fits").max(client::largest_read(&limits())),
    );
    input.append(bytes).expect("test input");
    while let Some(down) = exchange.below.pop() {
        match down {
            Down::Demand { read, room } => {
                if room > 0 {
                    exchange.up(Up::Room);
                } else if read != Read::Nothing {
                    exchange.up(input.meet(read).map_or(Up::End, Up::Bytes));
                }
            }
            Down::Send(_) | Down::Finish => {}
        }
    }
    exchange.exchange.sent = sent;
    exchange.result()
}
pub fn basic(value: &str) -> String {
    use skein_lib::{
        Writer,
        base64::{self, Alphabet},
    };
    let length = base64::encoded_len(Alphabet::Standard, u32::try_from(value.len()).expect("credential fits"))
        .expect("base64 size");
    let mut writer = Writer::new(usize::try_from(length).expect("size fits"));
    base64::encode(Alphabet::Standard, value.as_bytes(), &mut writer).expect("sized base64");
    String::from_utf8(writer.finish().into_vec()).expect("ASCII base64")
}
