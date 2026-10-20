//! A full-screen editor for `edit(...)`, styled like the pager: type or paste, then Ctrl-S returns the text and Esc, Ctrl-Q or
//! Ctrl-C cancel. Ctrl-K cuts lines (in a row, they pile up) and Ctrl-U pastes them back, Ctrl-Z/Ctrl-Y undo and redo, Ctrl-F
//! finds (F3 finds again), the mouse wheel scrolls and a click moves the cursor.

use crate::pager::Screen;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use crossterm::{cursor, queue, terminal};
use std::io::{self, IsTerminal, Write, stdout};
use zil::ansi::Role;

type Lines = Vec<Vec<char>>;

/// The text being edited, its cursor, undo history and cut lines.
struct Buf {
    lines: Lines,
    row: usize,
    col: usize,
    undo: Vec<(Lines, usize, usize)>,
    redo: Vec<(Lines, usize, usize)>,
    /// The last edit was a typed char, so the next one joins its undo step.
    typing: bool,
    cut: Lines,
    /// The last key cut a line, so the next cut adds to it.
    cutting: bool,
}

impl Buf {
    fn new(text: &str) -> Buf {
        let lines = text.split('\n').map(|l| l.trim_end_matches('\r').chars().collect()).collect();
        Buf { lines, row: 0, col: 0, undo: vec![], redo: vec![], typing: false, cut: vec![], cutting: false }
    }

    fn text(&self) -> String {
        self.lines.iter().map(|l| l.iter().collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    /// Remembers the text before an edit, for undo.
    // ponytail: whole-buffer snapshots; diffs if someone edits megabytes.
    fn save(&mut self, typing: bool) {
        if !(typing && self.typing) {
            self.undo.push((self.lines.clone(), self.row, self.col));
        }
        self.redo.clear();
        self.typing = typing;
    }

    /// Inserts `c` at the cursor without touching undo; `\n` splits the line.
    fn put(&mut self, c: char) {
        if c == '\n' {
            let rest = self.lines[self.row].split_off(self.col);
            self.row += 1;
            self.lines.insert(self.row, rest);
            self.col = 0;
        } else {
            self.lines[self.row].insert(self.col, c);
            self.col += 1;
        }
    }

    fn type_char(&mut self, c: char) {
        self.save(true);
        self.put(c);
    }

    /// A paste, or several keys at once: one undo step, with `\r\n` and tabs evened out.
    fn insert(&mut self, s: &str) {
        self.save(false);
        for c in s.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ").chars() {
            self.put(c);
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.save(false);
            self.col -= 1;
            self.lines[self.row].remove(self.col);
        } else if self.row > 0 {
            self.save(false);
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].len();
            self.lines[self.row].extend(line);
        }
    }

    fn delete(&mut self) {
        if self.col < self.lines[self.row].len() {
            self.save(false);
            self.lines[self.row].remove(self.col);
        } else if self.row + 1 < self.lines.len() {
            self.save(false);
            let line = self.lines.remove(self.row + 1);
            self.lines[self.row].extend(line);
        }
    }

    /// Ctrl-K: cuts the cursor's line, adding to the last cut when `more`.
    fn cut_line(&mut self, more: bool) {
        self.save(false);
        if !more {
            self.cut.clear();
        }
        if self.lines.len() == 1 {
            self.cut.push(std::mem::take(&mut self.lines[0]));
        } else {
            self.cut.push(self.lines.remove(self.row));
            self.row = self.row.min(self.lines.len() - 1);
        }
        self.col = 0;
        self.cutting = true;
    }

    /// Ctrl-U: pastes the cut lines at the cursor.
    fn uncut(&mut self) {
        if !self.cut.is_empty() {
            let text: String = self.cut.iter().map(|l| l.iter().collect::<String>() + "\n").collect();
            self.insert(&text);
        }
    }

    fn undo(&mut self, back: bool) {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some((lines, row, col)) = from.pop() {
            to.push((std::mem::replace(&mut self.lines, lines), self.row, self.col));
            (self.row, self.col) = (row, col);
        }
        self.typing = false;
    }

    /// Moves to `row`, keeping the column where the line allows.
    fn go(&mut self, row: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.col = self.col.min(self.lines[self.row].len());
    }

    fn left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].len();
        }
    }

    fn right(&mut self) {
        if self.col < self.lines[self.row].len() {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    /// Moves to the next case-insensitive `query` after the cursor, wrapping around the end; false if there's none.
    fn find(&mut self, query: &str) -> bool {
        let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase().next()).collect();
        if q.is_empty() {
            return false;
        }
        let n = self.lines.len();
        for i in 0..=n {
            let r = (self.row + i) % n;
            let line: Vec<char> = self.lines[r].iter().flat_map(|c| c.to_lowercase().next()).collect();
            let from = if i == 0 { self.col + 1 } else { 0 };
            let to = if i == n { self.col + 1 } else { line.len() + 1 };
            if let Some(c) = (from..to.min(line.len() + 1)).find(|&c| line[c..].starts_with(&q)) {
                (self.row, self.col) = (r, c);
                return true;
            }
        }
        false
    }
}

/// Edits `text` full-screen and returns it when saved; errors if cancelled or there's no terminal to edit on.
pub fn edit(text: &str) -> io::Result<String> {
    if !io::stdin().is_terminal() || !stdout().is_terminal() {
        return Err(io::Error::other("needs a terminal"));
    }
    let _screen = Screen::enter(true)?;
    let mut b = Buf::new(text);
    let (mut top, mut left, mut follow) = (0usize, 0usize, true);
    let (mut query, mut finding, mut note, mut quitting) = (String::new(), None::<String>, String::new(), false);
    loop {
        let (w, h) = terminal::size()?;
        let (w, rows) = ((w as usize).max(1), (h as usize).saturating_sub(1).max(1));
        // Scroll to the cursor after a key; the wheel leaves it behind.
        if follow {
            top = top.clamp(b.row.saturating_sub(rows - 1), b.row);
            left = left.clamp(b.col.saturating_sub(w - 1), b.col);
        }
        top = top.min(b.lines.len().saturating_sub(1));
        draw(&b, top, left, rows, w, finding.as_deref(), &note)?;
        let ev = event::read()?;
        note.clear();
        if let Some(q) = &mut finding {
            match ev {
                Event::Key(KeyEvent { code, kind: KeyEventKind::Press, modifiers, .. }) => match code {
                    KeyCode::Char('c' | 'q') if modifiers.contains(KeyModifiers::CONTROL) => finding = None,
                    KeyCode::Char(c) => q.push(c),
                    KeyCode::Backspace if q.pop().is_none() => finding = None,
                    KeyCode::Esc => finding = None,
                    KeyCode::Enter => {
                        query = finding.take().unwrap();
                        if !query.is_empty() && !b.find(&query) {
                            note = format!("no match for \"{query}\"");
                        }
                        follow = true;
                    }
                    _ => {}
                },
                Event::Paste(s) => q.push_str(&s.replace(['\r', '\n'], " ")),
                _ => {}
            }
            continue;
        }
        let more = std::mem::take(&mut b.cutting);
        let was_quitting = std::mem::take(&mut quitting);
        match ev {
            Event::Key(KeyEvent { code, kind: KeyEventKind::Press, modifiers, .. }) => {
                follow = true;
                // AltGr arrives as Ctrl+Alt on Windows, and types a char.
                let ctrl = modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT);
                if !matches!(code, KeyCode::Char(_)) || ctrl {
                    b.typing = false;
                }
                match code {
                    KeyCode::Char('s') if ctrl => return Ok(b.text()),
                    KeyCode::Char('q' | 'c') if ctrl => quitting = true,
                    KeyCode::Esc => quitting = true,
                    KeyCode::Char('k') if ctrl => b.cut_line(more),
                    KeyCode::Char('u') if ctrl => b.uncut(),
                    KeyCode::Char('z') if ctrl => b.undo(true),
                    KeyCode::Char('y') if ctrl => b.undo(false),
                    KeyCode::Char('f') if ctrl => finding = Some(String::new()),
                    KeyCode::F(3) if query.is_empty() => note = "press Ctrl-F to find first".into(),
                    KeyCode::F(3) if !b.find(&query) => note = format!("no match for \"{query}\""),
                    KeyCode::Char(_) if ctrl => {}
                    KeyCode::Char(c) => b.type_char(c),
                    KeyCode::Tab => b.insert("\t"),
                    KeyCode::Enter => b.insert("\n"),
                    KeyCode::Backspace => b.backspace(),
                    KeyCode::Delete => b.delete(),
                    KeyCode::Left => b.left(),
                    KeyCode::Right => b.right(),
                    KeyCode::Up => b.go(b.row.saturating_sub(1)),
                    KeyCode::Down => b.go(b.row + 1),
                    KeyCode::PageUp => b.go(b.row.saturating_sub(rows)),
                    KeyCode::PageDown => b.go(b.row + rows),
                    KeyCode::Home if ctrl => (b.row, b.col) = (0, 0),
                    KeyCode::End if ctrl => b.go(usize::MAX),
                    KeyCode::Home => b.col = 0,
                    KeyCode::End => b.col = b.lines[b.row].len(),
                    _ => {}
                }
                // Cancelling throws away what was typed, so a changed buffer asks twice.
                if quitting && (was_quitting || b.undo.is_empty()) {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                }
                if quitting {
                    note = "unsaved changes: press Esc again to discard, Ctrl-S to save".into();
                }
            }
            Event::Paste(s) => {
                follow = true;
                b.insert(&s);
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => (top, follow) = (top.saturating_sub(3), false),
                MouseEventKind::ScrollDown => (top, follow) = (top + 3, false),
                MouseEventKind::Down(MouseButton::Left) if (m.row as usize) < rows => {
                    b.row = (top + m.row as usize).min(b.lines.len() - 1);
                    b.col = (left + m.column as usize).min(b.lines[b.row].len());
                    b.typing = false;
                    follow = true;
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn draw(b: &Buf, top: usize, left: usize, rows: usize, width: usize, finding: Option<&str>, note: &str) -> io::Result<()> {
    let mut out = stdout();
    queue!(out, terminal::BeginSynchronizedUpdate)?;
    // ponytail: one column per char; wide (CJK, emoji) chars push the rest of their line off by one each.
    for r in 0..rows {
        let line: String = b.lines.get(top + r).map_or(String::new(), |l| l.iter().skip(left).take(width).collect());
        queue!(out, cursor::MoveTo(0, r as u16))?;
        write!(out, "{line}")?;
        queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    }
    let status = match finding {
        Some(q) => format!("find: {q}"),
        None if !note.is_empty() => format!(" {note}"),
        None => format!(
            " line {}/{}, col {}{}   ^S save · Esc cancel · ^K cut ^U uncut · ^Z ^Y undo redo · ^F F3 find",
            b.row + 1,
            b.lines.len(),
            b.col + 1,
            if b.undo.is_empty() { "" } else { " (modified)" }
        ),
    };
    let status: String = status.chars().take(width).collect();
    queue!(out, cursor::MoveTo(0, rows as u16))?;
    write!(out, "{}{status:<width$}\x1b[0m", Role::StatusBar.code())?;
    let (x, y) = (b.col.wrapping_sub(left), b.row.wrapping_sub(top));
    if finding.is_some() {
        queue!(out, cursor::MoveTo(status.chars().count() as u16, rows as u16), cursor::Show)?;
    } else if x < width && y < rows {
        queue!(out, cursor::MoveTo(x as u16, y as u16), cursor::Show)?;
    } else {
        queue!(out, cursor::Hide)?;
    }
    queue!(out, terminal::EndSynchronizedUpdate)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_cuts_undoes_and_finds() {
        let mut b = Buf::new("one\r\ntwo");
        b.go(1);
        b.col = 3;
        b.insert("\r\nthree\tx");
        assert_eq!(b.text(), "one\ntwo\nthree    x");
        b.type_char('y');
        b.type_char('z');
        b.undo(true);
        assert_eq!(b.text(), "one\ntwo\nthree    x");
        b.undo(false);
        assert_eq!(b.text(), "one\ntwo\nthree    xyz");
        b.go(0);
        b.cut_line(false);
        b.cut_line(true);
        assert_eq!(b.text(), "three    xyz");
        b.go(usize::MAX);
        b.col = 0;
        b.uncut();
        assert_eq!(b.text(), "one\ntwo\nthree    xyz");
        b.col = 0;
        b.backspace();
        assert_eq!((b.text().as_str(), b.row, b.col), ("one\ntwothree    xyz", 1, 3));
        b.delete();
        assert_eq!(b.text(), "one\ntwohree    xyz");
        (b.row, b.col) = (0, 0);
        assert!(b.find("O") && (b.row, b.col) == (1, 2));
        assert!(b.find("o") && (b.row, b.col) == (0, 0));
        assert!(!b.find("nope"));
        assert!(b.find("one") && (b.row, b.col) == (0, 0));
    }
}
