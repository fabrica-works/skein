use super::*;
use crate::Outgoing;
use kernel::{Complete, Done, Error, Fd, Op, Submit};

// The complete fixture has a status line, two headers and a four-byte body.
const REPLY: &[u8] = b"HTTP/1.1 423 Locked\r\nCache-Control: private\r\nContent-Length: 4\r\n\r\ntext";
const PORT: u16 = 3000;
const FD: i32 = 7;
// One socket needs fewer than 64 turns to connect, upload and settle.
const TURNS: usize = 64;

struct Driver<'a> {
    host: OneShot<'a>,
    recv: Option<Submit>,
}
impl<'a> Driver<'a> {
    fn new(exchange: &'a mut ExchangeHost) -> Self {
        let mut host = OneShot::new(Some(exchange), Time::ZERO.saturating_add(Duration::from_secs(10)));
        host.requests
            .push(io::Request::Connect { owner: Token::new(0), addr: io::Addr::Ip(([127, 0, 0, 1], PORT).into()) });
        let mut this = Self { host, recv: None };
        this.drain();
        this
    }
    #[expect(clippy::wildcard_enum_match_arm, reason = "the one-shot host submits only socket operations")]
    fn turn(&mut self) {
        self.host.iterate(Time::ZERO, Wall::EPOCH);
        while let Some(Submit { op, kind }) = self.host.submit.pop() {
            let result = match &kind {
                Op::Recv { .. } => {
                    assert!(self.recv.replace(Submit { op, kind }).is_none());
                    continue;
                }
                Op::Socket { .. } => Ok(Done::Fd(Fd::new(FD))),
                Op::Send { bytes, from, .. } => Ok(Done::Count(u32::try_from(bytes.len()).expect("send fits") - from)),
                Op::Connect { .. } | Op::Shutdown { .. } | Op::Close { .. } => Ok(Done::Nothing),
                Op::Cancel { target } => {
                    if let Some(pending) = self.recv.take() {
                        assert_eq!(*target, pending.op);
                        self.host.complete.push(Complete {
                            op: pending.op,
                            kind: pending.kind,
                            result: Err(Error::Cancelled),
                        });
                        Ok(Done::Nothing)
                    } else {
                        // A receive already completed with Reset may still be
                        // queued above io when cancellation is submitted.
                        Err(Error::TooLate)
                    }
                }
                other => panic!("unexpected operation {other:?}"),
            };
            self.host.complete.push(Complete { op, kind, result });
        }
    }
    fn drain(&mut self) {
        for _ in 0..TURNS {
            if !self.host.work_pending(Time::ZERO) {
                return;
            }
            self.turn();
        }
        panic!("driver did not settle");
    }
    fn receive(&mut self, bytes: &[u8]) {
        let Submit { op, mut kind } = self.recv.take().expect("submitted receive");
        if let Op::Recv { buf, .. } = &mut kind {
            buf[..bytes.len()].copy_from_slice(bytes);
        }
        self.host.complete.push(Complete {
            op,
            kind,
            result: Ok(Done::Count(u32::try_from(bytes.len()).expect("reply fits"))),
        });
        self.turn();
    }
    fn reset(&mut self) {
        let Submit { op, kind } = self.recv.take().expect("next submitted receive");
        self.host.complete.push(Complete { op, kind, result: Err(Error::Reset) });
        self.drain();
    }
}
fn exchange() -> ExchangeHost {
    ExchangeHost::new(PORT, &Outgoing::get("/api/v1/user", None), None, REPLY.len())
}
#[test]
fn a_received_reply_finishes_before_a_later_reset() {
    let mut exchange = exchange();
    let mut driver = Driver::new(&mut exchange);
    driver.receive(REPLY);
    driver.reset();
    assert!(exchange.error.is_none(), "{:?}", exchange.error);
    assert_eq!(exchange.exchange.status, 423);
    assert_eq!(exchange.exchange.body, b"text");
}
#[test]
fn an_incomplete_reply_still_fails_on_reset() {
    let mut exchange = exchange();
    let mut driver = Driver::new(&mut exchange);
    let (_, incomplete) = REPLY.split_last().expect("nonempty reply");
    driver.receive(incomplete);
    driver.reset();
    assert_eq!(format!("{:?}", exchange.error), "Some(Io(\"malformed HTTP answer: Stream(Reset)\"))");
}
#[test]
fn terminal_close_cancels_and_drains_the_receive() {
    let mut exchange = exchange();
    let mut driver = Driver::new(&mut exchange);
    driver.receive(REPLY);
    driver.drain();
    assert!(driver.host.is_empty());
    assert!(driver.recv.is_none());
}
// wait() supplies a policy deadline without any socket or network retry.
const WAIT: Duration = Duration::from_millis(1);
#[test]
fn wait_remains_pending_before_its_deadline() {
    let mut host = OneShot::new(None, Time::ZERO.saturating_add(WAIT));
    host.iterate(Time::ZERO, Wall::EPOCH);
    assert!(!host.is_empty());
    assert_eq!(host.next_deadline(), Some(Time::ZERO.saturating_add(WAIT)));
}
#[test]
fn wait_finishes_at_its_deadline() {
    let deadline = Time::ZERO.saturating_add(WAIT);
    let mut host = OneShot::new(None, deadline);
    host.iterate(deadline, Wall::EPOCH);
    assert!(host.is_empty());
    assert_eq!(host.next_deadline(), None);
}
