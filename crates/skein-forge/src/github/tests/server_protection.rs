use super::fixtures::{bytes, decode, limits};
use crate::{
    api::Bypass,
    github::{
        document::{self, Document, Kind, Row, Rule, RuleKind},
        server,
    },
};
use alloc::boxed::Box;
use skein_lib::Writer;
#[test]
fn each_server_protection_document_round_trips_through_its_client_decoder() {
    let rules = [
        Rule { ruleset: 17, kind: RuleKind::PullRequest { approvals: 3, dismiss_stale: true, code_owners: true } },
        Rule {
            ruleset: 17,
            kind: RuleKind::StatusChecks(Box::from([Box::from(b"build".as_slice()), Box::from(b"lint".as_slice())])),
        },
        Rule { ruleset: 17, kind: RuleKind::Queue },
        Rule { ruleset: 17, kind: RuleKind::Linear },
        Rule { ruleset: 17, kind: RuleKind::Signatures },
        Rule { ruleset: 99, kind: RuleKind::Skipped },
    ];
    for chunk in [1, 7] {
        let mut out = Writer::new(4096);
        server::rules(&rules, &mut out).unwrap();
        let wire = bytes(out);
        let (rows, done) = decode(Kind::RuleRows, &wire, chunk, &limits());
        assert_eq!(done, Ok(Document::Rows));
        assert_eq!(rows.into_boxed().as_ref(), &rules.clone().map(Row::Rule));
        for bypass in [Bypass::Never, Bypass::PullsOnly, Bypass::Always] {
            let mut out = Writer::new(256);
            server::ruleset(bypass, &mut out).unwrap();
            let (rows, done) = decode(Kind::Ruleset, &bytes(out), chunk, &limits());
            assert_eq!(rows.len(), 0);
            assert_eq!(done, Ok(Document::Ruleset(bypass)));
        }
        for enabled in [false, true] {
            let value = document::Protection {
                contexts: Box::from([Box::from(b"build".as_slice())]),
                approvals: 2,
                dismiss_stale: enabled,
                code_owners: enabled,
                signatures: enabled,
                linear: enabled,
                queue: false,
            };
            let mut out = Writer::new(2048);
            server::classic_protection(&value, &mut out).unwrap();
            let (rows, done) = decode(Kind::ClassicProtection, &bytes(out), chunk, &limits());
            assert_eq!(rows.len(), 0);
            assert_eq!(done, Ok(Document::ClassicProtection(Some(value))));
        }
    }
    assert!(server::rules(&rules, &mut Writer::new(1)).is_err());
    assert!(server::ruleset(Bypass::Never, &mut Writer::new(1)).is_err());
    assert!(server::classic_protection(&document::Protection::empty(), &mut Writer::new(1)).is_err());
}
