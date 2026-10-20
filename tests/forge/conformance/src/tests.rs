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
    assert!(crate::facts::matches(&conflict_exchange(), 409, "head out of date"));
}
#[test]
fn a_fact_read_from_a_case_rejects_a_different_status() {
    let mut exchange = conflict_exchange();
    exchange.status = 405;
    assert!(!crate::facts::matches(&exchange, 409, "head out of date"));
}
fn conflict_exchange() -> Exchange {
    Exchange {
        sent: Vec::new(),
        status: 409,
        head: b"HTTP/1.1 409 Conflict\r\n".to_vec(),
        body: b"{\"message\":\"head out of date\"}".to_vec(),
    }
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
    let body = listed_runner();
    let row = crate::registration::listed(&body, "30313233-3435-3637-3839-616263646566").expect("found");
    assert_eq!(row.get("name").str(), Some("r1"));
}
#[test]
fn a_runner_absent_from_the_list_is_not_found() {
    let body = listed_runner();
    assert_eq!(crate::registration::listed(&body, "absent").map(|_| ()), None);
}
fn listed_runner() -> crate::json::Json {
    crate::json::Json::parse(r#"[{"id":1,"uuid":"30313233-3435-3637-3839-616263646566","owner_id":0,"repo_id":0,"name":"r1","status":"idle","version":"v@","labels":["kit-host"],"description":"","ephemeral":false}]"#.replace('@', skein_throwaway_forgejo::RUNNER_VERSION).as_bytes()).expect("row")
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
    assert!(crate::deliveries::signed(&signed_body()));
}
#[test]
fn a_signature_refuses_a_changed_body() {
    let mut received = signed_body();
    received.body.push(b'!');
    assert!(!crate::deliveries::signed(&received));
}
fn signed_body() -> crate::deliveries::Received {
    crate::deliveries::Received { head: b"X-Forgejo-Signature: 1f5a95724ba9e23c63adf66cb4c7df0ea84725008688c6ddd5606088fc16ca20\r\nX-Hub-Signature-256: sha256=1f5a95724ba9e23c63adf66cb4c7df0ea84725008688c6ddd5606088fc16ca20\r\n".to_vec(), body: b"body".to_vec() }
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
    let exchange = Exchange {
        sent: Vec::new(),
        status: 0,
        head: b"POST /hooks/1 HTTP/1.1\r\nContent-Length: 33\r\n\r\n".to_vec(),
        body: b"2026-10-18T12:34:56.1234567+05:30".to_vec(),
    };
    assert_eq!(
        mask::response(&mut mask, &exchange),
        b"POST /hooks/1 HTTP/1.1\r\nContent-Length: 33\r\n\r\n2020-01-01T00:00:01.0000000+05:30"
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

#[test]
fn unquoted_log_times_keep_fraction_width_offset_and_identity() {
    let mut mask = Mask::new(3000);
    let raw = b"2026-10-18T12:34:56.1234567+05:30";
    assert_eq!(mask.bytes(b"\"2026-10-18T12:34:56.1234567+05:30\""), b"\"2020-01-01T00:00:01+05:30\"");
    assert_eq!(mask.bytes(raw), b"2020-01-01T00:00:01.0000000+05:30");
    assert_eq!(mask.bytes(b"2026-10-18T12:34:57.89Z"), b"2020-01-01T00:00:02.00Z");
    assert_eq!(mask.bytes(b"\"2026-10-18T12:34:57.89Z\""), b"\"2020-01-01T00:00:02Z\"");
    assert_eq!(mask.bytes(raw), b"2020-01-01T00:00:01.0000000+05:30");
}

#[test]
fn recorded_corpus_has_no_unmasked_uuid_or_log_time() {
    let mut bad_uuids = 0;
    let mut bad_times = 0;
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for group in ["transcripts", "deliveries"] {
        for case in std::fs::read_dir(corpus.join(group)).expect("corpus group") {
            let case = case.expect("case").path();
            if !case.is_dir() {
                continue;
            }
            for name in ["request", "response"] {
                let bytes = std::fs::read(case.join(name)).expect("capture");
                let (uuids, times) = unmasked_fields(&bytes);
                bad_uuids += uuids;
                bad_times += times;
            }
        }
    }
    assert_eq!(bad_uuids, 0);
    assert_eq!(bad_times, 0);
}

// Inspect captured bytes directly, independently of the mask and the dialect decoder.
fn unmasked_fields(bytes: &[u8]) -> (usize, usize) {
    const UUID: &[u8] = b"00000000-0000-0000-0000-000000000000";
    const CANONICAL_DATE: &[u8] = b"2020-01-01T";
    const TIME: &[u8] = b"2020-01-01T00:00:00";
    let mut bad_uuids = 0;
    let mut bad_times = 0;
    let mut at = 0;
    // Hyphens select the sparse field candidates without walking every large-body window.
    for part in bytes.split(|b| *b == b'-') {
        at += part.len();
        if let Some(start) = at.checked_sub(8)
            && let Some(value) = bytes.get(start..start + UUID.len())
            && value
                .iter()
                .enumerate()
                .all(|(i, b)| if [8, 13, 18, 23].contains(&i) { *b == b'-' } else { b.is_ascii_hexdigit() })
            && !value.starts_with(b"00000000-0000-0000-0000-")
        {
            bad_uuids += 1;
        }
        if let Some(start) = at.checked_sub(4)
            && let Some(value) = bytes.get(start..start + TIME.len())
            && value.iter().enumerate().all(|(i, b)| match i {
                4 | 7 => *b == b'-',
                10 => *b == b'T',
                13 | 16 => *b == b':',
                _ => b.is_ascii_digit(),
            })
            && (&b"2000"[..]..=&b"2999"[..]).contains(&&value[..4])
        {
            let tail = &bytes[start + TIME.len()..];
            let fraction = tail.strip_prefix(b".").unwrap_or_default();
            if !value.starts_with(CANONICAL_DATE)
                || fraction.iter().take_while(|b| b.is_ascii_digit()).any(|b| *b != b'0')
            {
                bad_times += 1;
            }
        }
        at += 1;
    }
    (bad_uuids, bad_times)
}

const USER_REPOS_ALICE: &str =
    r#"{"id":30,"name":"widgets","private":true,"owner":{"id":1,"login":"alice"},"permissions":{"push":true}}"#;
const USER_REPOS_BOB: &str =
    r#"{"id":10,"name":"widgets","private":true,"owner":{"id":2,"login":"bob"},"permissions":{"push":false}}"#;
const USER_REPOS_ACME: &str =
    r#"{"id":20,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":true}}"#;

fn user_repos_facts(name: &str, all: &str, rest: &str, second: &str, repeat_status: u16) -> Vec<crate::facts::Fact> {
    let out = std::env::temp_dir().join(format!("skein-user-repos-{}-{name}", std::process::id()));
    for (name, status, body) in [
        ("setup-user-repos", 200, format!("[{USER_REPOS_BOB}]")),
        ("setup-user-repos-page-2", 200, format!("[{second}]")),
        ("setup-user-repos-all", 200, all.to_owned()),
        ("setup-user-repos-rest", 200, rest.to_owned()),
        ("setup-user-repos-repeat", repeat_status, format!("[{USER_REPOS_BOB}]")),
        ("setup-user-repos-past-end", 200, "[]".to_owned()),
    ] {
        let directory = out.join("transcripts").join(name);
        std::fs::create_dir_all(&directory).expect("canned transcript directory");
        std::fs::write(
            directory.join("response"),
            format!("HTTP/1.1 {status} canned\r\nContent-Type: application/json\r\n\r\n{body}"),
        )
        .expect("canned response");
    }
    let facts = crate::repos::facts(&out).expect("repository facts");
    std::fs::remove_dir_all(out).expect("remove canned transcripts");
    facts
}

#[test]
fn forgejo_user_repos_facts_accept_owned_collaborating_team_private_pages() {
    let all = format!("[{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}]");
    let facts = user_repos_facts("holds", &all, "[]", USER_REPOS_ACME, 200);
    assert_eq!(facts.len(), 3);
    assert!(facts.iter().all(|fact| matches!(fact.outcome, crate::facts::Outcome::Holds)));
}

#[test]
fn forgejo_user_repos_facts_accept_listing_capped_at_two_rows() {
    let all = format!("[{USER_REPOS_BOB},{USER_REPOS_ACME}]");
    let rest = format!("[{USER_REPOS_ALICE}]");
    let facts = user_repos_facts("capped", &all, &rest, USER_REPOS_ACME, 200);
    assert!(facts.iter().all(|fact| matches!(fact.outcome, crate::facts::Outcome::Holds)));
}

#[test]
fn forgejo_user_repos_facts_reject_missing_team_or_private_access() {
    for (name, all) in [
        ("missing-team", format!("[{USER_REPOS_BOB},{USER_REPOS_ALICE}]")),
        (
            "private",
            format!("[{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}]")
                .replace("\"private\":true", "\"private\":false"),
        ),
    ] {
        let facts = user_repos_facts(name, &all, "[]", USER_REPOS_ACME, 200);
        assert!(matches!(facts[0].outcome, crate::facts::Outcome::Fails(_)));
    }
}

#[test]
fn forgejo_user_repos_facts_require_boolean_push_on_every_row() {
    for (name, permissions) in [
        ("missing-push", "\"permissions\":{}"),
        ("push-type", "\"permissions\":{\"push\":\"false\"}"),
        ("push-grant", "\"permissions\":{\"push\":true}"),
    ] {
        let bob = USER_REPOS_BOB.replace("\"permissions\":{\"push\":false}", permissions);
        let all = format!("[{bob},{USER_REPOS_ACME},{USER_REPOS_ALICE}]");
        let facts = user_repos_facts(name, &all, "[]", USER_REPOS_ACME, 200);
        assert!(matches!(facts[0].outcome, crate::facts::Outcome::Holds));
        assert!(matches!(facts[1].outcome, crate::facts::Outcome::Fails(_)));
    }
}

#[test]
fn forgejo_user_repos_facts_reject_repeated_pages_and_refused_reads() {
    let all = format!("[{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}]");
    for (name, second, status) in [("same-page", USER_REPOS_BOB, 200), ("refused", USER_REPOS_ACME, 401)] {
        let facts = user_repos_facts(name, &all, "[]", second, status);
        assert!(matches!(facts[2].outcome, crate::facts::Outcome::Fails(_)));
    }
}

fn search_facts(name: &str, first: &str, second: &str, pages: &[&str], status: u16) -> Vec<crate::facts::Fact> {
    let out = std::env::temp_dir().join(format!("skein-repo-search-{}-{name}", std::process::id()));
    let mut responses = vec![
        ("setup-search".to_owned(), status, first.to_owned()),
        ("setup-search-page-2".to_owned(), 200, second.to_owned()),
        ("setup-search-repeat".to_owned(), 200, first.to_owned()),
    ];
    for (index, body) in pages.iter().enumerate() {
        responses.push((format!("setup-search-all-{}", index + 1), 200, (*body).to_owned()));
    }
    for (name, status, body) in responses {
        let directory = out.join("transcripts").join(name);
        std::fs::create_dir_all(&directory).expect("canned search directory");
        std::fs::write(
            directory.join("response"),
            format!("HTTP/1.1 {status} canned\r\nContent-Type: application/json\r\n\r\n{body}"),
        )
        .expect("canned search response");
    }
    let facts = crate::search::facts(&out).expect("search facts");
    std::fs::remove_dir_all(out).expect("remove canned search transcripts");
    facts
}
fn search_answer(rows: &str) -> String {
    format!(r#"{{"ok":true,"data":[{rows}]}}"#)
}
#[test]
fn forgejo_repo_search_facts_accept_private_owners_and_capped_pages() {
    let first = search_answer(USER_REPOS_BOB);
    let second = search_answer(USER_REPOS_ACME);
    let all = search_answer(&format!("{USER_REPOS_BOB},{USER_REPOS_ACME}"));
    let rest = search_answer(USER_REPOS_ALICE);
    let facts = search_facts("holds", &first, &second, &[&all, &rest, &search_answer("")], 200);
    assert_eq!(facts.len(), 3);
    assert_eq!(
        facts.iter().map(|fact| fact.name).collect::<Vec<_>>(),
        vec![
            "setup-search-by-an-admin-lists-every-owners-private-repositories",
            "setup-search-answers-ok-and-data",
            "setup-search-sorted-by-id-pages-by-page-and-limit",
        ]
    );
    assert!(facts.iter().all(|fact| matches!(fact.outcome, crate::facts::Outcome::Holds)));
}
#[test]
fn forgejo_repo_search_facts_reject_missing_owner_or_private_repository() {
    let first = search_answer(USER_REPOS_BOB);
    let second = search_answer(USER_REPOS_ACME);
    for (name, rows) in [
        ("missing-owner", format!("{USER_REPOS_BOB},{USER_REPOS_ALICE}")),
        (
            "public",
            format!("{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}")
                .replace("\"private\":true", "\"private\":false"),
        ),
        ("missing-owner-id", format!("{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}").replace("\"id\":3,", "")),
    ] {
        let all = search_answer(&rows);
        let facts = search_facts(name, &first, &second, &[&all, &search_answer("")], 200);
        assert!(matches!(facts[0].outcome, crate::facts::Outcome::Fails(_)));
    }
}
#[test]
fn forgejo_repo_search_facts_require_ok_and_array_data() {
    let first = search_answer(USER_REPOS_BOB);
    let second = search_answer(USER_REPOS_ACME);
    for (name, all) in [
        ("false", r#"{"ok":false,"data":[]}"#),
        ("missing-ok", r#"{"data":[]}"#),
        ("missing-data", r#"{"ok":true}"#),
        ("wrong-data", r#"{"ok":true,"data":{}}"#),
        ("wrong-ok", r#"{"ok":"true","data":[]}"#),
    ] {
        let facts = search_facts(name, &first, &second, &[all], 200);
        assert!(matches!(facts[1].outcome, crate::facts::Outcome::Fails(_)));
    }
}
#[test]
fn forgejo_repo_search_facts_reject_repeated_or_unordered_pages() {
    let first = search_answer(USER_REPOS_BOB);
    let second = search_answer(USER_REPOS_ACME);
    for (name, second, rows) in [
        ("repeat", first.as_str(), format!("{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}")),
        ("unordered", second.as_str(), format!("{USER_REPOS_ALICE},{USER_REPOS_ACME},{USER_REPOS_BOB}")),
        (
            "duplicate",
            second.as_str(),
            format!("{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ACME},{USER_REPOS_ALICE}"),
        ),
    ] {
        let all = search_answer(&rows);
        let facts = search_facts(name, &first, second, &[&all, &search_answer("")], 200);
        assert!(matches!(facts[2].outcome, crate::facts::Outcome::Fails(_)));
    }
}
#[test]
fn forgejo_repo_search_facts_reject_refused_reads() {
    let first = search_answer(USER_REPOS_BOB);
    let second = search_answer(USER_REPOS_ACME);
    let all = search_answer(&format!("{USER_REPOS_BOB},{USER_REPOS_ACME},{USER_REPOS_ALICE}"));
    let facts = search_facts("refused", &first, &second, &[&all, &search_answer("")], 401);
    assert!(facts.iter().all(|fact| matches!(fact.outcome, crate::facts::Outcome::Fails(_))));
}
