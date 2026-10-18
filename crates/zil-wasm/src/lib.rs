//! The browser build, for the sandbox and the docs' run buttons. `pnpm wasm` builds it; `docs/sandbox/worker.js` calls it.
//! `Session` (in `session.rs`) is the embedding API for other JS code.
#![cfg(target_arch = "wasm32")]

mod session;

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use zil::ansi::{RESET, Role, strip_ansi, tint, to_html};
use zil::host::Host;
use zil::repl::{self, Shown};
use zil::{Session, Value, help};

thread_local! {
    /// What the program printed, since the last `take_output`.
    static OUT: RefCell<String> = const { RefCell::new(String::new()) };
    /// The sandbox's REPL: one session for the whole page, and how many results it has numbered.
    static SESSION: RefCell<(Session, usize)> = RefCell::new((sandboxed(), 0));
}

/// The browser: output goes to the JS function, or collects for the page to take; the clock and randomness come from JS,
/// and HTTP goes through the worker's `zilGet`. No files, processes or environment.
struct JsHost(Option<js_sys::Function>);

impl Host for JsHost {
    fn print(&self, text: &str) {
        match &self.0 {
            Some(f) => drop(f.call1(&wasm_bindgen::JsValue::NULL, &text.into())),
            None => OUT.with_borrow_mut(|o| {
                o.push_str(text);
                o.push('\n');
            }),
        }
    }
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }
    fn time_zone(&self) -> jiff::tz::TimeZone {
        jiff::tz::TimeZone::system()
    }
    fn seed(&self) -> u64 {
        fastrand::u64(..)
    }
    fn http_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, String> {
        zil_get(url, &serde_json::to_string(query).unwrap()).map_err(|e| e.as_string().unwrap_or_else(|| "network error".into()))
    }
    fn exit(&self, _: i32) -> String {
        "there's no process to exit in the browser".into()
    }
}

#[wasm_bindgen]
extern "C" {
    /// A blocking GET, defined by `docs/sandbox/worker.js`: `query` is JSON `[[key, value], ...]`, and failures throw a message.
    #[wasm_bindgen(catch, js_name = zilGet)]
    fn zil_get(url: &str, query: &str) -> Result<String, wasm_bindgen::JsValue>;
}

#[wasm_bindgen(start)]
fn start() {
    zil::host::set_default(Rc::new(JsHost(None)));
}

/// A session whose runaway code errors well inside the page's 10 s worker kill, so a REPL session survives it.
fn sandboxed() -> Session {
    let mut s = Session::new(zil::host::get());
    s.limits().seconds = Some(5.0);
    s.limits().output = 1 << 20;
    s
}

/// Everything printed since the last call.
fn take_output() -> String {
    OUT.take()
}

/// A value as the REPL shows it, in color; nothing for nil and functions.
fn shown(v: &Value) -> String {
    match v {
        Value::Nil | Value::Fn(_) => String::new(),
        Value::Table(_) => repl::display(v, true),
        Value::Str(s) if s.contains(['\n', '\x1b']) => s.to_string(),
        v => format!("{}{v:?}{RESET}", tint(v).code()),
    }
}

/// Runs a script in a fresh session: `[printed, result, error, lines]`, the first three HTML and possibly empty.
/// `lines` is JSON `[[line, text], ...]`: each 0-based line ending a statement with a one-line value, and the error's
/// line, for the editor to show beside the code.
#[wasm_bindgen]
pub fn run(src: &str) -> Vec<String> {
    take_output();
    let last_line = |end: usize| src[..end].trim_end().matches('\n').count();
    let error_line = |e: &zil::Error| e.line_col(src).0 - 1;
    let mut lines: Vec<(usize, String)> = Vec::new();
    let (result, err) = match zil::parse(src) {
        Err(e) => {
            lines.push((error_line(&e), format!("error: {}", e.msg)));
            (String::new(), e.render("sandbox", src, true))
        }
        Ok(prog) => {
            let mut session = sandboxed();
            let mut out = Ok(Value::Nil);
            for stmt in &prog {
                let line = last_line(stmt.span().end);
                out = session.run(stmt);
                // A loop's value is just its last pass; `x = 5; x += 2` on one line shows only the last.
                lines.retain(|l| l.0 != line);
                match &out {
                    Ok(v) if !stmt.is_loop() => {
                        let text = strip_ansi(&shown(v));
                        if !text.is_empty() && !text.contains('\n') {
                            lines.push((line, text));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        lines.push((error_line(e), format!("error: {}", e.msg)));
                        break;
                    }
                }
            }
            match out {
                Ok(v) => (shown(&v), String::new()),
                Err(e) => (String::new(), e.render("sandbox", src, true)),
            }
        }
    };
    let mut html: Vec<_> = [take_output(), result, err].iter().map(|s| to_html(s.trim_end())).collect();
    html.push(serde_json::to_string(&lines).unwrap());
    html
}

/// Runs a docs code block one top-level statement at a time in one session, like typing it into the REPL.
/// Returns JSON `[[line, text], ...]`: plain text to show after each 0-based `line` that ends a statement with output.
/// Assignments and loops stay quiet, so a recipe shows only what it prints and its last value.
#[wasm_bindgen]
pub fn notebook(src: &str) -> String {
    let last_line = |end: usize| src[..end].trim_end().matches('\n').count();
    let rows: Vec<(usize, String)> = match zil::parse(src) {
        Err(e) => vec![(last_line(src.len()), format!("error: {}", e.msg))],
        Ok(prog) => {
            let mut session = sandboxed();
            take_output();
            prog.iter()
                .enumerate()
                .filter_map(|(i, stmt)| {
                    let r = session.run(stmt);
                    let line = last_line(stmt.span().end);
                    // `x = 5; x += 2; x` shows one result, like a REPL line.
                    let quiet = stmt.is_assignment() || stmt.is_loop() || prog.get(i + 1).is_some_and(|next| last_line(next.span().end) == line);
                    let printed = take_output();
                    let value = match r {
                        // `"abc".find("z")` → nil is an answer; after `print` it's just noise.
                        Ok(Value::Nil) if !quiet && printed.is_empty() => "nil".into(),
                        Ok(v) if !quiet => strip_ansi(&shown(&v)),
                        Ok(_) => String::new(),
                        Err(e) => format!("error: {}", e.msg),
                    };
                    let text = (printed + &value).trim_end().to_string();
                    (!text.is_empty()).then_some((line, text))
                })
                .collect()
        }
    };
    serde_json::to_string(&rows).unwrap()
}

/// The sandbox's sidebar: `help("examples")`'s sections as JSON `[[top-level module, title, [[label, code], ...]], ...]`, highlights first.
#[wasm_bindgen]
pub fn examples() -> String {
    help::examples_json()
}

/// The sandbox's tour as JSON `[[chapter, [[text, code], ...]], ...]`.
#[wasm_bindgen]
pub fn tour() -> String {
    help::tour_json()
}

/// Starts a fresh REPL session, returning its greeting as HTML.
#[wasm_bindgen]
pub fn repl_reset() -> String {
    SESSION.set((sandboxed(), 0));
    to_html(&format!(
        "zil {} {}· help for docs{RESET}
{}",
        env!("CARGO_PKG_VERSION"),
        Role::Dim.code(),
        help::linked(true, || help::tip(true))
    ))
}

/// Whether Enter should continue `src` on a new line instead of running it.
#[wasm_bindgen]
pub fn repl_open(src: &str) -> bool {
    repl::unclosed(src)
}

/// Runs one REPL input in the session, as the terminal REPL would: `[printed, shown, error]` as HTML, like `run`.
#[wasm_bindgen]
pub fn repl(src: &str) -> Vec<String> {
    if src.trim() == "clear history" {
        SESSION.with_borrow_mut(|(session, n)| repl::forget(session, n));
        return vec![String::new(); 3];
    }
    take_output();
    let src = &*repl::help_sugar(src);
    // Help shown straight to the screen links its names and examples, for the sandbox to make clickable.
    let (shown, err) = help::linked(repl::help_call(src), || match repl::help_shorthand(src) {
        Some(topic) => (help::help(topic, true).unwrap_or_else(|e| e), String::new()),
        // Like the terminal REPL, a bare `help(...)` or `tip()` comes back colored; the log scrolls, so nothing wraps.
        None => SESSION.with_borrow_mut(|(session, n)| match help::live(repl::help_call(src).then_some((true, usize::MAX)), || session.eval(src)) {
            Ok(v) => match repl::record(session, n, src, &v) {
                Shown::Hidden => (String::new(), String::new()),
                Shown::Bare => (v.to_string(), String::new()),
                Shown::Numbered(k) => (repl::show(k, &v, true), String::new()),
                Shown::Named(k, pairs) => (repl::show_named(k, &pairs, true), String::new()),
                Shown::Defined(sig) => (format!("{}{sig}{RESET}", Role::Dim.code()), String::new()),
            },
            Err(e) => (String::new(), e.render("<repl>", src, true)),
        }),
    });
    [take_output(), shown, err].iter().map(|s| to_html(s.trim_end())).collect()
}

/// Completions the editor can't work out itself, after `to` and after a quantity: `repl::complete` in the REPL's session
/// for the text `before` the word at the cursor, one per line.
#[wasm_bindgen]
pub fn complete(before: &str) -> String {
    SESSION.with_borrow(|(session, _)| repl::complete(session, before, "").join("\n"))
}

/// The sandbox editor's vocabulary, for completion, hover docs and highlighting, as JSON:
/// `{fns: [[name, sig, desc, module], ...], units: [[names, desc], ...], consts: [name, ...], modules: [[path, about], ...]}`.
#[wasm_bindgen]
pub fn vocab() -> String {
    help::vocab_json()
}
