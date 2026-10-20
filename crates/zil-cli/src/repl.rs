//! The REPL: line editing with completion, hints and syntax colors, multi-line input, `help` shorthand and numbered results.

use crate::pager;
use crate::report;
use crate::sessions;
use rustyline::error::ReadlineError;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use zil::ansi::{RESET, Role, color_on, highlight};
use zil::repl::{self, Shown, help_shorthand, unclosed};
use zil::{Session, cache_dir, help};

/// REPL tab completion through `repl::complete`, over the shared session.
struct Names(Rc<RefCell<Session>>);

impl rustyline::completion::Completer for Names {
    type Candidate = String;
    fn complete(&self, line: &str, pos: usize, _: &rustyline::Context<'_>) -> rustyline::Result<(usize, Vec<String>)> {
        let start = pos - line[..pos].chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').map(char::len_utf8).sum::<usize>();
        Ok((start, repl::complete(&self.0.borrow(), &line[..start], &line[start..pos])))
    }
}

/// Fish-style ghost text: the rest of the word when exactly one completion fits.
impl rustyline::hint::Hinter for Names {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<String> {
        use rustyline::completion::Completer;
        if pos < line.len() {
            return None;
        }
        let (start, hits) = self.complete(line, pos, ctx).ok()?;
        let [hit] = hits.as_slice() else { return None };
        Some(hit[pos - start..].to_string()).filter(|h| !h.is_empty())
    }
}

/// Syntax colors for the line being typed; unlexable bits stay plain.
impl rustyline::highlight::Highlighter for Names {
    fn highlight<'l>(&self, line: &'l str, _: usize) -> Cow<'l, str> {
        Cow::Owned(highlight(line))
    }
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _: bool) -> Cow<'b, str> {
        Cow::Owned(format!("{}{prompt}{RESET}", Role::Prompt.code()))
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("{}{hint}{RESET}", Role::Dim.code()))
    }
    fn highlight_char(&self, _: &str, _: usize, _: rustyline::highlight::CmdKind) -> bool {
        true
    }
}

/// Help shaped for stdout, `(color, width)`, if `src` is help printed straight to it (see `repl::help_call`).
pub fn help_term(src: &str) -> Option<(bool, usize)> {
    let (terminal_size::Width(w), _) = terminal_size::terminal_size_of(std::io::stdout())?;
    repl::help_call(src).then_some((color_on(), w as usize))
}

/// A newline pasted on Windows arrives as Enter with the rest of the paste still queued; it joins the line instead of running it.
/// Elsewhere bracketed paste already inserts the whole paste.
impl rustyline::validate::Validator for Names {
    fn validate(&self, _: &mut rustyline::validate::ValidationContext) -> rustyline::Result<rustyline::validate::ValidationResult> {
        Ok(if pasting() { rustyline::validate::ValidationResult::Incomplete } else { rustyline::validate::ValidationResult::Valid(None) })
    }
}

/// Whether more keys are already waiting, as they are mid-paste but never right after a typed Enter.
#[cfg(windows)]
fn pasting() -> bool {
    use windows_sys::Win32::System::Console::{GetStdHandle, INPUT_RECORD, KEY_EVENT, PeekConsoleInputW, STD_INPUT_HANDLE};
    let mut recs = [unsafe { std::mem::zeroed::<INPUT_RECORD>() }; 16];
    let mut n = 0;
    let ok = unsafe { PeekConsoleInputW(GetStdHandle(STD_INPUT_HANDLE), recs.as_mut_ptr(), recs.len() as u32, &mut n) } != 0;
    ok && recs[..n as usize].iter().any(|r| u32::from(r.EventType) == KEY_EVENT && unsafe { r.Event.KeyEvent.bKeyDown } != 0)
}

#[cfg(not(windows))]
fn pasting() -> bool {
    false
}
impl rustyline::Helper for Names {}

/// Alt+Q: drop the line being typed but keep it in history, to come back to with Up.
struct Stash(std::sync::Arc<std::sync::Mutex<Option<String>>>);

impl rustyline::ConditionalEventHandler for Stash {
    fn handle(&self, _: &rustyline::Event, _: rustyline::RepeatCount, _: bool, ctx: &rustyline::EventContext) -> Option<rustyline::Cmd> {
        *self.0.lock().unwrap() = Some(ctx.line().to_string());
        Some(rustyline::Cmd::Interrupt)
    }
}

/// The REPL on session `name`: replays what it saved, then saves each input that runs. `archived` is the name `--new`
/// just moved the last session to, to say so.
pub fn repl(session: Session, name: &str, archived: Option<&str>) {
    let session = Rc::new(RefCell::new(session));
    let config = rustyline::Config::builder().completion_type(rustyline::CompletionType::List).build();
    let mut rl = rustyline::Editor::with_config(config).expect("terminal");
    rl.set_helper(Some(Names(Rc::clone(&session))));
    let stashed = std::sync::Arc::default();
    rl.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('q'), rustyline::Modifiers::ALT),
        rustyline::EventHandler::Conditional(Box::new(Stash(std::sync::Arc::clone(&stashed)))),
    );
    // Ctrl-C while code runs stops it; while reading a line, rustyline sees it as a key instead.
    let interrupt = std::sync::Arc::clone(&session.borrow_mut().limits().interrupt);
    let _ = ctrlc::set_handler(move || interrupt.store(true, std::sync::atomic::Ordering::Relaxed));
    let history = cache_dir().map(|d| d.join("history.txt"));
    if let Some(h) = &history {
        let _ = rl.load_history(h);
    }
    let log = sessions::path(name);
    // "last" is the unnamed session, so only a name from `-c` shows.
    let named = (name != sessions::LAST).then_some(name);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(named.map_or("zil".into(), |s| format!("zil · {s}"))));
    let tag = match named {
        Some(s) if log.as_ref().is_some_and(|l| l.exists()) => format!("session {s} · "),
        Some(s) => format!("new session {s} · "),
        None => String::new(),
    };
    let (dim, reset) = if color_on() { (Role::Dim.code(), RESET) } else { ("", "") };
    println!("zil {} {dim}· {tag}help for docs, Alt-Q to set a line aside, exit or Ctrl-D to quit{reset}", env!("CARGO_PKG_VERSION"));
    if let Some(a) = archived {
        println!("{dim}saved the last session as {a}; zil -c {a} reopens it{reset}");
    }
    println!("{}\n", help::tip(color_on()));
    let mut n = 0;
    let mut saved = Vec::new();
    if let Some(old) = log.as_ref().and_then(|l| std::fs::read_to_string(l).ok()) {
        for src in sessions::inputs(&old) {
            for (i, line) in src.lines().enumerate() {
                let prompt = if i == 0 { "> " } else { ". " };
                if color_on() {
                    println!("{}{prompt}{RESET}{}", Role::Prompt.code(), highlight(line));
                } else {
                    println!("{prompt}{line}");
                }
            }
            if eval(&mut session.borrow_mut(), &mut n, src) {
                saved.push(src.to_string());
            }
        }
        save(&log, &saved);
    }
    let mut buf = String::new();
    loop {
        let line = match rl.readline(if buf.is_empty() { "> " } else { ". " }) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                if let Some(line) = stashed.lock().unwrap().take() {
                    buf.push_str(&line);
                    if !buf.trim().is_empty() {
                        let _ = rl.add_history_entry(buf.trim_end());
                    }
                }
                buf.clear();
                continue;
            }
            Err(_) => break,
        };
        buf.push_str(&line);
        buf.push('\n');
        if unclosed(&buf) {
            continue;
        }
        let src = std::mem::take(&mut buf);
        if src.trim().is_empty() {
            continue;
        }
        if matches!(src.trim(), "exit" | "quit") {
            break;
        }
        match src.trim() {
            "clear" => {
                let _ = rl.clear_screen();
                continue;
            }
            "clear history" => {
                let _ = rl.clear_history();
                continue;
            }
            "clear results" => {
                repl::forget(&mut session.borrow_mut(), &mut n);
                continue;
            }
            c @ ("clear session" | "clear all") => {
                if sessions::confirm(&format!("{c}? This forgets every variable, function and unit.")) {
                    repl::reset(&mut session.borrow_mut(), &mut n);
                    saved.clear();
                    save(&log, &saved);
                    if c == "clear all" {
                        let _ = rl.clear_history();
                        let _ = rl.clear_screen();
                    }
                }
                println!();
                continue;
            }
            _ => {}
        }
        let _ = rl.add_history_entry(src.trim_end());
        if eval(&mut session.borrow_mut(), &mut n, &src) {
            saved.push(src.trim_end().to_string());
            save(&log, &saved);
        }
    }
    if let Some(h) = &history {
        let _ = std::fs::create_dir_all(h.parent().unwrap());
        let _ = rl.save_history(h);
    }
}

/// Runs one input and prints its result; true if it ran without error and isn't help, so `--continue` should replay it.
fn eval(session: &mut Session, n: &mut usize, src: &str) -> bool {
    let src = repl::help_sugar(src).into_owned();
    session.limits().interrupt.store(false, std::sync::atomic::Ordering::Relaxed);
    // Printed directly, so it keeps its colors.
    if let Some(topic) = help_shorthand(&src) {
        pager::page(&help::live(help_term(&src), || help::help(topic, color_on())).unwrap_or_else(|e| e));
        println!();
        return false;
    }
    let ok = match help::live(help_term(&src), || session.eval(&src)) {
        Ok(v) => {
            match repl::record(session, n, &src, &v) {
                Shown::Hidden => {}
                Shown::Bare if help_term(&src).is_some() => pager::page(&v.to_string()),
                Shown::Bare => println!("{v}"),
                Shown::Numbered(k) => println!("{}", repl::show(k, &v, color_on())),
                Shown::Named(k, pairs) => println!("{}", repl::show_named(k, &pairs, color_on())),
                Shown::Defined(sig) if color_on() => println!("{}{sig}{RESET}", Role::Dim.code()),
                Shown::Defined(sig) => println!("{sig}"),
            }
            help_term(&src).is_none()
        }
        Err(e) => {
            report("<repl>", &src, e);
            false
        }
    };
    println!();
    ok
}

/// Rewrites the session file: the saved inputs, each ended by a NUL since one input can span lines.
fn save(log: &Option<std::path::PathBuf>, saved: &[String]) {
    if let Some(l) = log {
        let _ = std::fs::create_dir_all(l.parent().unwrap());
        let _ = std::fs::write(l, saved.iter().map(|s| format!("{s}\0")).collect::<String>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::completion::Completer;

    #[test]
    fn completes_names_at_cursor() {
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let session = Rc::new(RefCell::new(Session::new(zil::host::get())));
        let names = Names(Rc::clone(&session));
        assert_eq!(names.complete("x.upp", 5, &ctx).unwrap(), (2, vec!["upper".to_string()]));
        assert!(names.complete("5 kilo", 6, &ctx).unwrap().1.contains(&"kilometers".to_string()));
        assert!(names.complete("x ", 2, &ctx).unwrap().1.is_empty());

        let to = |line: &str| names.complete(line, line.len(), &ctx).unwrap().1;
        let length = to("5 km to ");
        assert!(length.contains(&"mi".into()) && length.contains(&"ly".into()));
        assert!(!length.contains(&"kg".into()) && !length.contains(&"kilometers".into()));
        assert_eq!(to("print(5 km to m"), ["m", "mm", "mi", "marathon"]);
        assert_eq!(to("5 mph to ")[..6], ["kph", "mph", "m/s", "kn", "mach", "ft/s"]);
        assert_eq!(to("5 mph "), ["to"]);
        assert!(to("5 ").is_empty() && to("5 mph t").contains(&"true".into()) && !to("i").contains(&"if".into()));
        assert!(to("y = 60 km/h to ").contains(&"mph".into()));
        session.borrow_mut().eval("x = 3 kg").unwrap();
        assert!(to("x to ").contains(&"g".into()));
        assert!(to("z to ").is_empty());

        session.borrow_mut().eval("my_total = 1; my_fn = |x| x").unwrap();
        assert_eq!(to("my_"), ["my_fn", "my_total"]);
        assert_eq!(to("x.my_"), ["my_fn"]);
        let t = std::time::Instant::now();
        assert!(to("(fn() { while true {} })().").contains(&"upper".into()) && t.elapsed().as_secs_f64() < 1.0);
        assert!(!to("x.").contains(&"km".into()) && to("x.").contains(&"round".into()) && !to("x.").contains(&"upper".into()));
    }

    #[test]
    fn hints_and_highlights() {
        use rustyline::highlight::Highlighter;
        use rustyline::hint::Hinter;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let names = Names(Rc::new(RefCell::new(Session::new(zil::host::get()))));
        assert_eq!(names.hint("x.upp", 5, &ctx).as_deref(), Some("er"));
        assert_eq!(names.hint("x.upp", 3, &ctx), None);
        assert_eq!(names.hint("5 mph ", 6, &ctx).as_deref(), Some("to"));
        let line = names.highlight("if x 5 \"s\"", 0);
        assert_eq!(line, format!("{}if{RESET} x {}5{RESET} {}\"s\"{RESET}", Role::Keyword.code(), Role::Number.code(), Role::Str.code()));
    }
}
