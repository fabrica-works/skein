//! The generic tree in the generated types' Debug form (codec.md, 8.3).

use skein_codec::schema::{DeclIx, FieldIx, Schema, Shape, Span, Type, TypeIx};
use skein_codec::value::{Node, Part, Path, Value};

fn spelling(schema: &Schema, span: Span) -> &str {
    core::str::from_utf8(schema.spelled(span)).expect("schema identifier")
}

// emit_limits::pascal: an underscore makes the next character uppercase.
fn pascal(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.push(character.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

#[must_use]
pub fn path_name(schema: &Schema, path: Path) -> String {
    let name = spelling(schema, schema.declaration(path.decl).name);
    let part = match path.part {
        Part::Field(field) => pascal(spelling(schema, schema.field(field).name)),
        Part::Tag => "Tag".into(),
        Part::Tail => "Tail".into(),
        Part::Version => "Version".into(),
    };
    format!("{name}{part}")
}

#[must_use]
pub fn render(schema: &Schema, value: &Value, node: Node, decl: DeclIx) -> String {
    let declaration = schema.declaration(decl);
    match declaration.shape {
        Shape::Record { first, count, .. } => {
            let name = spelling(schema, declaration.name);
            let mut fields = Vec::new();
            let mut bools = Vec::new();
            for offset in 0..count {
                let field = FieldIx::new(first.index().checked_add(offset).expect("field index"));
                let child = value.field(node, field).expect("record field");
                let field = schema.field(field);
                let rendered = render_type(schema, value, child, field.ty);
                if schema.ty(field.ty) == Type::Bool {
                    bools.push(rendered.clone());
                }
                fields.push((schema.ty(field.ty), format!("{}: {}", spelling(schema, field.name), rendered)));
            }
            // emit_types::grouped_bools puts four or more bools in the first member.
            let fields = if bools.len() > 3 {
                let mut grouped = vec![format!("skein_bools: [{}]", bools.join(", "))];
                grouped.extend(fields.into_iter().filter(|(ty, _)| *ty != Type::Bool).map(|(_, text)| text));
                grouped
            } else {
                fields.into_iter().map(|(_, text)| text).collect::<Vec<_>>()
            };
            if fields.is_empty() { name.into() } else { format!("{name} {{ {} }}", fields.join(", ")) }
        }
        Shape::Enum { .. } => {
            let variant = schema.variant(value.tag(node).expect("enumeration"));
            let name = pascal(spelling(schema, variant.name));
            match variant.record {
                Some(decl) => format!(
                    "{name}({})",
                    render(schema, value, value.payload(node).expect("enum").expect("payload"), decl)
                ),
                None => name,
            }
        }
    }
}

fn render_type(schema: &Schema, value: &Value, node: Node, ty: TypeIx) -> String {
    match schema.ty(ty) {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 => format!("{}", value.uint(node).expect("integer")),
        Type::Duration => format!("{:?}", skein_lib::Duration::from_nanos(value.uint(node).expect("duration"))),
        Type::Bool => format!("{}", value.boolean(node).expect("boolean")),
        Type::Fixed(_) | Type::Bytes { .. } | Type::Text(_) => format!("{:?}", value.content(node).expect("content")),
        Type::Secret(_) => format!("Secret {{ len: {} }}", value.expose(node).expect("secret").len()),
        Type::Named(decl) => render(schema, value, node, decl),
        Type::List { item, .. } => {
            let mut items = value.items(node).expect("list");
            let mut rendered = Vec::new();
            while let Some(child) = value.next_item(&mut items) {
                rendered.push(render_type(schema, value, child, item));
            }
            format!("List {{ items: [{}], capacity: {} }}", rendered.join(", "), rendered.len())
        }
        Type::Option(item) => match value.some(node).expect("option") {
            Some(child) => format!("Some({})", render_type(schema, value, child, item)),
            None => "None".into(),
        },
    }
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

#[must_use]
pub fn golden_root(schema: &Schema, name: &str) -> DeclIx {
    (0..schema.declarations())
        .map(DeclIx::new)
        .find(|decl| {
            let declaration = schema.declaration(*decl);
            let prefix = match declaration.shape {
                Shape::Record { .. } => "record",
                Shape::Enum { .. } => "enum",
            };
            name.starts_with(&format!("{prefix}_{}_", snake(spelling(schema, declaration.name))))
        })
        .expect("golden names its declaration")
}

// Every's largest tree has fewer than 128 nodes at its ceilings; all other
// families here have fewer than 128 too (LongRecord has 100 including its root).
pub const ROOM: u32 = 256;
