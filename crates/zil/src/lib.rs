//! zil: a terse calculator and scripting language for units, dates and much more. The CLI is the `zil-cli` crate, the browser build `zil-wasm`.

#![deny(clippy::unwrap_used)]

mod ansi;
mod ast;
mod concepts;
mod data;
mod error;
mod guide;
mod help;
pub mod host;
mod interp;
mod lexer;
mod modules;
mod ops;
mod parser;
mod repl;
mod session;
mod settings;
mod signature;
mod snapshot;
mod state;
mod tags;
mod value;

use interp::Interp;
use std::path::PathBuf;

#[cfg(any(target_arch = "wasm32", test))]
pub use ansi::to_html;
pub use ansi::{RESET, Role, color_on, highlight, strip_ansi, tint};
pub use data::Data;
pub use error::{Error, Limit};
pub use help::{all_examples, docs::pages as doc_pages, examples_json, help, live, tip, vocab_json};
#[cfg(any(target_arch = "wasm32", test))]
pub use help::{linked, tour_json};
pub use interp::Limits;
pub use repl::{Out, Shown, Transcript, abbreviation, help_call, help_shorthand, unclosed};
pub use session::{Builder, Ctx, HostModule, Session, Statement};
pub use settings::{KEYS, Key, Settings};
pub use value::Value;

/// A line of program output, from `print` and friends.
fn print(s: &str) {
    host::get().print(s)
}

/// Per-user cache for currency rates and REPL history. A `zil-data` folder beside the executable takes its place:
/// that is the portable build, which leaves nothing behind outside its own folder.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("zil-data"))).filter(|d| d.is_dir()) {
        return Some(dir);
    }
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    let base = env("XDG_CACHE_HOME").or_else(|| env("LOCALAPPDATA")).or_else(|| Some(env("HOME")?.join(".cache")))?;
    Some(base.join("zil"))
}

/// Splits `src` into top-level statements, to run one at a time with `Session::run`.
pub fn parse(src: &str) -> Result<Vec<Statement>, Error> {
    Ok(parser::parse(src)?.into_iter().map(Statement).collect())
}

fn run(interp: &mut Interp, src: &str) -> Result<Value, Error> {
    interp.run(&interp.parse(src)?)
}

/// Interpreter thread stack: deep enough for `interp::MAX_DEPTH` nested calls in a debug build.
/// Only reserved, not committed, so the size costs nothing until used. The browser build sets its own in `.cargo/config.toml`.
#[cfg(not(target_arch = "wasm32"))]
pub const STACK: usize = 1 << 30;
