//! The text shell's pure rendering and commands (ui-text.md, 3).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
mod command;
pub use command::{Command, Refusal, command, refusal_line};
mod render;
pub use render::{Print, render, worst_case};
mod terminal;
pub use terminal::{Settings, Terminal};
#[cfg(test)]
mod tests;
