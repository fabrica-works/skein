//! Bounded source checks for the forge client's contract fixtures.
use std::{collections::BTreeMap, env, fs, path::PathBuf, process};

const FILES: [&str; 4] = [
    "crates/skein-forge-client/src/tests/contracts.rs",
    "crates/skein-forge-client/src/tests/fixtures.rs",
    "crates/skein-forge-client/src/tests/fixtures/keep.rs",
    "crates/skein-forge-client/src/tests/fixtures/outbox.rs",
];
const SOURCE_LIMIT: usize = 256 * 1024; // four files, each below the plan's 800-line ceiling

#[derive(Debug)]
struct Token {
    text: String,
    line: usize,
}

// Strings remain opaque tokens, so fixture data distinguishes otherwise equal
// setups. Comments and whitespace cannot disguise a repeated setup.
fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let (mut at, mut line) = (0, 1);
    while at < bytes.len() {
        let start = at;
        let start_line = line;
        if bytes[at].is_ascii_whitespace() {
            line += usize::from(bytes[at] == b'\n');
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1;
            while at < bytes.len() && depth > 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    line += usize::from(bytes[at] == b'\n');
                    at += 1;
                }
            }
            continue;
        }
        // Rust raw strings, including byte strings, may contain quotes and
        // comment markers without contributing executable tokens.
        let mut quote = at + usize::from(bytes[at] == b'b');
        if bytes.get(quote) == Some(&b'r') {
            quote += 1;
            let hash_start = quote;
            while bytes.get(quote) == Some(&b'#') {
                quote += 1;
            }
            if bytes.get(quote) == Some(&b'"') {
                let hashes = quote - hash_start;
                at = quote + 1;
                while at < bytes.len() {
                    if bytes[at] == b'"' && bytes.get(at + 1..at + 1 + hashes) == Some(&bytes[hash_start..quote]) {
                        at += 1 + hashes;
                        break;
                    }
                    line += usize::from(bytes[at] == b'\n');
                    at += 1;
                }
            }
        }
        if at == start {
            quote = at + usize::from(bytes[at] == b'b');
            let quoted = bytes.get(quote) == Some(&b'"')
                || (bytes.get(quote) == Some(&b'\'')
                    && (bytes.get(quote + 2) == Some(&b'\'') || bytes.get(quote + 1) == Some(&b'\\')));
            if quoted {
                let delimiter = bytes[quote];
                at = quote + 1;
                while at < bytes.len() {
                    let byte = bytes[at];
                    line += usize::from(byte == b'\n');
                    at += 1;
                    if byte == b'\\' {
                        at = (at + 1).min(bytes.len());
                    } else if byte == delimiter {
                        break;
                    }
                }
            } else if bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' {
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
            } else {
                // Consume a whole UTF-8 character for a valid slice boundary.
                at += source[at..].chars().next().expect("character exists").len_utf8();
            }
        }
        result.push(Token { text: source[start..at].into(), line: start_line });
    }
    result
}

fn literal(token: &Token) -> bool {
    token.text.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

fn diagnostics(source: &str, setups: &mut BTreeMap<Vec<String>, String>, path: &str) -> Vec<String> {
    let ts = tokens(source);
    let mut errors = Vec::new();
    for (at, token) in ts.iter().enumerate() {
        if ["with_capacity", "zeroed", "resize", "truncate", "take", "repeat", "split_at"]
            .contains(&token.text.as_str())
            && ts.get(at + 1).is_some_and(|next| next.text == "(")
        {
            let mut depth = 1;
            let mut cursor = at + 2;
            while let Some(arg) = ts.get(cursor) {
                if arg.text == "(" {
                    depth += 1;
                } else if arg.text == ")" {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                if arg.text == "," && depth == 1 {
                    break;
                }
                if literal(arg) {
                    errors.push(format!(
                        "{path}:{}: Named fixtures and sizes: name the capacity/length in {}",
                        arg.line, token.text
                    ));
                }
                cursor += 1;
            }
        }
        // Find `let mut name = ...current();`, then the three assignments
        // head/base/reviewers. Canonicalize only the local receiver, retaining
        // each field's data, so a different pull fixture is not a duplicate.
        if token.text != "let" || ts.get(at + 1).is_none_or(|next| next.text != "mut") {
            continue;
        }
        let Some(name) = ts.get(at + 2) else { continue };
        let rest = &ts[at + 3..];
        let Some(end) = rest.iter().position(|next| next.text == ";") else { continue };
        let init = &rest[..end];
        if init.first().is_none_or(|next| next.text != "=")
            || !init.windows(3).any(|w| w[0].text == "current" && w[1].text == "(" && w[2].text == ")")
        {
            continue;
        }
        let mut tail = &rest[end + 1..];
        let mut key = Vec::new();
        let mut fields = Vec::new();
        for _ in 0..3 {
            let Some(end) = tail.iter().position(|next| next.text == ";") else { break };
            let statement = &tail[..end];
            if statement.len() < 4
                || statement[0].text != name.text
                || statement[1].text != "."
                || statement[3].text != "="
            {
                break;
            }
            fields.push(statement[2].text.as_str());
            key.extend(statement[1..].iter().map(|next| next.text.clone()));
            key.push(";".into());
            tail = &tail[end + 1..];
        }
        if fields == ["head", "base", "reviewers"] {
            let location = format!("{path}:{}", token.line);
            if let Some(first) = setups.insert(key, location.clone()) {
                errors.push(format!(
                    "{location}: A setup written once: share reviewed-pull setup first found at {first}"
                ));
            }
        }
    }
    errors
}

fn main() {
    let args: Vec<_> = env::args_os().skip(1).map(PathBuf::from).collect();
    let paths = if args.is_empty() { FILES.iter().map(PathBuf::from).collect() } else { args };
    let mut setups = BTreeMap::new();
    let mut failed = false;
    for path in paths {
        let source = fs::read_to_string(&path).expect("read scoped contract source");
        assert!(source.len() <= SOURCE_LIMIT, "contract source exceeds bounded checker input");
        for error in diagnostics(&source, &mut setups, &path.display().to_string()) {
            eprintln!("{error}");
            failed = true;
        }
    }
    if failed {
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::diagnostics;
    use std::collections::BTreeMap;

    const ORIGINAL: &str = r#"
        let mut pull = super::fixtures::keep::current();
        pull.head = Box::from(&b"kit"[..]);
        pull.base = Box::from(&b"kit"[..]);
        pull.reviewers = Box::new([7, 8]);
    "#;

    #[test]
    fn duplicate_setup_survives_local_renaming_and_comments() {
        let renamed = ORIGINAL.replace("pull", "other").replace(".", " /* gap */ . ");
        let source = format!("fn first() {{ {ORIGINAL} }} fn second() {{ {renamed} }}");
        let errors = diagnostics(&source, &mut BTreeMap::new(), "control.rs");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("A setup written once"));
    }

    #[test]
    fn different_pull_data_and_shared_setup_are_accepted() {
        let different = ORIGINAL.replace("[7, 8]", "[8, 9]");
        let source = format!(
            "{ORIGINAL} {different} reviewed_pull(REVIEWERS); reviewed_pull(REVIEWERS); let mut unrelated = current(); unrelated.merged = None;"
        );
        assert!(diagnostics(&source, &mut BTreeMap::new(), "control.rs").is_empty());
    }

    #[test]
    fn bare_capacity_and_length_arguments_are_refused() {
        for source in ["List::with_capacity(4)", "zeroed(5)", "values.truncate(3)", "List::with_capacity(ROOM + 1)"] {
            let errors = diagnostics(source, &mut BTreeMap::new(), "control.rs");
            assert_eq!(errors.len(), 1, "{source}");
            assert!(errors[0].contains("Named fixtures and sizes"));
        }
    }

    #[test]
    fn named_sizes_and_opaque_source_are_accepted() {
        let source = r####"
            List::with_capacity(ROOM); zeroed(NAMESPACE_BYTES);
            // List::with_capacity(4);
            /* outer /* zeroed(5) */ List::with_capacity(6) */
            "List::with_capacity(7)"; b"zeroed(8)";
            r###"with_capacity(9) \" /* comment */"###;
            br#"zeroed(10)"#;
        "####;
        assert!(diagnostics(source, &mut BTreeMap::new(), "control.rs").is_empty());
    }
}
