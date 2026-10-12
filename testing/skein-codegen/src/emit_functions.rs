#![expect(clippy::format_push_string, reason = "generator assembles reviewed source fragments")]

//! Split generated worst-case accumulations at codec.md, section 6's line bound.

pub(crate) fn split_worst_functions(out: &mut String) {
    let source = core::mem::take(out);
    let lines = source.lines().collect::<Vec<_>>();
    let mut offset = 0;
    let mut limit_checks = Vec::new();
    while let Some(line) = lines.get(offset) {
        let worst = line.trim_start().starts_with("pub fn worst_case_");
        let check = line.trim_start().starts_with("fn check(");
        if !worst && !check {
            out.push_str(line);
            out.push('\n');
            offset = offset.checked_add(1).expect("source offset");
            continue;
        }
        let indent = line.len().checked_sub(line.trim_start().len()).expect("indent length");
        let closing = format!("{}}}", " ".repeat(indent));
        let count = lines.iter().skip(offset).position(|candidate| *candidate == closing).expect("function closes");
        let end = offset.checked_add(count).expect("function end");
        let function = lines.get(offset..=end).expect("function range");
        if function.len() > 100 && worst {
            split(function, indent, out);
        } else if function.len() > 100 && check {
            split_checks(function, &mut limit_checks, out);
        } else {
            for line in function {
                out.push_str(line);
                out.push('\n');
            }
        }
        offset = end.checked_add(1).expect("source offset");
    }
    for (index, chunk) in limit_checks.chunks(90).enumerate() {
        out.push_str(&format!("\nfn check_limits_{index}(limits: &Limits) -> Result<(), Problem> {{\n"));
        for line in chunk {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("    Ok(())\n}\n");
    }
}

fn split(function: &[&str], indent: usize, out: &mut String) {
    let signature = function.first().expect("function signature");
    let name = signature
        .trim_start()
        .strip_prefix("pub fn ")
        .expect("public function")
        .split('(')
        .next()
        .expect("function name");
    let start = function.iter().position(|line| line.trim_start().starts_with("let mut ")).expect("accumulator");
    let variable = function
        .get(start)
        .expect("accumulator declaration")
        .trim_start()
        .strip_prefix("let mut ")
        .expect("mutable accumulator")
        .split(' ')
        .next()
        .expect("accumulator name");
    let end = function
        .iter()
        .rposition(|line| line.trim_start().starts_with(&format!("{variable} =")))
        .expect("accumulation statements");
    for line in function.iter().take(start.checked_add(1).expect("declaration end")) {
        out.push_str(line);
        out.push('\n');
    }
    let statements = function.get(start.checked_add(1).expect("statements start")..=end).expect("statements");
    let padding = " ".repeat(indent);
    let scope = if indent == 0 { "" } else { "Self::" };
    for (index, _) in statements.chunks(90).enumerate() {
        out.push_str(&format!("{padding}    {variable} = {scope}{name}_{index}(limits, {variable})?;\n"));
    }
    for line in function.iter().skip(end.checked_add(1).expect("statements end")) {
        out.push_str(line);
        out.push('\n');
    }
    for (index, chunk) in statements.chunks(90).enumerate() {
        out.push_str(&format!(
            "\n{padding}fn {name}_{index}(limits: &Limits, mut {variable}: u64) -> Option<u64> {{\n"
        ));
        for line in chunk {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("{padding}    Some({variable})\n{padding}}}\n"));
    }
}

fn split_checks(function: &[&str], checks: &mut Vec<String>, out: &mut String) {
    let mut emitted = false;
    for line in function {
        if line.trim_start().starts_with("if limits.") {
            if checks.is_empty() {
                checks.extend(
                    function
                        .iter()
                        .filter(|line| line.trim_start().starts_with("if limits."))
                        .map(|line| format!("    {}", line.trim_start())),
                );
            }
            if !emitted {
                for (index, _) in checks.chunks(90).enumerate() {
                    out.push_str(&format!("        check_limits_{index}(limits)?;\n"));
                }
                emitted = true;
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
}
