use crate::{
    fixture::encode,
    mask::{self, Mask},
    write::{self, Leak, SecretKind, Secrets},
};
use skein_throwaway_forgejo::Exchange;
#[test]
fn masking_rewrites_ports_ids_times_and_dates_by_first_appearance() {
    let rows: &[(&[u8], &[u8])] = &[
        (b"http://127.0.0.1:41823/1", b"http://127.0.0.1:3000/1"),
        (b"9fceb02d0ae598e95dc970b74767f19372d61af8", b"0000000000000000000000000000000000000001"),
        (b"\"2026-10-14T10:12:13+05:30\"", b"\"2020-01-01T00:00:01+05:30\""),
        (
            b"Date: Fri, 16 Oct 2026 00:00:00 GMT\r\nLast-Modified: yesterday\r\n",
            b"Date: Wed, 01 Jan 2020 00:00:00 GMT\r\nLast-Modified: Wed, 01 Jan 2020 00:00:00 GMT\r\n",
        ),
        (b"\"0001-01-01T00:00:00Z\" \"1970-01-01T00:00:00Z\"", b"\"0001-01-01T00:00:00Z\" \"1970-01-01T00:00:00Z\""),
    ];
    for (input, expected) in rows {
        assert_eq!(Mask::new(41823).bytes(input), *expected, "{input:?}");
    }
    let mut mask = Mask::new(3000);
    assert_eq!(mask.bytes(b"9fceb02d0ae598e95dc970b74767f19372d61af8"), b"0000000000000000000000000000000000000001");
    assert_eq!(
        mask.bytes(b"9fceb02d0ae598e95dc970b74767f19372d61af8 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        b"0000000000000000000000000000000000000001 0000000000000000000000000000000000000002"
    );
}
#[test]
fn a_chunked_response_is_written_unframed_with_its_masked_length() {
    let exchange = Exchange {
        sent: Vec::new(),
        status: 200,
        head: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n".to_vec(),
        body: b"127.0.0.1:41823".to_vec(),
    };
    assert_eq!(
        mask::response(&mut Mask::new(41823), &exchange),
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 14\r\n\r\n127.0.0.1:3000"
    );
}
#[test]
fn a_secret_in_a_case_stops_the_run_naming_the_case() {
    let secrets = Secrets::of(&[(SecretKind::ApiToken, "0123456789abcdef0123456789abcdef01234567")]);
    let leaked = write::check(
        "user",
        b"GET /api/v1/user HTTP/1.1\r\nAuthorization: token MASKED\r\n\r\n",
        b"HTTP/1.1 401 Unauthorized\r\n\r\n{\"message\":\"token 0123456789abcdef0123456789abcdef01234567\"}",
        &secrets,
    );
    assert_eq!(leaked, Err(Leak { case: "user".to_owned(), kind: SecretKind::ApiToken }));
}
#[test]
fn targets_encode_every_byte_but_unreserved_ones() {
    for (plain, encoded) in [
        ("ABCxyz019-._~", "ABCxyz019-._~"),
        ("feature/slash", "feature%2Fslash"),
        ("kit / label", "kit%20%2F%20label"),
        ("é+?&:", "%C3%A9%2B%3F%26%3A"),
    ] {
        assert_eq!(encode(plain), encoded, "{plain}");
    }
}
#[test]
fn a_fact_line_is_holds_fails_or_unchecked_with_its_reason() {
    use crate::facts::{Fact, Outcome};
    for (outcome, expected) in [
        (Outcome::Holds, "holds example\n"),
        (Outcome::Observed("reads=[false, true]".into()), "holds example\n# example: reads=[false, true]\n"),
        (Outcome::Fails("HTTP 500".into()), "fails example: HTTP 500\n"),
        (Outcome::Unchecked("runner absent".into()), "unchecked example: runner absent\n"),
    ] {
        assert_eq!(Fact { name: "example", outcome }.line(), expected);
    }
}
#[test]
fn a_fact_read_from_a_case_compares_its_status_and_message() {
    let mut exchange = Exchange {
        sent: Vec::new(),
        status: 409,
        head: b"HTTP/1.1 409 Conflict\r\n".to_vec(),
        body: b"{\"message\":\"head out of date\"}".to_vec(),
    };
    assert!(crate::facts::matches(&exchange, 409, "head out of date"));
    exchange.status = 405;
    assert!(!crate::facts::matches(&exchange, 409, "head out of date"));
}
#[test]
fn the_component_block_is_replaced_and_other_lines_kept() {
    use crate::facts::{self, Fact, Outcome};
    let facts = [Fact { name: "component-read-items", outcome: Outcome::Fails("TooLarge".to_owned()) }];
    for (before, expected) in [
        (
            "# forgejo pinned\n# runner none\nholds merge\nholds component-read-items\n",
            "# forgejo pinned\n# runner none\nholds merge\nfails component-read-items: TooLarge\n",
        ),
        (
            "holds component-start-ready\nholds merge\nunchecked component-read-job: none\n",
            "fails component-read-items: TooLarge\nholds merge\n",
        ),
        ("holds merge", "holds merge\nfails component-read-items: TooLarge\n"),
    ] {
        assert_eq!(facts::replace_block(before, "component-", &facts), expected, "{before}");
    }
}
#[test]
fn a_sent_request_is_counted_by_its_request_line() {
    for (name, bytes, expected) in [
        ("head", b"GET /api/v1/user HTTP/1.1\r\nAuthorization: token MASKED\r\n\r\n".as_slice(), 1),
        ("body", b"{\"body\":\"GET /api/v1/user\"}", 0),
        ("two heads", b"POST /api/v1/repos/a/b/issues HTTP/1.1\r\n\r\nGET /api/v1/user HTTP/1.1\r\n", 2),
    ] {
        assert_eq!(crate::component::hosted::sent_requests(bytes), expected, "{name}");
    }
}

#[test]
fn a_password_is_read_between_its_first_and_last_quote() {
    for (text, expected) in [
        ("generated random password is 'a'b'\nnext", Some("a'b")),
        ("no quotes", None),
        ("one'", None),
        ("''", Some("")),
    ] {
        assert_eq!(crate::cli::password_of(text), expected, "{text}");
    }
}
#[test]
fn a_raw_token_is_forty_lowercase_digits_and_a_line_feed() {
    for (text, expected) in [
        ("0123456789abcdef0123456789abcdef01234567\n", Some("0123456789abcdef0123456789abcdef01234567")),
        ("0123456789abcdef0123456789abcdef01234567", None),
        ("0123456789abcdef0123456789abcdef0123456\n", None),
        ("0123456789ABCDEF0123456789abcdef01234567\n", None),
        ("0123456789abcdef0123456789abcdef01234567\n\n", None),
    ] {
        assert_eq!(crate::cli::raw_token(text), expected, "{text:?}");
    }
}

#[test]
fn the_runner_configuration_names_one_connection_by_its_token_file() {
    let text = crate::registration::configuration(
        "http://127.0.0.1:3000",
        "30313233-3435-3637-3839-616263646566",
        std::path::Path::new("/scratch/runner/secret"),
        std::path::Path::new("/scratch/runner/jobs"),
    );
    assert_eq!(
        text,
        "log:\n  level: info\nrunner:\n  capacity: 1\n  timeout: 1m\n  shutdown_timeout: 1s\n  fetch_interval: 100ms\n  report_interval: 100ms\ncache:\n  enabled: false\nhost:\n  workdir_parent: /scratch/runner/jobs\nserver:\n  connections:\n    forgejo:\n      url: http://127.0.0.1:3000/\n      uuid: 30313233-3435-3637-3839-616263646566\n      token_url: file:/scratch/runner/secret\n      labels:\n        - kit-host:host\n"
    );
}
#[test]
fn a_listed_runner_is_found_by_its_uuid() {
    let body = crate::json::Json::parse(r#"[{"id":1,"uuid":"30313233-3435-3637-3839-616263646566","owner_id":0,"repo_id":0,"name":"r1","status":"idle","version":"v@","labels":["kit-host"],"description":"","ephemeral":false}]"#.replace('@', skein_throwaway_forgejo::RUNNER_VERSION).as_bytes()).expect("row");
    let row = crate::registration::listed(&body, "30313233-3435-3637-3839-616263646566").expect("found");
    assert_eq!(row.get("name").str(), Some("r1"));
    assert_eq!(crate::registration::listed(&body, "absent").map(|_| ()), None);
}
#[test]
fn the_clone_workflow_runs_steps_only_and_names_the_jobs_token() {
    let workflow = crate::job_clone::workflow();
    assert!(!workflow.contains("uses:"));
    assert_eq!(workflow.matches("github.token").count(), 1);
    assert_eq!(workflow.matches("git clone").count(), 1);
    assert!(workflow.contains("http.extraHeader=Authorization: basic $auth"));
    assert!(workflow.contains("test -s b/README.md"));
}

#[test]
fn a_signature_holds_only_for_the_body_it_signs() {
    use crate::deliveries::{Received, signed};
    let mut received = Received { head: b"X-Forgejo-Signature: 1f5a95724ba9e23c63adf66cb4c7df0ea84725008688c6ddd5606088fc16ca20\r\nX-Hub-Signature-256: sha256=1f5a95724ba9e23c63adf66cb4c7df0ea84725008688c6ddd5606088fc16ca20\r\n".to_vec(), body: b"body".to_vec() };
    assert!(signed(&received));
    received.body.push(b'!');
    assert!(!signed(&received));
}
#[test]
fn masking_also_rewrites_the_listeners_port_and_delivery_ids() {
    let mut mask = Mask::new(40000);
    mask.listener(40001);
    assert_eq!(mask.bytes(b"Host: 127.0.0.1:40001\r\nX-Forgejo-Delivery: 12345678-1234-4567-89ab-123456789abc\r\nX-Gitea-Delivery: 12345678-1234-4567-89ab-123456789abc\r\n"),b"Host: 127.0.0.1:3001\r\nX-Forgejo-Delivery: 00000000-0000-0000-0000-000000000001\r\nX-Gitea-Delivery: 00000000-0000-0000-0000-000000000001\r\n");
}
#[test]
fn a_masked_delivery_keeps_a_content_length_that_matches_its_body() {
    let mut mask = Mask::new(40000);
    mask.listener(40001);
    let exchange = Exchange {
        sent: Vec::new(),
        status: 0,
        head: b"POST /hooks/1 HTTP/1.1\r\nContent-Length: 15\r\n\r\n".to_vec(),
        body: b"127.0.0.1:40001".to_vec(),
    };
    assert_eq!(
        mask::response(&mut mask, &exchange),
        b"POST /hooks/1 HTTP/1.1\r\nContent-Length: 14\r\n\r\n127.0.0.1:3001"
    );
}
#[test]
fn the_listener_holds_its_answer_until_the_probe_replies() {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };
    let listener = crate::deliveries::Listener::start().expect("listener");
    let mut client = TcpStream::connect(("127.0.0.1", listener.port)).expect("connect");
    client.write_all(b"POST /hooks/1 HTTP/1.1\r\nContent-Length: 4\r\n\r\nbody").expect("sent");
    let (received, reply) = listener.next(Duration::from_secs(5)).expect("delivery");
    assert_eq!(received.body, b"body");
    client.set_read_timeout(Some(Duration::from_millis(100))).expect("timeout");
    assert!(client.read(&mut [0; 1]).is_err());
    reply.answer(204);
    client.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    let mut answer = String::new();
    client.read_to_string(&mut answer).expect("answer");
    assert!(answer.starts_with("HTTP/1.1 204 No Content\r\n"), "{answer}");
}
#[test]
fn the_roll_up_ignores_the_losers_of_the_pairs() {
    use crate::facts::{Fact, Outcome};
    let mut facts = vec![
        Fact { name: "delivery-job-end-follows-its-commit-status", outcome: Outcome::Holds },
        Fact { name: "delivery-job-end-precedes-its-commit-status", outcome: Outcome::Fails("later".into()) },
        Fact {
            name: "delivery-pull-is-settled-when-its-push-is-delivered",
            outcome: Outcome::Fails("checking".into()),
        },
        Fact { name: "delivery-pull-is-checking-when-its-push-is-delivered", outcome: Outcome::Holds },
    ];
    facts.push(Fact { name: "delivery-cancelled-job-end-is-delivered", outcome: Outcome::Fails("absent".into()) });
    facts.push(Fact { name: "delivery-branch-deletion-sends-no-push", outcome: Outcome::Fails("push".into()) });
    assert!(crate::deliveries::roll_up(&facts));
    facts[0].outcome = Outcome::Fails("varied".into());
    assert!(!crate::deliveries::roll_up(&facts));
}

#[test]
fn the_pull_pair_records_any_checking_probe_that_settles() {
    for (reads, settled, expected) in [
        (vec![vec![true], vec![true]], true, (true, false)),
        (vec![vec![false, true], vec![true]], true, (false, true)),
        (vec![vec![true], vec![false, false, true]], true, (false, true)),
        (vec![vec![false, true], vec![false, true]], true, (false, true)),
        (vec![vec![false, false], vec![true]], false, (false, false)),
        (vec![vec![true]], true, (false, false)),
    ] {
        assert_eq!(crate::deliveries::pull_pair(&reads, settled), expected, "{reads:?}, settled={settled}");
    }
}
#[test]
fn a_fresh_password_in_a_body_leaves_its_transcript_masked() {
    let password = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let secrets = Secrets::of(&[(SecretKind::Password, password)]);
    let body = format!("{{\"password\":\"{password}\"}}");
    let masked = Mask::new(3000).bytes(body.as_bytes());
    assert_eq!(masked, br#"{"password":"0000000000000000000000000000000000000000000000000000000000000001"}"#);
    assert_eq!(write::check("new password", &masked, b"", &secrets), Ok(()));
}

#[test]
fn a_provisioning_failure_keeps_at_most_two_hundred_body_bytes() {
    let exchange = Exchange { sent: Vec::new(), status: 500, head: Vec::new(), body: vec![b'x'; 201] };
    assert_eq!(
        crate::fixture::api_answer("/pulls", &exchange).err().expect("HTTP 500").to_string(),
        concat!(
            "provisioning /pulls: HTTP 500: ",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
        )
    );
}

#[test]
fn a_made_tokens_value_and_tail_leave_its_transcript_masked() {
    let exchange = Exchange {
        sent: Vec::new(),
        status: 201,
        head: b"HTTP/1.1 201 Created\r\n".to_vec(),
        body: br#"{"sha1":"0123456789abcdef0123456789abcdef01234567","token_last_eight":"01234567"}"#.to_vec(),
    };
    let mut secrets = Secrets::of(&[]);
    let mut mask = Mask::new(3000);
    crate::cases::made_token(&exchange, &mut mask, &mut secrets);
    let masked = mask::response(&mut mask, &exchange);
    assert_eq!(masked, b"HTTP/1.1 201 Created\r\nContent-Length: 81\r\n\r\n{\"sha1\":\"0000000000000000000000000000000000000002\",\"token_last_eight\":\"00000001\"}");
    assert_eq!(write::check("made token", b"", &masked, &secrets), Ok(()));
}

#[test]
fn git_packet_ids_share_the_plain_ids_first_appearance_order() {
    let mut mask = Mask::new(3000);
    assert_eq!(mask.bytes(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), b"0000000000000000000000000000000000000001");
    assert_eq!(
        mask.bytes(b"0000002fbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb x\n002faaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa y\n002fbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb z\n0000"),
        b"0000002f0000000000000000000000000000000000000002 x\n002f0000000000000000000000000000000000000001 y\n002f0000000000000000000000000000000000000002 z\n0000"
    );
}

#[test]
fn a_truncated_git_packet_keeps_its_length_attached_to_the_hex_run() {
    let mut mask = Mask::new(3000);
    assert_eq!(
        mask.bytes(b"003fbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb x\n"),
        b"003fbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb x\n"
    );
}

#[test]
fn masking_sorts_the_deprecated_units_set_in_every_team() {
    let exchange = Exchange {
        sent: Vec::new(),
        status: 200,
        head: b"HTTP/1.1 200 OK\r\n".to_vec(),
        body: br#"[{"units":["repo.pulls","repo.code"]},{"units":["repo.issues","repo.actions"]}]"#.to_vec(),
    };
    assert_eq!(
        mask::response(&mut Mask::new(3000), &exchange),
        b"HTTP/1.1 200 OK\r\nContent-Length: 79\r\n\r\n[{\"units\":[\"repo.code\",\"repo.pulls\"]},{\"units\":[\"repo.actions\",\"repo.issues\"]}]"
    );
}
