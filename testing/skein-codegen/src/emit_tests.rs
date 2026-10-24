#![expect(clippy::format_push_string, reason = "generator assembles reviewed source fragments")]

//! Fixed-rule golden values and generated round trips (codec.md, section 6).

use crate::{Declaration, Enumeration, Golden, Record, Type, Variant, analysis::Context, emit_limits, emit_types};

fn pattern(bound: u32, text: bool) -> Vec<u8> {
    let source: &[u8] = if text { b"abc" } else { &[0, 0x7f, 0xff] };
    source.iter().copied().take(usize::try_from(bound).expect("u32 fits usize")).collect()
}

fn write_record(record: &Record, full: bool, schema: &Context<'_>, out: &mut Vec<u8>) {
    if record.versioned {
        out.extend_from_slice(&schema.version.to_be_bytes());
    }
    for field in &record.fields {
        write_type(&field.ty, full, schema, out);
    }
}

fn write_variant(enumeration: &Enumeration, variant: &Variant, full: bool, schema: &Context<'_>, out: &mut Vec<u8>) {
    let index =
        enumeration.variants.iter().position(|candidate| candidate.name == variant.name).expect("variant in enum");
    out.push(u8::try_from(index).expect("at most 256 variants"));
    if let Some(record) = &variant.record {
        match schema.declaration(record) {
            Declaration::Record(record) => write_record(record, full, schema, out),
            Declaration::Enum(_) => unreachable!("a variant only holds a record"),
        }
    }
}

fn write_type(ty: &Type, full: bool, schema: &Context<'_>, out: &mut Vec<u8>) {
    match ty {
        Type::U8 => out.push(if full { u8::MAX } else { 0 }),
        Type::U16 => out.extend_from_slice(&(if full { u16::MAX } else { 0 }).to_be_bytes()),
        Type::U32 => out.extend_from_slice(&(if full { u32::MAX } else { 0 }).to_be_bytes()),
        Type::U64 | Type::Duration => out.extend_from_slice(&(if full { u64::MAX } else { 0 }).to_be_bytes()),
        Type::Bool => out.push(u8::from(full)),
        Type::Fixed(bound) => {
            for _ in 0_u32..*bound {
                out.push(if full { 0xa5 } else { 0 });
            }
        }
        Type::Bytes(bound) | Type::Secret(bound) | Type::Text(bound) => {
            let bytes = if full { pattern(*bound, matches!(ty, Type::Text(_))) } else { Vec::new() };
            out.extend_from_slice(&u32::try_from(bytes.len()).expect("pattern fits u32").to_be_bytes());
            out.extend_from_slice(&bytes);
        }
        Type::List(bound, item) => {
            let count = u32::from(full && *bound > 0);
            out.extend_from_slice(&count.to_be_bytes());
            if count > 0 {
                write_type(item, true, schema, out);
            }
        }
        Type::Option(item) => {
            out.push(u8::from(full));
            if full {
                write_type(item, true, schema, out);
            }
        }
        Type::Named(name) => match schema.declaration(name) {
            Declaration::Record(record) => write_record(record, full, schema, out),
            Declaration::Enum(enumeration) => {
                let variant = enumeration.variants.first().expect("enum nonempty");
                write_variant(enumeration, variant, full, schema, out);
            }
        },
    }
}

fn bytes_literal(bytes: &[u8]) -> String {
    let elements = bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(", ");
    format!("&[{elements}]")
}

fn value_expr(ty: &Type, full: bool, schema: &Context<'_>) -> String {
    match ty {
        Type::U8 => if full { "u8::MAX" } else { "0_u8" }.into(),
        Type::U16 => if full { "u16::MAX" } else { "0_u16" }.into(),
        Type::U32 => if full { "u32::MAX" } else { "0_u32" }.into(),
        Type::U64 => if full { "u64::MAX" } else { "0_u64" }.into(),
        Type::Bool => full.to_string(),
        Type::Duration => format!("skein_lib::Duration::from_nanos({})", if full { "u64::MAX" } else { "0_u64" }),
        Type::Fixed(bound) => format!("[{}_u8; {}]", if full { 0xa5_u8 } else { 0_u8 }, emit_limits::literal(*bound)),
        Type::Secret(bound) => format!("skein_lib::Secret::new({})", value_expr(&Type::Bytes(*bound), full, schema)),
        Type::Bytes(bound) | Type::Text(bound) => {
            let bytes = if full { pattern(*bound, matches!(ty, Type::Text(_))) } else { Vec::new() };
            let elements = bytes.iter().map(|byte| format!("{byte}_u8")).collect::<Vec<_>>().join(", ");
            format!("Box::from([{elements}].as_slice())")
        }
        Type::List(bound, item) => {
            if full && *bound > 0 {
                format!(
                    "{{ let mut items = skein_lib::List::with_capacity(1); items.push({}).expect(\"one slot\"); items }}",
                    value_expr(item, true, schema)
                )
            } else {
                "skein_lib::List::with_capacity(0)".into()
            }
        }
        Type::Option(item) => {
            if full {
                format!("Some({})", value_expr(item, true, schema))
            } else {
                "None".into()
            }
        }
        Type::Named(name) => match schema.declaration(name) {
            Declaration::Record(record) => record_expr(record, full, schema),
            Declaration::Enum(enumeration) => {
                let variant = enumeration.variants.first().expect("enum nonempty");
                variant_expr(enumeration, variant, full, schema)
            }
        },
    }
}

fn record_expr(record: &Record, full: bool, schema: &Context<'_>) -> String {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| !emit_types::grouped_bools(record) || !matches!(field.ty, Type::Bool))
        .map(|field| format!("{}: {}", field.name, value_expr(&field.ty, full, schema)))
        .collect::<Vec<_>>();
    if emit_types::grouped_bools(record) {
        let bools = record
            .fields
            .iter()
            .filter(|field| matches!(field.ty, Type::Bool))
            .map(|field| value_expr(&field.ty, full, schema))
            .collect::<Vec<_>>()
            .join(", ");
        fields.insert(0, format!("skein_bools: [{bools}]"));
    }
    let fields = fields.join(", ");
    let call = format!("{}::new(&CEILINGS, {}Parts {{ {fields} }})", record.name, record.name);
    if schema.bounds.is_empty() { call } else { format!("{call}.expect(\"golden within ceilings\")") }
}

fn variant_expr(enumeration: &Enumeration, variant: &Variant, full: bool, schema: &Context<'_>) -> String {
    let name = emit_limits::pascal(&variant.name);
    match &variant.record {
        Some(record) => match schema.declaration(record) {
            Declaration::Record(record) => {
                let value = record_expr(record, full, schema);
                let value = if variant.boxed { format!("Box::new({value})") } else { value };
                format!("{}::{name}({value})", enumeration.name)
            }
            Declaration::Enum(_) => unreachable!("a variant only holds a record"),
        },
        None => format!("{}::{name}", enumeration.name),
    }
}

fn golden_case(name: &str, ty: &str, value: &str, bytes: &[u8]) -> String {
    let bytes = bytes_literal(bytes);
    format!(
        "        (\"{name}\", || {{ let value = {value}; let bytes: &[u8] = {bytes}; let mut writer = skein_lib::Writer::new(usize::try_from(value.measure()).expect(\"size fits usize\")); let encoded = match value.encode(&mut writer) {{ Ok(()) => Ok(writer.finish()), Err(problem) => Err(problem) }}; golden(value, bytes, encoded, {ty}::decode(&CEILINGS, &mut skein_lib::Reader::new(bytes))) }}),\n"
    )
}

fn write_bound_type(ty: &Type, name: &str, target: &str, count: u32, schema: &Context<'_>, out: &mut Vec<u8>) {
    if name == target {
        out.extend_from_slice(&count.to_be_bytes());
        for _ in 0_u32..count {
            match ty {
                Type::List(_, item) => write_type(item, false, schema, out),
                Type::Bytes(_) | Type::Secret(_) | Type::Text(_) => out.push(0),
                Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::Bool
                | Type::Duration
                | Type::Fixed(_)
                | Type::Option(_)
                | Type::Named(_) => unreachable!("adjustable bound"),
            }
        }
    } else {
        match ty {
            Type::Option(item) => {
                out.push(1);
                write_bound_type(item, &format!("{name}_some"), target, count, schema, out);
            }
            Type::List(_, item) => {
                out.extend_from_slice(&1_u32.to_be_bytes());
                write_bound_type(item, &format!("{name}_item"), target, count, schema, out);
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Bytes(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Named(_) => unreachable!("ancestor of bound"),
        }
    }
}

fn bound_wire(record: &Record, field_index: usize, target: &str, count: u32, schema: &Context<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    if record.versioned {
        bytes.extend_from_slice(&schema.version.to_be_bytes());
    }
    for (index, field) in record.fields.iter().enumerate() {
        if index == field_index {
            let name = format!("{}_{}", emit_limits::snake(&record.name), field.name);
            write_bound_type(&field.ty, &name, target, count, schema, &mut bytes);
        } else {
            write_type(&field.ty, false, schema, &mut bytes);
        }
    }
    bytes
}

fn field_bounds(ty: &Type, name: &str, bounds: &mut Vec<(String, u32)>) {
    match ty {
        Type::Bytes(bound) | Type::Secret(bound) | Type::Text(bound) => bounds.push((name.into(), *bound)),
        Type::List(bound, item) => {
            bounds.push((name.into(), *bound));
            if *bound > 0 {
                field_bounds(item, &format!("{name}_item"), bounds);
            }
        }
        Type::Option(item) => field_bounds(item, &format!("{name}_some"), bounds),
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Bool
        | Type::Duration
        | Type::Fixed(_)
        | Type::Named(_) => {}
    }
}

fn bound_cases(record: &Record, schema: &Context<'_>, cases: &mut Vec<String>) {
    for (index, field) in record.fields.iter().enumerate() {
        let name = format!("{}_{}", emit_limits::snake(&record.name), field.name);
        let path = format!("{}{}", record.name, emit_limits::pascal(&field.name));
        let mut bounds = Vec::new();
        field_bounds(&field.ty, &name, &mut bounds);
        for (target, ceiling) in bounds {
            let limit = ceiling.min(3);
            let at = bytes_literal(&bound_wire(record, index, &target, limit, schema));
            let past =
                bytes_literal(&bound_wire(record, index, &target, limit.checked_add(1).expect("small limit"), schema));
            cases.push(format!("        (\"bound_{target}\", || bound((Path::{path}, skein_codec::Reason::Bound), &Limits {{ {target}: {limit}, ..CEILINGS }}, {at}, {past}, |limits, bytes| match {record}::decode(limits, &mut skein_lib::Reader::new(bytes)) {{ Ok(value) => Ok(value), Err(problem) => Err((problem.path, problem.reason)) }})),\n", record = record.name));
        }
    }
}

fn tables(name: &str, cases: &[String], out: &mut String) {
    let item = name.to_uppercase();
    for (index, chunk) in cases.chunks(if name == "golden" { 1_024 } else { 64 }).enumerate() {
        out.push_str(&format!("    static {item}_{index}: [Case; {}] = [\n", chunk.len()));
        for case in chunk {
            out.push_str(case);
        }
        out.push_str(&format!(
            "    ];\n\n    #[test]\n    fn {name}_{index}() {{\n        table(&{item}_{index});\n    }}\n\n"
        ));
    }
}

fn golden_cases(schema: &Context<'_>, cases: &mut Vec<String>) -> Vec<Golden> {
    named_goldens(schema, cases).into_iter().map(|(_, golden)| golden).collect()
}

/// Golden values paired with their schema declaration, without parsing filenames.
pub(crate) fn goldens(schema: &crate::Schema) -> Vec<(skein_codec::schema::DeclIx, Golden)> {
    named_goldens(&Context::new(schema), &mut Vec::new())
}

fn named_goldens(schema: &Context<'_>, cases: &mut Vec<String>) -> Vec<(skein_codec::schema::DeclIx, Golden)> {
    let mut goldens = Vec::new();
    for (index, declaration) in schema.declarations.iter().enumerate() {
        let root = skein_codec::schema::DeclIx::new(u32::try_from(index).expect("schema declaration index"));
        match declaration {
            Declaration::Record(record) => {
                for (label, full) in [("smallest", false), ("full", true)] {
                    let mut bytes = Vec::new();
                    write_record(record, full, schema, &mut bytes);
                    let name = format!("record_{}_{}", emit_limits::snake(&record.name), label);
                    cases.push(golden_case(&name, &record.name, &record_expr(record, full, schema), &bytes));
                    goldens.push((root, Golden { name: format!("{name}.bin"), bytes }));
                }
            }
            Declaration::Enum(enumeration) => {
                for variant in &enumeration.variants {
                    for (label, full) in [("smallest", false), ("full", true)] {
                        let mut bytes = Vec::new();
                        write_variant(enumeration, variant, full, schema, &mut bytes);
                        let name = format!("enum_{}_{}_{}", emit_limits::snake(&enumeration.name), variant.name, label);
                        cases.push(golden_case(
                            &name,
                            &enumeration.name,
                            &variant_expr(enumeration, variant, full, schema),
                            &bytes,
                        ));
                        goldens.push((root, Golden { name: format!("{name}.bin"), bytes }));
                    }
                }
            }
        }
    }
    goldens
}

pub(crate) fn emit(schema: &Context<'_>, out: &mut String) -> Vec<Golden> {
    let mut imports = vec!["CEILINGS".to_string()];
    let mut bounds = Vec::new();
    for declaration in schema.declarations {
        match declaration {
            Declaration::Record(record) => {
                imports.push(record.name.clone());
                imports.push(format!("{}Parts", record.name));
                bound_cases(record, schema, &mut bounds);
            }
            Declaration::Enum(enumeration) => imports.push(enumeration.name.clone()),
        }
    }
    if !bounds.is_empty() {
        imports.push("Path".to_string());
    }
    let mut cases = Vec::new();
    let goldens = golden_cases(schema, &mut cases);
    out.push_str("\n#[cfg(test)]\nmod golden_tests {\n");
    if !cases.is_empty() {
        out.push_str(&format!("    use super::{{{}}};\n", imports.join(", ")));
        if !bounds.is_empty() {
            out.push_str("    use super::Limits;\n");
        }
        let helpers = include_str!("test_helpers.rs");
        let helpers =
            if bounds.is_empty() { helpers.split("type BoundDecode").next().expect("golden helper") } else { helpers };
        for line in helpers.lines().skip(1) {
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("    {line}\n"));
            }
        }
    }
    tables("golden", &cases, out);
    tables("bounds", &bounds, out);
    out.push_str("}\n");
    goldens
}
