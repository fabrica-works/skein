use crate::Limits;

pub(crate) const LIMITS: Limits = Limits {
    nodes: 12,
    depth: 4,
    text_per_node: 32,
    text_bytes: 128,
    drawing_bytes: 8,
    styles_per_node: 2,
    patches: 72,
    bindings: 4,
    slots: 4,
    markdown_depth: 2,
    scene_marks: 16,
    scene_bytes: 256,
};

/// Start a generic page with a single-node banner.
pub(crate) fn page() -> crate::Builder {
    let mut builder = crate::Builder::new(&LIMITS);
    builder.open(crate::Role::Page);
    builder.open(crate::Role::Banner);
    builder.close();
    builder
}

pub(crate) fn finish(mut builder: crate::Builder) -> crate::Tree {
    builder.close();
    builder.finish()
}
