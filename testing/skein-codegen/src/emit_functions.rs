#![expect(clippy::format_push_string, reason = "generator assembles reviewed source fragments")]

//! Split generated functions at codec.md, section 6's line bound.

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
    let name = function
        .first()
        .expect("signature")
        .trim_start()
        .strip_prefix("pub fn ")
        .expect("public function")
        .split('(')
        .next()
        .expect("name");
    let start = function.iter().position(|line| line.trim_start().starts_with("let mut ")).expect("accumulator");
    let variable = function
        .get(start)
        .expect("declaration")
        .trim_start()
        .strip_prefix("let mut ")
        .expect("mutable accumulator")
        .split(' ')
        .next()
        .expect("name");
    let end =
        function.iter().rposition(|line| line.trim_start().starts_with(&format!("{variable} ="))).expect("statements");
    let body_start = start.checked_add(1).expect("body start");
    let tail_start = end.checked_add(1).expect("tail start");
    let statements = function.get(body_start..tail_start).expect("statements");
    let padding = " ".repeat(indent);
    let scope = if indent == 0 { "" } else { "Self::" };
    out.push_str(&format!("{}\n", function.get(..body_start).expect("header").join("\n")));
    for (index, _) in statements.chunks(90).enumerate() {
        out.push_str(&format!("{padding}    {variable} = {scope}{name}_{index}(limits, {variable})?;\n"));
    }
    out.push_str(&format!("{}\n", function.get(tail_start..).expect("tail").join("\n")));
    for (index, chunk) in statements.chunks(90).enumerate() {
        out.push_str(&format!("\n{padding}fn {name}_{index}(limits: &Limits, mut {variable}: u64) -> Option<u64> {{\n{}\n{padding}    Some({variable})\n{padding}}}\n", chunk.join("\n")));
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

/// Keeps each field's operations together when a record method exceeds the bound.
pub(crate) fn split_record_functions(record: &crate::Record, source: &str) -> String {
    let mut out = String::new();
    let mut helpers = String::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(&format!("{line}\n"));
        let method = line.trim_start().strip_prefix("pub fn ").or_else(|| line.trim_start().strip_prefix("fn "));
        let name = method.and_then(|method| method.split('(').next()).unwrap_or("");
        if !line.ends_with('{') || !matches!(name, "check" | "measure" | "encode" | "decode_from") {
            continue;
        }
        let mut body = lines.by_ref().take_while(|line| *line != "    }").collect::<Vec<_>>();
        if body.len() < 99 {
            for line in body {
                out.push_str(&format!("{line}\n"));
            }
            out.push_str("    }\n");
            continue;
        }
        let mut tail = body.pop().expect("method result").to_owned();
        let (parameters, arguments, result, finish, call) = match name {
            "measure" => ("&self, mut size: u32", "self, size", "u32", "size", "size = "),
            "encode" => (
                "&self, writer: &mut skein_lib::Writer",
                "self, writer",
                "Result<(), skein_lib::Overflow>",
                "Ok(())",
                "",
            ),
            "check" => ("&self, limits: &Limits", "self, limits", "Result<(), Problem>", "Ok(())", ""),
            "decode_from" => ("limits: &Limits, reader: &mut skein_lib::Reader<'_>", "limits, reader", "", "", ""),
            _ => unreachable!("record method selected"),
        };
        if name == "measure" || body.first().is_some_and(|line| line.trim_start().starts_with("let version =")) {
            out.push_str(&format!("{}\n", body.remove(0)));
            if name == "decode_from" {
                out.push_str(&format!("{}\n", body.remove(0)));
            }
        }
        let mut chunks = vec![String::new()];
        for line in body {
            let chunk = chunks.last().expect("chunk");
            let boundary = chunk.matches('{').count() == chunk.matches('}').count()
                && (!matches!(name, "measure" | "encode") || line.trim_start().starts_with("let field_"));
            if boundary && chunk.lines().count() >= 70 {
                chunks.push(String::new());
            }
            chunks.last_mut().expect("chunk").push_str(&format!("{line}\n"));
        }
        for (index, chunk) in chunks.iter().enumerate() {
            let (parameters, arguments) = if name == "check" && !chunk.contains("self.") && !chunk.contains(" = self;")
            {
                ("limits: &Limits", "limits")
            } else {
                (parameters, arguments)
            };
            let fragment = format!("{}Decoded{index}", record.name);
            let mut result = result.to_owned();
            let mut finish = finish.to_owned();
            let mut call = call.to_owned();
            if name == "decode_from" {
                let fields = record
                    .fields
                    .iter()
                    .filter(|field| chunk.contains(&format!("let decoded_{} =", field.name)))
                    .collect::<Vec<_>>();
                let members = fields
                    .iter()
                    .map(|field| format!("{}: decoded_{}", field.name, field.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                for field in &fields {
                    for suffix in [",", " }", "]"] {
                        tail = tail.replace(
                            &format!("decoded_{}{suffix}", field.name),
                            &format!("decoded_{index}.{}{suffix}", field.name),
                        );
                    }
                }
                helpers.push_str(&format!("\n#[derive(Debug)]\nstruct {fragment} {{\n"));
                for field in &fields {
                    helpers.push_str(&format!("    {}: {},\n", field.name, crate::emit_types::rust_type(&field.ty)));
                }
                helpers.push_str("}\n");
                result = format!("Result<{fragment}, Problem>");
                finish = format!("Ok({fragment} {{ {members} }})");
                call = format!("let decoded_{index} = ");
            }
            let question = if name == "measure" { "" } else { "?" };
            out.push_str(&format!("        {call}Self::{name}_{index}({arguments}){question};\n"));
            helpers.push_str(&format!("\nimpl {} {{\n    fn {name}_{index}({parameters}) -> {result} {{\n{chunk}        {finish}\n    }}\n}}\n", record.name));
        }
        out.push_str(&format!("{tail}\n    }}\n"));
    }
    out.push_str(&helpers);
    out
}
