//! Native TLS over io and against a peer in memory (tls.md, 3.6).

use rustls::ContentType;
use skein_lib::{
    Token,
    stream::{OutputDown, OutputOutcome, OutputUp, Read, Up},
};
use skein_sim::Config;
use skein_tls::client::{self, Version, native};
use skein_tls_world::{
    native::{Case, conversation},
    native_pair::{Pair, UPPER, granted},
    pki,
    server::Server,
};

#[test]
fn late_plaintext_sends_while_both_reads_wait() {
    for version in [Version::Tls12, Version::Tls13] {
        let observed = conversation(801, Config { buffer: 7, ..Config::calm() }, Case::Late, version);
        assert_eq!(observed.server_plaintext, b"ping");
        assert_eq!(observed.reply, b"ok");
        assert!(observed.behind_both_reads);
        assert_eq!(observed.terminals.len(), 1);
        assert_eq!(observed.terminals[0].1, OutputOutcome::Granted);
        assert_eq!((observed.closed, observed.transport_closing, observed.transport_closed), (1, 0, 0));
    }
}

#[test]
fn one_byte_short_waits_for_ciphertext_credit() {
    for version in [Version::Tls12, Version::Tls13] {
        let result = conversation(802, Config { buffer: 7, ..Config::calm() }, Case::ByteOneShort, version);
        assert_eq!(result.server_plaintext, b"ping");
        assert_eq!(result.reply, b"ok");
        assert!(result.behind_both_reads);
        assert!(result.blocked);
        assert_eq!(result.terminals.len(), 2);
        assert!(result.terminals.iter().all(|(_, terminal)| *terminal == OutputOutcome::Granted));
        assert_eq!((result.closed, result.transport_closing, result.transport_closed), (1, 0, 0));
    }
}

#[test]
fn exact_ciphertext_credit_admits_the_output() {
    for version in [Version::Tls12, Version::Tls13] {
        let result = conversation(802, Config { buffer: 7, ..Config::calm() }, Case::ByteExact, version);
        assert_eq!(result.server_plaintext, b"ping");
        assert_eq!(result.reply, b"ok");
        assert!(result.behind_both_reads);
        assert!(!result.blocked);
        assert_eq!(result.terminals.len(), 2);
        assert!(result.terminals.iter().all(|(_, terminal)| *terminal == OutputOutcome::Granted));
        assert_eq!((result.closed, result.transport_closing, result.transport_closed), (1, 0, 0));
    }
}

#[test]
fn full_send_slots_wait_despite_available_byte_credit() {
    for version in [Version::Tls12, Version::Tls13] {
        let observed = conversation(803, Config { buffer: 7, ..Config::calm() }, Case::SlotsFull, version);
        assert_eq!(observed.server_plaintext, b"ping");
        assert_eq!(observed.reply, b"ok");
        assert!(observed.behind_both_reads && observed.blocked);
        assert_eq!(observed.terminals.len(), 3);
        assert!(observed.terminals.iter().all(|(_, terminal)| *terminal == OutputOutcome::Granted));
        assert_eq!((observed.closed, observed.transport_closing, observed.transport_closed), (1, 0, 0));
    }
}

#[test]
fn io_closed_before_ready_cancels_the_output_above() {
    for version in [Version::Tls12, Version::Tls13] {
        let observed = conversation(804, Config { buffer: 7, ..Config::calm() }, Case::DirectClosed, version);
        assert!(observed.server_plaintext.is_empty() && observed.reply.is_empty());
        assert_eq!(observed.terminals.len(), 1);
        assert_eq!(observed.terminals[0].1, OutputOutcome::Cancelled);
        assert_eq!((observed.closed, observed.transport_closing, observed.transport_closed), (0, 0, 1));
    }
}

#[test]
fn local_close_answers_once_after_io_cancellation() {
    for version in [Version::Tls12, Version::Tls13] {
        let result = conversation(805, Config { buffer: 7, ..Config::calm() }, Case::LocalCloseFirst, version);
        assert!(result.reply.is_empty());
        assert!(result.blocked && result.behind_both_reads);
        assert_eq!(result.terminals.len(), 2);
        assert_eq!(result.terminals[0].1, OutputOutcome::Granted);
        assert_eq!(result.terminals[1].1, OutputOutcome::Cancelled);
        assert_eq!(result.transport_closing, 1);
        assert_eq!((result.closed, result.transport_closed), (1, 0));
    }
}

#[test]
fn physical_close_answers_once_after_io_cancellation() {
    for version in [Version::Tls12, Version::Tls13] {
        let result = conversation(805, Config { buffer: 7, ..Config::calm() }, Case::PhysicalCloseFirst, version);
        assert!(result.reply.is_empty());
        assert!(result.blocked && result.behind_both_reads);
        assert_eq!(result.terminals.len(), 2);
        assert_eq!(result.terminals[0].1, OutputOutcome::Granted);
        assert_eq!(result.terminals[1].1, OutputOutcome::Cancelled);
        assert_eq!(result.transport_closing, 1);
        assert_eq!((result.closed, result.transport_closed), (0, 1));
    }
}

#[test]
fn queued_grant_releases_after_cancellation_before_the_next_right() {
    let mut pair = Pair::ready(&pki::Server::plain());
    let first = Token::new(50);
    let next = Token::new(51);
    assert!(pair.down(native::Request::Output(OutputDown::Room { right: first, bytes: 16 })).is_empty());
    let queued = pair.answer().expect("observed emitted lower winner");
    assert!(matches!(queued, native::LowerEvent::Output(OutputUp::Settled { outcome: OutputOutcome::Granted, .. })));
    assert_eq!(
        pair.down(native::Request::Output(OutputDown::Cancel { right: first })),
        [native::Event::Output(OutputUp::Settled { right: first, outcome: OutputOutcome::Cancelled })]
    );
    assert!(pair.down(native::Request::Output(OutputDown::Room { right: next, bytes: 16 })).is_empty());
    assert!(pair.up(queued).is_empty(), "queued lower grant releases; no second upper terminal");
    assert_eq!(
        pair.settle(),
        [native::Event::Output(OutputUp::Settled { right: next, outcome: OutputOutcome::Granted })]
    );
    assert!(
        pair.down(native::Request::Output(OutputDown::Cancel { right: next })).is_empty(),
        "emitted upper grant wins"
    );
    assert!(
        pair.down(native::Request::Output(OutputDown::Send { right: next, bytes: b"ping".as_slice().into() }))
            .is_empty()
    );
    assert_eq!(pair.peer.rustls().received, b"ping");
}

#[test]
fn finish_spends_a_held_grant_and_sends_close_notify() {
    let mut pair = Pair::ready(&pki::Server::plain());
    assert_eq!(pair.room(UPPER, 16), [granted(UPPER)]);
    assert!(pair.finish().is_empty());
    assert!(pair.settle().is_empty());
    assert!(pair.peer_closed());
    assert_eq!(pair.finished, 1);
}

#[test]
#[should_panic(expected = "no native read after Finish withdrawal")]
fn finish_forbids_later_reads() {
    let mut pair = Pair::ready(&pki::Server::plain());
    pair.finish();
    pair.settle();
    pair.read(1);
}

#[test]
#[should_panic(expected = "native Finish with no live upper read")]
fn finish_refuses_a_live_read_above() {
    let mut pair = Pair::ready(&pki::Server::plain());
    assert!(pair.read(1).is_empty());
    pair.finish();
}

#[test]
fn key_updates_precede_plaintext_under_the_grant_held() {
    let mut pair = Pair::ready(&pki::Server::plain());
    let right = Token::new(70);
    assert!(pair.read(16).is_empty());
    assert_eq!(pair.room(right, 16), [granted(right)]);
    for _ in 0..3 {
        pair.peer.rustls_mut().key_update();
        pair.pull();
        assert!(
            pair.settle().is_empty(),
            "control records never satisfy the held plaintext read or consume upper grant"
        );
    }
    let payload = vec![b'x'; 16];
    assert!(pair.down(native::Request::Output(OutputDown::Send { right, bytes: payload.clone().into() })).is_empty());
    assert_eq!(
        pair.peer.rustls().received,
        payload,
        "every observed owed key-update response precedes authenticated data"
    );
    assert_eq!(
        pair.last_send().expect("observed deferred reply plus maximum16 Send").1,
        27 + 16 + 22,
        "pinned rustls coalesces the three valid requests to one authenticated deferred reply"
    );
}

#[test]
fn empty_send_and_release_each_spend_the_grant() {
    let mut pair = Pair::ready(&pki::Server::plain());
    for (right, empty) in [(Token::new(80), true), (Token::new(81), false)] {
        assert_eq!(pair.room(right, 16), [granted(right)]);
        let spent =
            if empty { OutputDown::Send { right, bytes: Box::from([]) } } else { OutputDown::Release { right } };
        assert!(pair.down(native::Request::Output(spent)).is_empty());
        assert!(pair.settle().is_empty());
        assert!(pair.peer.rustls().received.is_empty(), "no invented application bytes from empty/release");
    }
    let right = Token::new(82);
    assert!(pair.down(native::Request::Output(OutputDown::Room { right, bytes: 4 })).is_empty());
    assert_eq!(pair.settle(), [native::Event::Output(OutputUp::Settled { right, outcome: OutputOutcome::Granted })]);
    assert!(pair.down(native::Request::Output(OutputDown::Send { right, bytes: Box::from(*b"ping") })).is_empty());
    assert_eq!(pair.peer.rustls().received, b"ping", "both spent real slots admit the next observed Send");
}

#[test]
#[should_panic(expected = "native Finish with no pending output")]
fn finish_refuses_pending_output_above() {
    let mut pair = Pair::ready(&pki::Server::plain());
    assert!(pair.down(native::Request::Output(OutputDown::Room { right: Token::new(90), bytes: 16 })).is_empty());
    pair.finish();
}

#[test]
fn owed_output_drains_in_order_across_an_older_grant() {
    let mut pair = Pair::ready_tls12();
    assert!(pair.read(16).is_empty());
    pair.peer.extracted_mut().hello_request();
    pair.pull();
    for _ in 0..2 {
        let observed = pair.answer().expect("observed HelloRequest header/body");
        assert!(matches!(observed, native::LowerEvent::Stream(_)));
        assert!(pair.up(observed).is_empty());
    }
    let original = pair.output_bytes().expect("observed TLS-owned refusal reservation");
    assert_eq!(original, 31, "two-byte authenticated warning plus TLS1.2 record overhead");
    let winner = pair.answer().expect("observed older full output grant emitted by proxy");
    let native::LowerEvent::Output(OutputUp::Settled { right: first, outcome: OutputOutcome::Granted }) = &winner
    else {
        panic!("observed queued winner")
    };
    let first = *first;
    assert!(pair.close().is_empty());
    assert!(pair.up(winner).is_empty());
    assert_eq!(pair.last_send(), Some((first, original)), "fitting ordered prefix spends original full grant");
    assert_eq!(pair.peer.extracted().alerts, [[1, 100]], "authenticated warning refusal first");
    assert_eq!(pair.output_bytes(), Some(original), "exact retained close suffix needs a distinct right");
    assert!(pair.settle().is_empty(), "draining ciphertext alone cannot impersonate physical Closed");
    let (second, suffix) = pair.last_send().expect("observed close suffix Send");
    assert_ne!(second, first, "checked distinct lower generation");
    assert_eq!(suffix, original, "whole ordered suffix, no lost control bytes");
    let peer = pair.peer.extracted();
    assert_eq!(peer.alerts, [[1, 100], [1, 0]], "observed warning then observed close_notify authenticate in order");
    assert_eq!(peer.records, [ContentType::Alert, ContentType::Alert]);
    assert!(peer.received.is_empty(), "no invented data from two observed control records");
    assert_eq!(
        pair.up(native::LowerEvent::Closed),
        [native::Event::Client(client::Event::Closed)],
        "bounded proxy lifetime terminates after both settled Sends; observed IO closure tested separately"
    );
}

#[test]
fn tls12_refusal_precedes_plaintext_under_the_grant_held() {
    let mut pair = Pair::ready_tls12();
    let upper = Token::new(100);
    assert!(pair.read(16).is_empty());
    assert!(pair.down(native::Request::Output(OutputDown::Room { right: upper, bytes: 16 })).is_empty());
    assert_eq!(
        pair.settle(),
        [native::Event::Output(OutputUp::Settled { right: upper, outcome: OutputOutcome::Granted })]
    );
    let before = pair.last_send();
    pair.peer.extracted_mut().hello_request();
    pair.pull();
    assert!(pair.settle().is_empty(), "refusal neither answers the read nor consumes backed upper grant");
    assert_eq!(pair.last_send(), before, "owed warning stays held behind the observed upper grant");
    assert!(
        pair.down(native::Request::Output(OutputDown::Send { right: upper, bytes: Box::from([b'x'; 16]) })).is_empty()
    );
    assert_eq!(pair.last_send().expect("observed complete owed+data Send").1, 31 + 16 + 29);
    let peer = pair.peer.extracted();
    assert_eq!(peer.alerts, [[1, 100]]);
    assert_eq!(peer.records, [ContentType::Alert, ContentType::ApplicationData], "complete prefix precedes data");
    assert_eq!(peer.received, [b'x'; 16], "full plaintext authenticates after observed owed warning");
}

#[test]
fn withdrawn_header_keeps_pending_output_usable() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        let right = Token::new(110);
        assert!(pair.read(16).is_empty());
        assert!(pair.down(native::Request::Output(OutputDown::Room { right, bytes: 16 })).is_empty());

        let winner = pair.queued_ciphertext(false);
        let before = pair.last_send();
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty(), "withdrawn winner creates no observation above");
        assert!(pair.read.is_none(), "no ciphertext Fill after withdrawal");
        assert_eq!(pair.last_send(), before, "dropped read spends no output");
        assert_eq!(pair.settle(), [granted(right)]);
        assert!(pair.send(right, b"ping").is_empty());
        assert_eq!(pair.peer.rustls().received, b"ping");
        assert!(pair.read.is_none(), "output cannot restart withdrawn read");
    }
}

#[test]
fn withdrawn_header_keeps_granted_output_usable() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        let right = Token::new(110);
        assert!(pair.read(16).is_empty());
        assert!(pair.down(native::Request::Output(OutputDown::Room { right, bytes: 16 })).is_empty());
        assert_eq!(pair.settle(), [granted(right)]);
        let winner = pair.queued_ciphertext(false);
        let before = pair.last_send();
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty(), "withdrawn winner creates no observation above");
        assert!(pair.read.is_none(), "no ciphertext Fill after withdrawal");
        assert_eq!(pair.last_send(), before, "dropped read spends no output");

        assert!(pair.send(right, b"ping").is_empty());
        assert_eq!(pair.peer.rustls().received, b"ping");
        assert!(pair.read.is_none(), "output cannot restart withdrawn read");
    }
}

#[test]
fn withdrawn_body_keeps_pending_output_usable() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        let right = Token::new(110);
        assert!(pair.read(16).is_empty());
        assert!(pair.down(native::Request::Output(OutputDown::Room { right, bytes: 16 })).is_empty());

        let winner = pair.queued_ciphertext(true);
        let before = pair.last_send();
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty(), "withdrawn winner creates no observation above");
        assert!(pair.read.is_none(), "no ciphertext Fill after withdrawal");
        assert_eq!(pair.last_send(), before, "dropped read spends no output");
        assert_eq!(pair.settle(), [granted(right)]);
        assert!(pair.send(right, b"ping").is_empty());
        assert_eq!(pair.peer.rustls().received, b"ping");
        assert!(pair.read.is_none(), "output cannot restart withdrawn read");
    }
}

#[test]
fn withdrawn_body_keeps_granted_output_usable() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        let right = Token::new(110);
        assert!(pair.read(16).is_empty());
        assert!(pair.down(native::Request::Output(OutputDown::Room { right, bytes: 16 })).is_empty());
        assert_eq!(pair.settle(), [granted(right)]);
        let winner = pair.queued_ciphertext(true);
        let before = pair.last_send();
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty(), "withdrawn winner creates no observation above");
        assert!(pair.read.is_none(), "no ciphertext Fill after withdrawal");
        assert_eq!(pair.last_send(), before, "dropped read spends no output");

        assert!(pair.send(right, b"ping").is_empty());
        assert_eq!(pair.peer.rustls().received, b"ping");
        assert!(pair.read.is_none(), "output cannot restart withdrawn read");
    }
}

#[test]
fn withdrawn_header_drops_before_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(false);
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty());
        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawn_header_drops_during_flush() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(false);
        assert!(pair.withdraw().is_empty());
        assert!(pair.finish().is_empty());
        assert_eq!(pair.finished, 0, "close_notify output is pending");
        assert!(pair.up(winner).is_empty(), "drops only the withdrawn Fill");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawn_header_drops_after_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(false);
        assert!(pair.withdraw().is_empty());
        assert!(pair.finish().is_empty());
        assert_eq!(pair.finished, 0, "close_notify output is pending");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "output settled before winner delivery");
        assert!(pair.up(winner).is_empty(), "drops only the withdrawn Fill");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawn_body_drops_before_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(true);
        assert!(pair.withdraw().is_empty());
        assert!(pair.up(winner).is_empty());
        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawn_body_drops_during_flush() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(true);
        assert!(pair.withdraw().is_empty());
        assert!(pair.finish().is_empty());
        assert_eq!(pair.finished, 0, "close_notify output is pending");
        assert!(pair.up(winner).is_empty(), "drops only the withdrawn Fill");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawn_body_drops_after_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let winner = pair.queued_ciphertext(true);
        assert!(pair.withdraw().is_empty());
        assert!(pair.finish().is_empty());
        assert_eq!(pair.finished, 0, "close_notify output is pending");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "output settled before winner delivery");
        assert!(pair.up(winner).is_empty(), "drops only the withdrawn Fill");
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1, "one Finish below");
        let peer = pair.peer.rustls();
        assert!(peer.closed && peer.failed.is_none(), "peer authenticates close_notify");
        assert!(peer.received.is_empty(), "read disposal invents no plaintext");
        assert!(pair.read.is_none() && pair.output_bytes().is_none());
        assert!(pair.close().is_empty());
        assert_eq!(pair.up(native::LowerEvent::Closed), [native::Event::Client(client::Event::Closed)]);
        assert!(pair.settle().is_empty(), "physical Closed has no second terminal");
    }
}

#[test]
fn withdrawal_during_handshake_keeps_engine_reads_until_ready() {
    for versions in [pki::Versions::Tls12, pki::Versions::Tls13] {
        let server = pki::Server { versions, ..pki::Server::plain() };
        let mut pair = skein_tls_world::native_pair::Pair::new(
            &pki::client(&[]),
            pki::name(),
            client::Limits { read: 16, send: 16, records: client::MAX_RECORD },
            Server::new(server.config()),
        );
        assert!(pair.down(native::Request::Client(client::Request::Handshake)).is_empty());
        assert_eq!(pair.read, Some(Read::Fill(client::HEADER)), "observed handshake engine read");
        assert!(pair.read(16).is_empty());
        assert!(pair.withdraw().is_empty());
        assert_eq!(
            pair.read,
            Some(Read::Fill(client::HEADER)),
            "upper withdrawal cannot withdraw engine handshake read"
        );
        assert!(matches!(&pair.settle()[..], [native::Event::Client(client::Event::Ready(_))]));
        assert!(!pair.peer.rustls().handshaking());
        assert!(pair.read.is_none(), "Ready does not create an ordinary read after withdrawal");
    }
}

#[test]
fn peer_end_after_header_allows_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let peer = pair.peer.rustls_mut();
        peer.write(b"x");
        peer.close_notify();
        pair.pull();
        let winner = pair.answer().expect("queued Header before peer End");
        assert!(matches!(&winner, native::LowerEvent::Stream(Up::Bytes(_))));
        assert!(pair.up(winner).is_empty(), "short plaintext cannot answer Fill16");
        assert_eq!(
            pair.settle(),
            [native::Event::Client(client::Event::Stream(Up::End))],
            "close_notify is End, never a short Fill answer"
        );
        assert!(pair.read.is_none(), "peer close forbids another ciphertext Fill");

        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1);
        assert!(pair.peer_closed(), "opposite close_notify authenticates");
    }
}

#[test]
fn peer_end_after_header_allows_withdrawal_and_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let peer = pair.peer.rustls_mut();
        peer.write(b"x");
        peer.close_notify();
        pair.pull();
        let winner = pair.answer().expect("queued Header before peer End");
        assert!(matches!(&winner, native::LowerEvent::Stream(Up::Bytes(_))));
        assert!(pair.up(winner).is_empty(), "short plaintext cannot answer Fill16");
        assert_eq!(
            pair.settle(),
            [native::Event::Client(client::Event::Stream(Up::End))],
            "close_notify is End, never a short Fill answer"
        );
        assert!(pair.read.is_none(), "peer close forbids another ciphertext Fill");
        assert!(pair.withdraw().is_empty(), "crossed End leaves Fill outstanding for withdrawal");
        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1);
        assert!(pair.peer_closed(), "opposite close_notify authenticates");
    }
}

#[test]
fn peer_end_after_body_allows_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let peer = pair.peer.rustls_mut();
        peer.write(b"x");
        peer.close_notify();
        pair.pull();
        let mut winner = pair.answer().expect("queued Header before peer End");
        assert!(pair.up(winner).is_empty());
        winner = pair.answer().expect("queued Body before peer End");
        assert!(matches!(&winner, native::LowerEvent::Stream(Up::Bytes(_))));
        assert!(pair.up(winner).is_empty(), "short plaintext cannot answer Fill16");
        assert_eq!(
            pair.settle(),
            [native::Event::Client(client::Event::Stream(Up::End))],
            "close_notify is End, never a short Fill answer"
        );
        assert!(pair.read.is_none(), "peer close forbids another ciphertext Fill");

        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1);
        assert!(pair.peer_closed(), "opposite close_notify authenticates");
    }
}

#[test]
fn peer_end_after_body_allows_withdrawal_and_finish() {
    for version in [Version::Tls12, Version::Tls13] {
        let mut pair = Pair::ready_version(version);
        assert!(pair.read(16).is_empty());
        let peer = pair.peer.rustls_mut();
        peer.write(b"x");
        peer.close_notify();
        pair.pull();
        let mut winner = pair.answer().expect("queued Header before peer End");
        assert!(pair.up(winner).is_empty());
        winner = pair.answer().expect("queued Body before peer End");
        assert!(matches!(&winner, native::LowerEvent::Stream(Up::Bytes(_))));
        assert!(pair.up(winner).is_empty(), "short plaintext cannot answer Fill16");
        assert_eq!(
            pair.settle(),
            [native::Event::Client(client::Event::Stream(Up::End))],
            "close_notify is End, never a short Fill answer"
        );
        assert!(pair.read.is_none(), "peer close forbids another ciphertext Fill");
        assert!(pair.withdraw().is_empty(), "crossed End leaves Fill outstanding for withdrawal");
        assert!(pair.finish().is_empty());
        assert!(pair.settle().is_empty());
        assert_eq!(pair.finished, 1);
        assert!(pair.peer_closed(), "opposite close_notify authenticates");
    }
}
