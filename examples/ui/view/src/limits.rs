//! Capacities derived from 64 notes and the record ceilings (ui.md, 15).
use crate::records;

#[must_use]
pub fn limits() -> skein_ui::Limits {
    skein_ui::Limits {
        nodes: 600,
        depth: 6,
        text_per_node: 256,
        text_bytes: 24_000,
        drawing_bytes: 0,
        styles_per_node: 0,
        patches: 1200,
        bindings: 64,
        slots: 0,
        markdown_depth: 0,
    }
}

#[must_use]
pub fn worst_case() -> Option<u64> {
    skein_ui::worst_case(&limits())?
        .checked_add(records::Notes::worst_case_heap(&records::CEILINGS)?)?
        .checked_add(crate::view::words_worst_case()?)
}
