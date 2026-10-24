//! Errors, how they print, and did-you-mean suggestions.

use crate::lexer::Span;
use crate::value::Value;
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};

/// A located error with labels, `note:` and `help:` lines.
///
/// Style:
/// - The title says what went wrong in general; the labels say what's at each place. Never repeat the title as a label.
/// - Messages are lowercase with no trailing period. Code, names and values go in backticks.
/// - Show the actual value or type you got, and what was expected.
/// - When the fix is obvious, add a `help:` line with it; add a `note:` line for context (sizes, valid ranges).
/// - Builtin errors start with the function name: `upper: expected str, got int`.
///
/// String errors from builtins and hooks can end in `\nnote: ...` and `\nhelp: ...` lines; `Error::new` splits them off.
#[derive(Debug, Clone)]
pub struct Error {
    pub msg: String,
    /// The first is the primary location; an empty text just underlines.
    pub labels: Vec<(Span, String)>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
    /// Set when a session limit stopped the run, so embedders can tell it from an error in the code.
    pub limit: Option<Limit>,
}

/// Which of `Interp::limits` was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Steps,
    Time,
    Interrupt,
    Depth,
    Len,
    Bits,
    Output,
}

impl Error {
    pub fn new(msg: impl Into<String>, span: Span) -> Error {
        let mut e = Error { msg: String::new(), labels: vec![(span, String::new())], notes: vec![], help: vec![], limit: None };
        for line in msg.into().split('\n') {
            match (line.strip_prefix("note: "), line.strip_prefix("help: ")) {
                (Some(n), _) => e.notes.push(n.into()),
                (_, Some(h)) => e.help.push(h.into()),
                _ if e.msg.is_empty() => e.msg = line.into(),
                _ => e.msg = format!("{}\n{line}", e.msg),
            }
        }
        e
    }

    /// Labels `span`; a label already on that span gets the text instead.
    pub fn label(mut self, span: Span, text: impl Into<String>) -> Error {
        match self.labels.iter_mut().find(|l| l.0 == span) {
            Some(l) => l.1 = text.into(),
            None => self.labels.push((span, text.into())),
        }
        self
    }

    pub fn note(mut self, text: impl Into<String>) -> Error {
        self.notes.push(text.into());
        self
    }

    pub fn help(mut self, text: impl Into<String>) -> Error {
        self.help.push(text.into());
        self
    }

    pub fn span(&self) -> Span {
        self.labels[0].0.clone()
    }

    /// Where the error starts in `src`: 1-based line and column, the column counted in characters.
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        let before = src.get(..self.span().start).unwrap_or(src);
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        (before.matches('\n').count() + 1, before[line_start..].chars().count() + 1)
    }

    /// The report for this error in `src` (called `name`), as the terminal shows it: ANSI colored if `color`, else plain.
    pub fn render(&self, name: &str, src: &str, color: bool) -> String {
        render(self, name, src, color)
    }
}

fn render(e: &Error, name: &str, src: &str, color: bool) -> String {
    let labels = e.labels.iter().map(|(span, text)| Label::new((name, span.clone())).with_color(Color::Red).with_message(text));
    let config = Config::default().with_color(color).with_index_type(IndexType::Byte);
    let mut r = Report::build(ReportKind::Error, (name, e.span())).with_config(config).with_message(&e.msg).with_labels(labels);
    for n in &e.notes {
        r = r.with_note(n);
    }
    for h in &e.help {
        r = r.with_help(h);
    }
    let mut out = Vec::new();
    let _ = r.finish().write((name, Source::from(src)), &mut out);
    String::from_utf8_lossy(&out).into_owned()
}

/// Up to three of `names` a typo or two from `word`, closest first.
pub fn near<'a>(word: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let max = (word.chars().count() / 3).max(1);
    let mut hits: Vec<_> = names.into_iter().filter(|n| *n != word).map(|n| (edits(word, n), n)).filter(|h| h.0 <= max).collect();
    hits.sort();
    hits.dedup();
    hits.into_iter().take(3).map(|h| h.1).collect()
}

/// `"\nhelp: did you mean `upper`?"`, to append to a message; empty when nothing is close.
pub fn did_you_mean<'a>(word: &str, names: impl IntoIterator<Item = &'a str>) -> String {
    let hits: Vec<_> = near(word, names).iter().map(|n| format!("`{n}`")).collect();
    match hits.as_slice() {
        [] => String::new(),
        [a] => format!("\nhelp: did you mean {a}?"),
        [rest @ .., last] => format!("\nhelp: did you mean {} or {last}?", rest.join(", ")),
    }
}

/// Edit distance, counting a swap of two neighbors as one edit (`nmae` is one from `name`).
pub fn edits(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    d[0] = (0..=b.len()).collect();
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// `n argument(s)`.
pub fn args_word(n: usize) -> String {
    format!("{n} argument{}", if n == 1 { "" } else { "s" })
}

/// A value for a label: `` `5 km` ``, cut short if long.
pub fn short(v: &Value) -> String {
    let s = format!("{v:?}");
    let s = if s.chars().count() > 40 { format!("{}...", s.chars().take(37).collect::<String>()) } else { s };
    format!("`{s}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions() {
        assert_eq!(edits("kitten", "sitting"), 3);
        assert_eq!(edits("nmae", "name"), 1);
        assert_eq!(did_you_mean("uper", ["upper", "lower", "zzz"]), "\nhelp: did you mean `upper`?");
        assert_eq!(did_you_mean("qqqq", ["upper"]), "");
        let e = Error::new("bad\nnote: n\nhelp: h", 0..1);
        assert_eq!((e.msg.as_str(), e.notes, e.help), ("bad", vec!["n".to_string()], vec!["h".to_string()]));
        let src = "x = 1\ny = \"é\" + )";
        assert_eq!(crate::parser::parse(src).unwrap_err().line_col(src), (2, 11));
        assert_eq!(Error::new("past the end", 99..99).line_col("a\nb"), (2, 2));
    }

    /// `src/errors.snap`: `>>> source` lines, each followed by its plain report. `UPDATE=1 cargo test` rewrites the reports.
    #[test]
    fn snapshots() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/errors.snap");
        let snap = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        let sources: Vec<String> = snap.lines().filter_map(|l| l.strip_prefix(">>> ")).map(String::from).collect();
        let run = move || {
            let mut out = String::new();
            for src in &sources {
                let src = src.replace("\\n", "\n");
                let report = match crate::run(&mut crate::interp::Interp::new(), &src) {
                    Ok(v) => format!("ok: {v:?}\n"),
                    Err(e) => e.render("<-e>", &src, false),
                };
                out += &format!(">>> {}\n{}\n", src.replace('\n', "\\n"), report.trim_end());
            }
            out
        };
        let out = std::thread::Builder::new().stack_size(crate::STACK).spawn(run).unwrap().join().unwrap();
        if std::env::var_os("UPDATE").is_some() {
            std::fs::write(path, &out).unwrap();
        } else {
            assert_eq!(snap, out, "error reports changed; rerun with UPDATE=1 to accept");
        }
    }
}
