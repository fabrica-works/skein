//! The outcomes each run is checked against (tls.md, 5).
use super::{Ending, Run, Scenario, Settings, meets, run};
use crate::pki::{self, Versions};
use skein_lib::stream::Fault;
use skein_tls::client::{self, Error, Version};

/// Runs the world as [`run`], and holds the run to what the scenario
/// implies: the handshake's outcome, the plaintext each side received, the
/// end or the error the server's ending makes, and `close_notify` sent on a
/// close or a finish once the handshake is done.
#[must_use]
pub fn check(scenario: &Scenario, settings: &Settings, seed: u64) -> Run {
    let run = run(scenario, settings, seed);
    let what = || format!("seed {seed}, {settings:?}, {:?} {:?}", scenario.server, scenario.ending);
    assert!(scenario.response.starts_with(&run.received), "what the client read is the response, in order; {}", what());
    assert!(scenario.request.starts_with(&run.served), "what the server read is the request, in order; {}", what());
    // Nothing on the way to the server is changed: its records are TLS.
    assert_eq!(run.server_failed, None, "the server reads what the client sends; {}", what());
    // The failure the stream below was told of, and nothing else, ends a
    // run whose stream failed before the outcome.
    if let (Some(fault), Some(error)) = (run.below_failed, run.failed)
        && error == Error::Stream(fault)
    {
        assert_eq!(run.stream_failed, Some(fault), "the stream told its own fault; {}", what());
        return run;
    }
    if !check_handshake(scenario, settings, &run, &what) {
        return run;
    }
    check_ending(scenario, &run, &what);
    check_left_alone(scenario, settings, &run, &what);
    run
}

fn check_handshake(scenario: &Scenario, settings: &Settings, run: &Run, what: &dyn Fn() -> String) -> bool {
    // The big chain, about 40 KB, spans three records at least, which fewer
    // than three records' room cannot hold whole: the handshake fails before
    // the certificate is judged.
    if run.failed == Some(Error::TooLong) {
        assert!(
            scenario.server.chain == pki::Chain::Big && settings.limits.records < 3 * client::MAX_RECORD,
            "a handshake message longer than the records held; {}",
            what()
        );
        assert!(run.agreed.is_none(), "{}", what());
        assert_eq!(run.stream_failed, Some(Fault::Invalid), "{}", what());
        return false;
    }
    if let Some(refusal) = scenario.refusal() {
        assert!(run.agreed.is_none(), "no handshake with a refused certificate; {}", what());
        if let Some(error) = run.failed {
            assert_eq!(error, refusal, "the certificate refused, as it should be; {}", what());
            assert_eq!(run.stream_failed, Some(Fault::Invalid), "{}", what());
        }
        assert!(run.received.is_empty() && run.served.is_empty(), "nothing exchanged; {}", what());
        return false;
    }
    if let Some(agreed) = &run.agreed {
        let version = match scenario.server.versions {
            Versions::Tls12 => Version::Tls12,
            Versions::Both | Versions::Tls13 => Version::Tls13,
        };
        assert_eq!(agreed.version, version, "the version the server allows; {}", what());
        let alpn = scenario.server.alpn.iter().find(|protocol| scenario.alpn.contains(protocol));
        assert_eq!(agreed.alpn.as_deref(), alpn.map(Vec::as_slice), "the protocol both name; {}", what());
    }
    true
}

fn check_ending(scenario: &Scenario, run: &Run, what: &dyn Fn() -> String) {
    if let Some(read) = run.ended {
        assert_eq!(scenario.ending, Ending::CloseNotify, "End only after close_notify; {}", what());
        let tail = &scenario.response[run.received.len()..];
        assert!(!meets(read, tail), "the end comes once nothing left meets the demand: {read:?}; {}", what());
    }
    match run.failed {
        None => {}
        Some(Error::Truncated) => {
            assert!(
                scenario.ending == Ending::Truncate || run.agreed.is_none(),
                "a truncation where the stream ended without close_notify; {}",
                what()
            );
            assert_eq!(run.stream_failed, Some(Fault::Invalid), "a truncation is invalid, never the end; {}", what());
            assert!(run.ended.is_none(), "{}", what());
        }
        Some(Error::Decrypt) => {
            assert_eq!(scenario.ending, Ending::Corrupt, "a record fails to decrypt where one was changed; {}", what());
            assert_eq!(run.stream_failed, Some(Fault::Invalid), "{}", what());
        }
        Some(error) => panic!("{error:?}: no failure but the scenario's; {}", what()),
    }
    // close_notify went to the server, on a finish or a close, once the
    // handshake was done, unless the connection failed first.
    if run.agreed.is_some() && run.failed.is_none() && run.below_failed.is_none() {
        assert!(run.notified, "close_notify sent: {:?}; {}", run.closed_while, what());
    }
    if run.sent && run.notified {
        assert_eq!(run.served, scenario.request, "a request sent and closed is all read; {}", what());
    }
}

fn check_left_alone(scenario: &Scenario, settings: &Settings, run: &Run, what: &dyn Fn() -> String) {
    // Left alone, a run comes to the end its server makes: the side above
    // reads to it, and no neighbour is held up for ever.
    if settings.close.is_none() && settings.failure.is_none() {
        match scenario.ending {
            Ending::CloseNotify => assert!(run.ended.is_some(), "the end; {}", what()),
            Ending::Truncate => assert_eq!(run.failed, Some(Error::Truncated), "{}", what()),
            Ending::Corrupt => assert_eq!(run.failed, Some(Error::Decrypt), "{}", what()),
            Ending::Silent => {}
        }
        // Unless the connection failed on what an eager server sent first.
        if run.failed.is_none() || !scenario.eager {
            assert!(run.sent, "the request is all sent; {}", what());
        }
    }
}
