//! GitHub's protection documents, containing only fields the client keeps (5.3).
use super::{document, output_limits};
use crate::api::Bypass;
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};
/// Writes one page of applicable rules, each with its ruleset id.
pub fn rules(rows: &[document::Rule], out: &mut Writer) -> Result<(), Overflow> {
    write(Output::Rules(rows), out)
}
/// Writes the installation App's own bypass capability.
pub fn ruleset(bypass: Bypass, out: &mut Writer) -> Result<(), Overflow> {
    write(Output::Ruleset(bypass), out)
}
/// Writes classic protection; queue and bypass have no classic fields.
pub fn classic_protection(value: &document::Protection, out: &mut Writer) -> Result<(), Overflow> {
    write(Output::Classic(value), out)
}
enum Output<'a> {
    Rules(&'a [document::Rule]),
    Ruleset(Bypass),
    Classic(&'a document::Protection),
}
fn write(value: Output<'_>, out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    encode(&value, &mut measure);
    let length = measure.measured().or(Err(Overflow))?;
    let mut json = Encoder::write(length, &limits);
    encode(&value, &mut json);
    out.put(&json.finish())
}
fn reviews(approvals: u32, dismiss_stale: bool, code_owners: bool, classic: bool, json: &mut Encoder) {
    json.key(b"required_approving_review_count");
    json.unsigned(u64::from(approvals));
    json.key(if classic { b"dismiss_stale_reviews" } else { b"dismiss_stale_reviews_on_push" });
    json.boolean(dismiss_stale);
    json.key(if classic { b"require_code_owner_reviews" } else { b"require_code_owner_review" });
    json.boolean(code_owners);
}
fn enabled(key: &[u8], value: bool, json: &mut Encoder) {
    json.key(key);
    json.object_start();
    json.key(b"enabled");
    json.boolean(value);
    json.object_end();
}
fn encode(value: &Output<'_>, json: &mut Encoder) {
    match value {
        Output::Rules(rows) => {
            json.array_start();
            for row in *rows {
                json.object_start();
                json.key(b"ruleset_id");
                json.unsigned(row.ruleset);
                json.key(b"type");
                match &row.kind {
                    document::RuleKind::PullRequest { approvals, dismiss_stale, code_owners } => {
                        json.string(b"pull_request");
                        json.key(b"parameters");
                        json.object_start();
                        reviews(*approvals, *dismiss_stale, *code_owners, false, json);
                        json.object_end();
                    }
                    document::RuleKind::StatusChecks(contexts) => {
                        json.string(b"required_status_checks");
                        json.key(b"parameters");
                        json.object_start();
                        json.key(b"required_status_checks");
                        json.array_start();
                        for context in contexts {
                            json.object_start();
                            json.key(b"context");
                            json.string(context);
                            json.object_end();
                        }
                        json.array_end();
                        json.object_end();
                    }
                    document::RuleKind::Queue => json.string(b"merge_queue"),
                    document::RuleKind::Signatures => json.string(b"required_signatures"),
                    document::RuleKind::Linear => json.string(b"required_linear_history"),
                    document::RuleKind::Skipped => json.string(b"creation"),
                }
                json.object_end();
            }
            json.array_end();
        }
        Output::Ruleset(bypass) => {
            json.object_start();
            json.key(b"current_user_can_bypass");
            json.string(match bypass {
                Bypass::Never => b"never",
                Bypass::PullsOnly => b"pull_requests_only",
                Bypass::Always => b"always",
            });
            json.object_end();
        }
        Output::Classic(value) => {
            json.object_start();
            json.key(b"required_status_checks");
            json.object_start();
            json.key(b"contexts");
            json.array_start();
            for context in &value.contexts {
                json.string(context);
            }
            json.array_end();
            json.object_end();
            json.key(b"required_pull_request_reviews");
            json.object_start();
            reviews(value.approvals, value.dismiss_stale, value.code_owners, true, json);
            json.object_end();
            enabled(b"required_signatures", value.signatures, json);
            enabled(b"required_linear_history", value.linear, json);
            json.object_end();
        }
    }
}
