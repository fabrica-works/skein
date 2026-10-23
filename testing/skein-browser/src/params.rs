//! The CDP parameters the browser sends. One description is written twice:
//! measuring before allocation, then writing exactly the measured length.

use crate::boundary::{Motion, Rect, Scheme};
use alloc::boxed::Box;

use skein_json::writer::{Encoder, Limits as JsonLimits};

#[derive(Debug)]
pub(crate) enum Params<'a> {
    Empty,
    Frame(&'a [u8]),
    CaretSheet(&'a [u8]),
    Dialog {
        accept: bool,
    },
    Document,
    Root,
    Selector {
        node: u64,
        selector: &'a [u8],
    },
    Node(u64),
    Children(u64),
    Object(&'a [u8]),
    Context(&'a [u8]),
    Target {
        url: &'a [u8],
        context: &'a [u8],
    },
    TargetId(&'a [u8]),
    AttachTarget(&'a [u8]),
    Url(&'a [u8]),
    Backend(u64),
    Query {
        object: &'a [u8],
        role: &'a [u8],
        name: Option<&'a [u8]>,
    },
    Quads(u64),
    Point {
        x: i64,
        y: i64,
    },
    Mouse {
        kind: &'static [u8],
        x: i64,
        y: i64,
        pressed: bool,
    },
    Text(&'a [u8]),
    Composition(&'a [u8]),
    Key {
        kind: &'static [u8],
        key: &'a [u8],
        code: &'a [u8],
        windows: u64,
        text: &'a [u8],
        modifiers: u64,
        /// The editing command the key's press runs: Chromium on macOS runs none
        /// for a synthesized shortcut (its key bindings are the system's), so a
        /// select-all names `selectAll`, on every system.
        command: Option<&'static [u8]>,
    },
    Viewport {
        width: u64,
        height: u64,
        mobile: bool,
    },
    Media {
        scheme: Scheme,
        motion: Motion,
    },
    Push(u64),
    Shot {
        clip: Rect,
        scale: u8,
    },
    HistoryEntry(u64),
    Describe {
        backend: u64,
        depth: i64,
    },
}

impl Params<'_> {
    pub(crate) fn encode(&self, limit: u32) -> Option<Box<[u8]>> {
        let limits = JsonLimits { depth: 4, length: limit };
        let mut measure = Encoder::measure(&limits);
        self.write(&mut measure);
        let len = measure.measured().ok()?;
        let mut writer = Encoder::write(len, &limits);
        self.write(&mut writer);
        Some(writer.finish())
    }

    #[expect(clippy::too_many_lines, reason = "one match gives each CDP parameter shape its measured and written form")]
    fn write(&self, json: &mut Encoder) {
        json.object_start();
        match self {
            Params::Empty => {}
            Params::Frame(frame) => {
                json.key(b"frameId");
                json.string(frame);
            }
            Params::CaretSheet(sheet) => {
                json.key(b"styleSheetId");
                json.string(sheet);
                json.key(b"text");
                json.string(b"* { caret-animation: manual !important }");
            }
            Params::Dialog { accept } => {
                json.key(b"accept");
                json.boolean(*accept);
            }
            Params::Root => {
                json.key(b"depth");
                json.signed(0);
                json.key(b"pierce");
                json.boolean(false);
            }
            Params::Selector { node, selector } => {
                json.key(b"nodeId");
                json.unsigned(*node);
                json.key(b"selector");
                json.string(selector);
            }
            Params::Children(node) => {
                json.key(b"nodeId");
                json.unsigned(*node);
                json.key(b"depth");
                json.signed(-1);
                json.key(b"pierce");
                json.boolean(true);
            }
            Params::Node(node) => {
                json.key(b"nodeId");
                json.unsigned(*node);
            }
            Params::Document => {
                json.key(b"expression");
                json.string(b"document");
            }
            Params::Object(object) => {
                json.key(b"objectId");
                json.string(object);
            }
            Params::Context(context) => {
                json.key(b"browserContextId");
                json.string(context);
            }
            Params::Target { url, context } => {
                json.key(b"url");
                json.string(url);
                json.key(b"browserContextId");
                json.string(context);
            }
            Params::TargetId(target) => {
                json.key(b"targetId");
                json.string(target);
            }
            Params::AttachTarget(target) => {
                json.key(b"targetId");
                json.string(target);
                json.key(b"flatten");
                json.boolean(true);
            }
            Params::Url(url) => {
                json.key(b"url");
                json.string(url);
            }
            Params::Backend(backend) | Params::Quads(backend) => {
                json.key(b"backendNodeId");
                json.unsigned(*backend);
            }
            Params::Query { object, role, name } => {
                json.key(b"objectId");
                json.string(object);
                json.key(b"role");
                json.string(role);
                if let Some(name) = name {
                    json.key(b"accessibleName");
                    json.string(name);
                }
            }
            Params::Point { x, y } => {
                json.key(b"x");
                json.signed(*x);
                json.key(b"y");
                json.signed(*y);
                json.key(b"includeUserAgentShadowDOM");
                json.boolean(true);
            }
            Params::Mouse { kind, x, y, pressed } => {
                json.key(b"type");
                json.string(kind);
                json.key(b"x");
                json.signed(*x);
                json.key(b"y");
                json.signed(*y);
                if *pressed {
                    json.key(b"button");
                    json.string(b"left");
                    json.key(b"buttons");
                    json.unsigned(1);
                    json.key(b"clickCount");
                    json.unsigned(1);
                } else {
                    json.key(b"button");
                    json.string(b"left");
                    json.key(b"buttons");
                    json.unsigned(0);
                    json.key(b"clickCount");
                    json.unsigned(1);
                }
            }
            Params::Text(text) => {
                json.key(b"text");
                json.string(text);
            }
            Params::Composition(text) => {
                json.key(b"text");
                json.string(text);
                json.key(b"selectionStart");
                json.unsigned(u64::try_from(text.len()).expect("text length fits u64"));
                json.key(b"selectionEnd");
                json.unsigned(u64::try_from(text.len()).expect("text length fits u64"));
            }
            Params::Key { kind, key, code, windows, text, modifiers, command } => {
                json.key(b"type");
                json.string(kind);
                json.key(b"key");
                json.string(key);
                json.key(b"code");
                json.string(code);
                json.key(b"windowsVirtualKeyCode");
                json.unsigned(*windows);
                json.key(b"text");
                json.string(text);
                if *modifiers != 0 {
                    json.key(b"modifiers");
                    json.unsigned(*modifiers);
                }
                if let Some(command) = command {
                    json.key(b"commands");
                    json.array_start();
                    json.string(command);
                    json.array_end();
                }
            }
            Params::Viewport { width, height, mobile } => {
                json.key(b"width");
                json.unsigned(*width);
                json.key(b"height");
                json.unsigned(*height);
                json.key(b"deviceScaleFactor");
                json.unsigned(1);
                json.key(b"mobile");
                json.boolean(*mobile);
            }
            Params::Media { scheme, motion } => {
                json.key(b"features");
                json.array_start();
                for (name, value) in [
                    (
                        b"prefers-color-scheme".as_slice(),
                        match scheme {
                            Scheme::Light => b"light".as_slice(),
                            Scheme::Dark => b"dark".as_slice(),
                        },
                    ),
                    (
                        b"prefers-reduced-motion".as_slice(),
                        match motion {
                            Motion::Reduce => b"reduce".as_slice(),
                            Motion::NoPreference => b"no-preference".as_slice(),
                        },
                    ),
                ] {
                    json.object_start();
                    json.key(b"name");
                    json.string(name);
                    json.key(b"value");
                    json.string(value);
                    json.object_end();
                }
                json.array_end();
            }
            Params::Push(node) => {
                json.key(b"backendNodeIds");
                json.array_start();
                json.unsigned(*node);
                json.array_end();
            }
            Params::Shot { clip, scale } => {
                json.key(b"format");
                json.string(b"png");
                json.key(b"clip");
                json.object_start();
                json.key(b"x");
                json.signed(clip.x);
                json.key(b"y");
                json.signed(clip.y);
                json.key(b"width");
                json.unsigned(clip.width);
                json.key(b"height");
                json.unsigned(clip.height);
                json.key(b"scale");
                json.unsigned(u64::from(*scale));
                json.object_end();
                json.key(b"captureBeyondViewport");
                json.boolean(true);
            }
            Params::HistoryEntry(entry) => {
                json.key(b"entryId");
                json.unsigned(*entry);
            }
            Params::Describe { backend, depth } => {
                json.key(b"backendNodeId");
                json.unsigned(*backend);
                json.key(b"depth");
                json.signed(*depth);
                json.key(b"pierce");
                json.boolean(true);
            }
        }
        json.object_end();
    }
}
