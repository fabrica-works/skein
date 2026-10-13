#![expect(clippy::format_push_string, reason = "generator assembles reviewed source fragments")]

//! Checked wire and heap ceilings for codec.md, section 4.

use crate::{Declaration, Enumeration, Record, Type, analysis::Context, emit_limits};

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::Bool => "bool".into(),
        Type::Duration => "skein_lib::Duration".into(),
        Type::Fixed(bound) => format!("[u8; {}]", emit_limits::literal(*bound)),
        Type::Secret(_) => "skein_lib::Secret".into(),
        Type::Bytes(_) | Type::Text(_) => "Box<[u8]>".into(),
        Type::List(_, item) => format!("List<{}>", rust_type(item)),
        Type::Option(item) => format!("Option<{}>", rust_type(item)),
        Type::Named(name) => name.clone(),
    }
}

fn bytes_expr(ty: &Type, bound: &str) -> String {
    match ty {
        Type::U8 | Type::Bool => "1_u64".into(),
        Type::U16 => "2_u64".into(),
        Type::U32 => "4_u64".into(),
        Type::U64 | Type::Duration => "8_u64".into(),
        Type::Fixed(length) => format!("u64::from({length}_u32)"),
        Type::Bytes(_) | Type::Secret(_) | Type::Text(_) => format!("4_u64.checked_add(u64::from(limits.{bound}))?"),
        Type::List(_, item) => {
            let item_expr = bytes_expr(item, &format!("{bound}_item"));
            format!("4_u64.checked_add(u64::from(limits.{bound}).checked_mul({item_expr})?)?")
        }
        Type::Option(item) => {
            let item_expr = bytes_expr(item, &format!("{bound}_some"));
            format!("1_u64.checked_add({item_expr})?")
        }
        Type::Named(name) => format!("{name}::worst_case_bytes_within(limits)?"),
    }
}

fn heap_expr(ty: &Type, bound: &str) -> String {
    match ty {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Bool | Type::Duration | Type::Fixed(_) => "0_u64".into(),
        Type::Bytes(_) | Type::Secret(_) | Type::Text(_) => format!("u64::from(limits.{bound})"),
        Type::List(_, item) => {
            let item_expr = heap_expr(item, &format!("{bound}_item"));
            format!(
                "List::<{}>::worst_case(limits.{bound})?.checked_add(u64::from(limits.{bound}).checked_mul({item_expr})?)?",
                rust_type(item)
            )
        }
        Type::Option(item) => heap_expr(item, &format!("{bound}_some")),
        Type::Named(name) => format!("{name}::worst_case_heap_within(limits)?"),
    }
}

fn emit_record(record: &Record, out: &mut String) {
    let limits_name = if record.fields.iter().any(|field| bytes_expr(&field.ty, "").contains("limits")) {
        "limits"
    } else {
        "_limits"
    };
    let size_mut = if record.fields.is_empty() { "" } else { "mut " };
    out.push_str(&format!("impl {} {{\n    /// Maximum encoded bytes under these limits.\n    #[must_use]\n    pub fn worst_case_bytes(limits: &Limits) -> Option<u64> {{\n        if check_limits(limits).is_err() {{ return None; }}\n        Self::worst_case_bytes_within(limits)\n    }}\n\n    pub(crate) fn worst_case_bytes_within({limits_name}: &Limits) -> Option<u64> {{\n        let {size_mut}size = {}_u64;\n", record.name, if record.versioned { 2_u64 } else { 0_u64 }));
    let mut bytes = Vec::new();
    for field in &record.fields {
        let bound = format!("{}_{}", emit_limits::snake(&record.name), field.name);
        bytes.push(bytes_expr(&field.ty, &bound));
    }
    emit_fold(&bytes, "size", "checked_add", "?", out);
    let heap_mut = if record.fields.is_empty() { "" } else { "mut " };
    out.push_str(&format!("        size.checked_add(0)\n    }}\n\n    /// Maximum heap held by one decoded value under these limits.\n    #[must_use]\n    pub fn worst_case_heap(limits: &Limits) -> Option<u64> {{\n        if check_limits(limits).is_err() {{ return None; }}\n        Self::worst_case_heap_within(limits)\n    }}\n\n    pub(crate) fn worst_case_heap_within({limits_name}: &Limits) -> Option<u64> {{\n        let {heap_mut}heap = 0_u64;\n"));
    let mut heaps = Vec::new();
    for field in &record.fields {
        let bound = format!("{}_{}", emit_limits::snake(&record.name), field.name);
        heaps.push(heap_expr(&field.ty, &bound));
    }
    emit_fold(&heaps, "heap", "checked_add", "?", out);
    out.push_str("        heap.checked_add(0)\n    }\n}\n\n");
}

// Chained pairs keep a record or enumeration with many fields below the function limit.
fn emit_fold(values: &[String], name: &str, operation: &str, checked: &str, out: &mut String) {
    for pair in values.chunks(2) {
        out.push_str(&format!("        {name} = {name}"));
        for value in pair {
            out.push_str(&format!(".{operation}({value}){checked}"));
        }
        out.push_str(";\n");
    }
}

fn emit_enum(enumeration: &Enumeration, out: &mut String) {
    let limits_name =
        if enumeration.variants.iter().any(|variant| variant.record.is_some()) { "limits" } else { "_limits" };
    let biggest_mut = if enumeration.variants.iter().any(|variant| variant.record.is_some()) { "mut " } else { "" };
    out.push_str(&format!("impl {} {{\n    /// Maximum encoded bytes among variants.\n    #[must_use]\n    pub fn worst_case_bytes(limits: &Limits) -> Option<u64> {{\n        if check_limits(limits).is_err() {{ return None; }}\n        Self::worst_case_bytes_within(limits)\n    }}\n\n    pub(crate) fn worst_case_bytes_within({limits_name}: &Limits) -> Option<u64> {{\n        let {biggest_mut}biggest = 0_u64;\n", enumeration.name));
    let mut bytes = Vec::new();
    for variant in &enumeration.variants {
        if let Some(record) = &variant.record {
            bytes.push(format!("{record}::worst_case_bytes_within(limits)?"));
        }
    }
    emit_fold(&bytes, "biggest", "max", "", out);
    out.push_str(&format!("        1_u64.checked_add(biggest)\n    }}\n\n    /// Maximum heap held by any decoded variant.\n    #[must_use]\n    pub fn worst_case_heap(limits: &Limits) -> Option<u64> {{\n        if check_limits(limits).is_err() {{ return None; }}\n        Self::worst_case_heap_within(limits)\n    }}\n\n    pub(crate) fn worst_case_heap_within({limits_name}: &Limits) -> Option<u64> {{\n        let {biggest_mut}biggest = 0_u64;\n"));
    let mut heaps = Vec::new();
    for variant in &enumeration.variants {
        if let Some(record) = &variant.record {
            let heap = format!("{record}::worst_case_heap_within(limits)?");
            let heap = if variant.boxed {
                format!("{heap}.checked_add(u64::try_from(size_of::<{record}>()).ok()?)?")
            } else {
                heap
            };
            heaps.push(heap);
        }
    }
    emit_fold(&heaps, "biggest", "max", "", out);
    out.push_str("        biggest.checked_add(0)\n    }\n}\n\n");
}

pub(crate) fn emit(schema: &Context<'_>, out: &mut String) {
    for declaration in schema.declarations {
        match declaration {
            Declaration::Record(record) => emit_record(record, out),
            Declaration::Enum(enumeration) => emit_enum(enumeration, out),
        }
    }
    let biggest_mut = if schema.declarations.is_empty() { "" } else { "mut " };
    out.push_str(&format!("/// Maximum wire bytes among this family's top-level types.\n#[must_use]\npub fn worst_case_bytes(limits: &Limits) -> Option<u64> {{\n    if check_limits(limits).is_err() {{ return None; }}\n    let {biggest_mut}biggest = 0_u64;\n"));
    for declaration in schema.declarations {
        let name = match declaration {
            Declaration::Record(record) => &record.name,
            Declaration::Enum(enumeration) => &enumeration.name,
        };
        out.push_str(&format!("    biggest = biggest.max({name}::worst_case_bytes_within(limits)?);\n"));
    }
    out.push_str(&format!("    Some(biggest)\n}}\n\n/// Maximum heap held by one decoded top-level value.\n#[must_use]\npub fn worst_case_heap(limits: &Limits) -> Option<u64> {{\n    if check_limits(limits).is_err() {{ return None; }}\n    let {biggest_mut}biggest = 0_u64;\n"));
    for declaration in schema.declarations {
        let name = match declaration {
            Declaration::Record(record) => &record.name,
            Declaration::Enum(enumeration) => &enumeration.name,
        };
        out.push_str(&format!("    biggest = biggest.max({name}::worst_case_heap_within(limits)?);\n"));
    }
    out.push_str("    Some(biggest)\n}\n");
}
