//! A full-screen pager for long REPL help and `page(...)`: arrows, PgUp/PgDn, Space, Home/End and the mouse wheel scroll, `/` searches
//! (`n`/`N` step through matches), dragging the mouse copies, `q` or Esc quits and leaves the page being read in the scrollback.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use crossterm::{cursor, execute, queue, terminal};
use std::io::{Write, stdout};
use zil::ansi::{Role, strip_ansi};
use zil::host::Host;

/// A mouse selection: where the drag started and where it is now, as (line, column).
type Sel = ((usize, usize), (usize, usize));

/// Puts the terminal back however the pager (or editor) ends, panics included; `paste` turns on bracketed paste.
pub struct Screen(bool);

impl Screen {
    pub fn enter(paste: bool) -> std::io::Result<Screen> {
        terminal::enable_raw_mode()?;
        // `?7l`: lines too wide after a resize are cut off rather than wrapped, which would push the status bar away.
        execute!(stdout(), terminal::EnterAlternateScreen, event::EnableMouseCapture, cursor::Hide, crossterm::style::Print("\x1b[?7l"))?;
        // Not every console has it; without, a paste arrives as typed keys.
        if paste {
            let _ = execute!(stdout(), event::EnableBracketedPaste);
        }
        Ok(Screen(paste))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if self.0 {
            let _ = execute!(stdout(), event::DisableBracketedPaste);
        }
        let _ = execute!(stdout(), crossterm::style::Print("\x1b[?7h"), cursor::Show, event::DisableMouseCapture, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Prints `text`, paging it when it's taller than the terminal.
pub fn page(text: &str) {
    // A blank line above and below, so the page doesn't butt against the prompt or the status bar.
    let lines: Vec<&str> = std::iter::once("").chain(text.lines()).chain(std::iter::once("")).collect();
    let text = &lines.join("\n");
    let fits = !std::io::IsTerminal::is_terminal(&stdout()) || terminal::size().map_or(true, |(_, h)| lines.len() < h as usize);
    if fits {
        println!("{text}");
        return;
    }
    match browse(&lines) {
        // Leave what was on screen behind, so it can be read while typing the next line.
        Ok(top) => {
            let rows = terminal::size().map_or(lines.len(), |(_, h)| (h as usize).saturating_sub(1).max(1));
            println!("{}", lines[top..lines.len().min(top + rows)].join("\n"));
        }
        Err(_) => println!("{text}"),
    }
}

/// The pager loop; returns the top line being shown when it was quit.
fn browse(lines: &[&str]) -> std::io::Result<usize> {
    let _screen = Screen::enter(false)?;
    let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l).to_lowercase()).collect();
    let (mut top, mut query, mut typing, mut note) = (0usize, String::new(), None::<String>, String::new());
    let (mut shown, mut sel, mut anchor) = (None, None::<Sel>, (0, 0));
    loop {
        let (w, h) = terminal::size()?;
        let rows = (h as usize).saturating_sub(1).max(1);
        let max_top = lines.len().saturating_sub(rows);
        top = top.min(max_top);
        // Repaint only on change: scrolling against either end would otherwise redraw the same screen and flicker.
        let state = Some((top, w, h, query.clone(), typing.clone(), note.clone(), sel));
        if state != shown {
            draw(lines, top, rows, w as usize, &query, typing.as_deref(), &note, sel)?;
            shown = state;
        }
        let ev = event::read()?;
        // Mouse motion is reported too; it shouldn't wipe a note, like the one a copy leaves.
        if !matches!(ev, Event::Mouse(event::MouseEvent { kind: MouseEventKind::Moved, .. })) {
            note.clear();
        }
        // Typing a search: keys edit the query until Enter or Esc.
        if let Some(q) = &mut typing {
            if let Event::Key(KeyEvent { code, kind: KeyEventKind::Press, modifiers, .. }) = ev {
                match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => typing = None,
                    KeyCode::Char(c) => q.push(c),
                    KeyCode::Backspace if q.pop().is_none() => typing = None,
                    KeyCode::Esc => typing = None,
                    KeyCode::Enter => {
                        query = typing.take().unwrap().to_lowercase();
                        if !query.is_empty() {
                            match find(&plain, &query, top, true) {
                                Some(i) => top = i,
                                None => note = format!("no match for \"{query}\""),
                            }
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }
        let step = |n: &str| if query.is_empty() { "press / to search first".to_string() } else { format!("no more matches for \"{n}\"") };
        match ev {
            Event::Key(KeyEvent { code, kind: KeyEventKind::Press, modifiers, .. }) => match code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(top),
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(top),
                KeyCode::Up => top = top.saturating_sub(1),
                KeyCode::Down | KeyCode::Enter => top += 1,
                KeyCode::PageUp => top = top.saturating_sub(rows),
                KeyCode::PageDown | KeyCode::Char(' ') => top += rows,
                KeyCode::Home => top = 0,
                KeyCode::End => top = max_top,
                KeyCode::Char('/') => typing = Some(String::new()),
                KeyCode::Char('n') => match find(&plain, &query, top + 1, true) {
                    Some(i) => top = i,
                    None => note = step(&query),
                },
                KeyCode::Char('N') => match find(&plain, &query, top, false) {
                    Some(i) => top = i,
                    None => note = step(&query),
                },
                _ => {}
            },
            Event::Mouse(m) => {
                let at = ((top + (m.row as usize).min(rows - 1)).min(lines.len().saturating_sub(1)), m.column as usize);
                match m.kind {
                    MouseEventKind::ScrollUp => top = top.saturating_sub(3),
                    MouseEventKind::ScrollDown => top += 3,
                    MouseEventKind::Down(MouseButton::Left) => (anchor, sel) = (at, None),
                    MouseEventKind::Drag(MouseButton::Left) => sel = Some((anchor, at)),
                    MouseEventKind::Up(MouseButton::Left) if sel.is_some() => {
                        let text = selected(lines, sel.unwrap());
                        // ponytail: copies synchronously, so the bar waits on the clipboard tool (PowerShell on Windows); thread it if that lag bothers.
                        note = match crate::native::NativeHost.copy_text(&text) {
                            Ok(()) => format!("{} characters copied to clipboard", text.chars().count()),
                            Err(e) => format!("couldn't copy: {e}"),
                        };
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// The columns `sel` covers on line `i`, end exclusive, if it reaches that line.
fn cols(sel: Sel, i: usize) -> Option<(usize, usize)> {
    let (a, b) = if sel.0 <= sel.1 { sel } else { (sel.1, sel.0) };
    (a.0..=b.0).contains(&i).then(|| (if i == a.0 { a.1 } else { 0 }, if i == b.0 { b.1 + 1 } else { usize::MAX }))
}

/// The text `sel` covers, without colors.
fn selected(lines: &[&str], sel: Sel) -> String {
    let (from, to) = (sel.0.0.min(sel.1.0), sel.0.0.max(sel.1.0));
    let part = |i: usize| {
        let (s, e) = cols(sel, i).unwrap();
        strip_ansi(lines[i]).chars().skip(s).take(e - s).collect::<String>()
    };
    (from..=to).map(part).collect::<Vec<_>>().join("\n")
}

/// The first line holding `query` from `from` on (or before `from`, going back), wrapping around the end.
fn find(plain: &[String], query: &str, from: usize, forward: bool) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    let n = plain.len();
    (0..n).map(|i| if forward { (from + i) % n } else { (from + n - 1 - i) % n }).find(|&i| plain[i].contains(query))
}

#[allow(clippy::too_many_arguments)]
fn draw(lines: &[&str], top: usize, rows: usize, width: usize, query: &str, typing: Option<&str>, note: &str, sel: Option<Sel>) -> std::io::Result<()> {
    let mut out = stdout();
    queue!(out, terminal::BeginSynchronizedUpdate)?;
    // Each line is written over the old one, then the rest cleared, so nothing is ever blank on screen.
    for (r, line) in lines[top..].iter().chain(std::iter::repeat(&"")).take(rows).enumerate() {
        queue!(out, cursor::MoveTo(0, r as u16))?;
        let line = match sel.and_then(|s| cols(s, top + r)) {
            Some(span) => reverse(line, &[span]),
            None => line.to_string(),
        };
        write!(out, "{}\x1b[0m", mark(&line, query))?;
        queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    }
    let end = lines.len().min(top + rows);
    let status = match typing {
        Some(q) => format!("/{q}"),
        None if !note.is_empty() => format!(" {note}"),
        None => {
            let pct = end * 100 / lines.len().max(1);
            format!(" lines {}-{end} of {} ({pct}%)   ↑↓ PgUp PgDn wheel scroll · / search · n N next/prev · drag copy · q quit", top + 1, lines.len())
        }
    };
    let status: String = status.chars().take(width).collect();
    // Padded to full width, so it covers the old bar without clearing first.
    queue!(out, cursor::MoveTo(0, rows as u16))?;
    write!(out, "{}{status:<width$}\x1b[0m", Role::StatusBar.code())?;
    if typing.is_some() {
        queue!(out, cursor::MoveTo(status.chars().count() as u16, rows as u16), cursor::Show)?;
    } else {
        queue!(out, cursor::Hide)?;
    }
    queue!(out, terminal::EndSynchronizedUpdate)?;
    out.flush()
}

/// `line` with each case-insensitive `query` match in reverse video, keeping its own colors.
fn mark(line: &str, query: &str) -> String {
    if query.is_empty() {
        return line.to_string();
    }
    let visible: Vec<char> = strip_ansi(line).chars().flat_map(|c| c.to_lowercase().next()).collect();
    let q: Vec<char> = query.chars().collect();
    let (mut spans, mut v) = (Vec::new(), 0);
    while v + q.len() <= visible.len() {
        if visible[v..v + q.len()] == q[..] {
            spans.push((v, v + q.len()));
            v += q.len();
        } else {
            v += 1;
        }
    }
    reverse(line, &spans)
}

/// `line` with the visible chars in each of `spans` (in order, end exclusive) in reverse video, keeping its own colors.
fn reverse(line: &str, spans: &[(usize, usize)]) -> String {
    // Byte offset of every visible char, skipping escape codes.
    let esc = regex::Regex::new("\x1b\\[[0-9;?]*[A-Za-z]").unwrap();
    let mut at = Vec::new();
    let mut i = 0;
    for m in esc.find_iter(line) {
        at.extend(line[i..m.start()].char_indices().map(|(j, _)| i + j));
        i = m.end();
    }
    at.extend(line[i..].char_indices().map(|(j, _)| i + j));
    at.push(line.len());
    let (mut out, mut last) = (String::new(), 0);
    for &(s, e) in spans {
        let (s, e) = (at[s.min(at.len() - 1)], at[e.min(at.len() - 1)]);
        if s == e {
            continue;
        }
        out.push_str(&line[last..s]);
        out.push_str(Role::SearchMatch.code());
        out.push_str(&line[s..e]);
        // ponytail: 27 undoes only reverse; a match style that isn't reverse needs the line's own style re-applied here.
        out.push_str("\x1b[27m");
        last = e;
    }
    out.push_str(&line[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_and_finds_matches() {
        assert_eq!(mark("\x1b[1mUpper\x1b[0m case UP", "up"), "\x1b[1m\x1b[7mUp\x1b[27mper\x1b[0m case \x1b[7mUP\x1b[27m");
        assert_eq!(mark("plain", ""), "plain");
        let plain: Vec<String> = ["a", "xy", "b", "xy"].iter().map(|s| s.to_string()).collect();
        assert_eq!(find(&plain, "xy", 2, true), Some(3));
        assert_eq!(find(&plain, "xy", 0, false), Some(3));
        assert_eq!(find(&plain, "xy", 3, false), Some(1));
        assert_eq!(find(&plain, "zz", 0, true), None);
        let lines = ["ab\x1b[1mcd", "efgh", "ij"];
        assert_eq!(selected(&lines, ((2, 0), (0, 2))), "cd\nefgh\ni");
        assert_eq!(reverse(lines[0], &[(1, 3)]), "a\x1b[7mb\x1b[1mc\x1b[27md");
        assert_eq!(reverse("ab", &[(1, usize::MAX)]), "a\x1b[7mb\x1b[27m");
    }
}
