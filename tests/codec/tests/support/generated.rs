//! Seeded canonical wire values and the generated decoder for every declaration.

use skein_codegen::{Declaration, Schema, Type};
use skein_lib::{Reader, Rng, Writer};

#[derive(Debug)]
pub struct Family {
    pub name: &'static str,
    pub text: &'static str,
    pub roots: &'static [Root],
}

#[derive(Debug)]
pub struct Root {
    pub name: &'static str,
    pub decode: fn(&[u8]) -> Decoded,
}

#[derive(Debug)]
pub enum Decoded {
    Value { debug: String, bytes: Vec<u8> },
    Problem { path: String, reason: skein_codec::Reason },
}

macro_rules! roots {
    ($module:ident; $($name:ident),+ $(,)?) => {
        &[$(Root {
            name: stringify!($name),
            decode: |bytes| {
                use skein_codec_tests::$module::{self, $name};
                match $name::decode(&$module::CEILINGS, &mut Reader::new(bytes)) {
                    Ok(value) => {
                        let debug = format!("{value:?}");
                        let mut writer = Writer::new(usize::try_from(value.measure()).expect("measured length"));
                        value.encode(&mut writer).expect("measured room");
                        Decoded::Value { debug, bytes: writer.finish().into_vec() }
                    }
                    Err(problem) => Decoded::Problem {
                        path: format!("{:?}", problem.path), reason: problem.reason,
                    },
                }
            },
        }),+]
    };
}

pub fn families() -> [Family; 5] {
    [
        Family {
            name: "sample",
            text: include_str!("../../schema/sample-v1.schema"),
            roots: roots!(v1; Field, Effect, Report, Choice, Marker),
        },
        Family {
            name: "scalars",
            text: include_str!("../../schema/scalars-v1.schema"),
            roots: roots!(scalars; Empty, Flag, Scalars),
        },
        Family {
            name: "lint-fixture",
            text: include_str!("../../schema/lints-v1.schema"),
            roots: roots!(lints; HostTool, Boundless, Payload, OtherPayload, Shared, OneByteItems,
                OptionalPayload, BoxedChoice, InlineChoice, FixedWidthItems, LongRecord),
        },
        Family {
            name: "secrets",
            text: include_str!("../../schema/secrets-v1.schema"),
            roots: roots!(secrets; Credential),
        },
        Family {
            name: "every",
            text: include_str!("../../schema/every-v1.schema"),
            roots: roots!(every; Blank, Numbers, Strings, Light, Stamp, Shape, Nest, Every),
        },
    ]
}

pub fn draw(schema: &Schema, root: &str, rng: &mut Rng) -> Vec<u8> {
    let mut bytes = Vec::new();
    draw_named(schema, root, rng, &mut bytes);
    bytes
}

fn draw_named(schema: &Schema, name: &str, rng: &mut Rng, bytes: &mut Vec<u8>) {
    let declaration = schema
        .declarations
        .iter()
        .find(|declaration| match declaration {
            Declaration::Record(record) => record.name == name,
            Declaration::Enum(enumeration) => enumeration.name == name,
        })
        .expect("type declared in parsed schema");
    match declaration {
        Declaration::Record(record) => {
            if record.versioned {
                bytes.extend_from_slice(&schema.version.to_be_bytes());
            }
            for field in &record.fields {
                draw_type(schema, &field.ty, rng, bytes);
            }
        }
        Declaration::Enum(enumeration) => {
            let tag = rng.below(u64::try_from(enumeration.variants.len()).expect("variant count fits"));
            bytes.push(u8::try_from(tag).expect("tag fits"));
            let variant =
                enumeration.variants.get(usize::try_from(tag).expect("tag fits index")).expect("chosen variant");
            if let Some(record) = &variant.record {
                draw_named(schema, record, rng, bytes);
            }
        }
    }
}

fn integer(maximum: u64, rng: &mut Rng) -> u64 {
    match rng.below(3) {
        0 => 0,
        1 => maximum,
        _ => rng.between(0, maximum),
    }
}

fn length(bound: u32, rng: &mut Rng) -> u32 {
    if bound <= 16 && rng.below(5) == 0 {
        bound
    } else {
        u32::try_from(rng.between(0, u64::from(bound.min(8)))).expect("bounded length")
    }
}

fn random_bytes(count: u32, rng: &mut Rng, bytes: &mut Vec<u8>) {
    for _ in 0..count {
        bytes.push(u8::try_from(rng.below(256)).expect("byte"));
    }
}

fn text(bound: u32, rng: &mut Rng) -> String {
    let target = usize::try_from(length(bound, rng)).expect("text length fits");
    let mut text = String::new();
    while text.len() < target {
        let character = match rng.below(3) {
            0 => char::from(u8::try_from(rng.between(32, 126)).expect("ASCII")),
            1 => 'é',
            _ => '界',
        };
        if character.len_utf8() <= target.checked_sub(text.len()).expect("room remains") {
            text.push(character);
        } else {
            text.push('a');
        }
    }
    text
}

fn draw_type(schema: &Schema, ty: &Type, rng: &mut Rng, bytes: &mut Vec<u8>) {
    match ty {
        Type::U8 => bytes.push(u8::try_from(integer(u64::from(u8::MAX), rng)).expect("u8")),
        Type::U16 => {
            bytes.extend_from_slice(&u16::try_from(integer(u64::from(u16::MAX), rng)).expect("u16").to_be_bytes());
        }
        Type::U32 => {
            bytes.extend_from_slice(&u32::try_from(integer(u64::from(u32::MAX), rng)).expect("u32").to_be_bytes());
        }
        Type::U64 => bytes.extend_from_slice(&integer(u64::MAX, rng).to_be_bytes()),
        Type::Bool => bytes.push(u8::try_from(rng.below(2)).expect("bool")),
        Type::Duration => bytes.extend_from_slice(&rng.next_u64().to_be_bytes()),
        Type::Fixed(count) => random_bytes(*count, rng, bytes),
        Type::Bytes(bound) | Type::Secret(bound) => {
            let count = length(*bound, rng);
            bytes.extend_from_slice(&count.to_be_bytes());
            random_bytes(count, rng, bytes);
        }
        Type::Text(bound) => {
            let text = text(*bound, rng);
            bytes.extend_from_slice(&u32::try_from(text.len()).expect("bounded text").to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        Type::List(bound, item) => {
            let count = u32::try_from(rng.between(0, u64::from((*bound).min(3)))).expect("bounded count");
            bytes.extend_from_slice(&count.to_be_bytes());
            for _ in 0..count {
                draw_type(schema, item, rng, bytes);
            }
        }
        Type::Option(item) => {
            let present = rng.below(2) == 1;
            bytes.push(u8::from(present));
            if present {
                draw_type(schema, item, rng, bytes);
            }
        }
        Type::Named(name) => draw_named(schema, name, rng, bytes),
    }
}
