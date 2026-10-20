use super::fixtures;
use crate::{
    api::{self, Answer, Error, Op, Read},
    forgejo::{
        client,
        decode::Malformed,
        document::{BranchProtection, Document, Row},
        plans::{plan::Step, reading},
        request::{Repo, Request},
        server,
    },
};
use alloc::boxed::Box;

const UNSIGNED: &[u8] = br#"{"rule_name":"main","enable_status_check":true,"status_check_contexts":["test"],"required_approvals":1,"dismiss_stale_approvals":true,"require_signed_commits":false}"#;
const SIGNED: &[u8] = br#"{"rule_name":"main","enable_status_check":true,"status_check_contexts":["test"],"required_approvals":1,"dismiss_stale_approvals":false,"require_signed_commits":true}"#;
const MISSING: &[u8] = br#"{"rule_name":"main","enable_status_check":true,"status_check_contexts":["test"],"required_approvals":1,"dismiss_stale_approvals":true}"#;

fn protection(signed: bool) -> BranchProtection {
    BranchProtection {
        rule_name: Box::from(*b"main"),
        enable_status_check: true,
        status_check_contexts: Box::from([Box::from(b"test".as_slice())]),
        required_approvals: 1,
        dismiss_stale_approvals: !signed,
        require_signed_commits: signed,
    }
}

fn request(list: bool) -> Request {
    let at = Repo { owner: Box::from(*b"acme"), name: Box::from(*b"widgets") };
    if list { Request::Protections { at } } else { Request::Protection { at, name: Box::from(*b"main") } }
}

fn listed(body: &[u8]) -> Box<[u8]> {
    let mut out = skein_lib::Writer::new(body.len().checked_add(2).expect("array delimiters fit"));
    out.put(b"[").unwrap();
    out.put(body).unwrap();
    out.put(b"]").unwrap();
    out.finish()
}

#[test]
fn protection_decodes_signed_commits_in_single_answers_and_lists() {
    let limits = fixtures::limits();
    for (body, signed) in [(UNSIGNED, false), (SIGNED, true)] {
        let decoded = fixtures::decode(client::decoder(&request(false), &limits).unwrap(), body, &limits);
        assert_eq!(decoded.document, Ok(Document::Protection(protection(signed))));
        let decoded = fixtures::decode(client::decoder(&request(true), &limits).unwrap(), &listed(body), &limits);
        assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
        assert_eq!(decoded.rows.as_slice(), &[Row::Protection(protection(signed))]);
    }
}

/// The true case is an unrecorded Forgejo fact; the transcripts carry false.
#[test]
fn protection_reads_the_signed_commits_rule() {
    for signed in [false, true] {
        assert_eq!(
            reading::protection(protection(signed)),
            api::Protection {
                branch: Box::from(*b"main"),
                contexts: Box::from([Box::from(b"test".as_slice())]),
                approvals: 1,
                dismiss_stale: !signed,
                queue: false,
                signatures: signed,
                linear: false,
                code_owners: false,
                bypass: api::Bypass::Never,
            }
        );
    }
}

#[test]
fn protection_requires_the_signed_commits_field_in_single_answers_and_lists() {
    let limits = fixtures::limits();
    for list in [false, true] {
        let body = if list { listed(MISSING) } else { Box::from(MISSING) };
        let decoded = fixtures::decode(client::decoder(&request(list), &limits).unwrap(), &body, &limits);
        assert_eq!(decoded.document, Err(Malformed::Missing { field: b"require_signed_commits" }));
    }
}

#[test]
fn protection_call_refuses_a_missing_signed_commits_field_as_invalid() {
    let mut run = fixtures::Run::known(Op::Read(Read::Protection { branch: Box::from(*b"main") }));
    run.begin();
    run.answer(200, br#"{"name":"main","commit":{"id":"0300000000000000000000000000000000000000"},"effective_branch_protection_name":"main"}"#);
    assert_eq!(run.answer(200, MISSING), Step::Done(Err(Error::InvalidAnswer)));
}

#[test]
fn protection_encoder_round_trips_both_signed_commits_values() {
    let limits = fixtures::limits();
    for signed in [false, true] {
        for list in [false, true] {
            let value = protection(signed);
            let answer = if list {
                server::Answer::Protections(Box::from([value.clone()]))
            } else {
                server::Answer::Protection(value.clone())
            };
            let body = server::encode(&answer, &limits).unwrap();
            let decoded = fixtures::decode(client::decoder(&request(list), &limits).unwrap(), &body, &limits);
            if list {
                assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
                assert_eq!(decoded.rows.as_slice(), &[Row::Protection(value)]);
            } else {
                assert_eq!(decoded.document, Ok(Document::Protection(value)));
            }
        }
    }
}

#[test]
fn protection_call_reads_both_signed_commits_values() {
    for (body, signed) in [(UNSIGNED, false), (SIGNED, true)] {
        let mut run = fixtures::Run::known(Op::Read(Read::Protection { branch: Box::from(*b"main") }));
        run.begin();
        run.answer(200, br#"{"name":"main","commit":{"id":"0300000000000000000000000000000000000000"},"effective_branch_protection_name":"main"}"#);
        assert_eq!(
            run.answer(200, body),
            Step::Done(Ok(Answer::Protection(Some(reading::protection(protection(signed))))))
        );
    }
}
