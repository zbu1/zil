//! Terminal styling: the ANSI colors zil prints with, when to use them, and how to take them out again.

use crate::lexer::Tok;
use crate::value::Value;

/// What a colored bit of output is: the theme, not the call site, decides how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// Uncolored.
    Plain,
    Number,
    Str,
    Regex,
    Bool,
    Nil,
    Keyword,
    /// A command line's command names, and its flags.
    Command,
    Flag,
    /// Secondary text: hints, notes, table rules.
    Dim,
    Prompt,
    /// The `_n =` label on a REPL result.
    ResultLabel,
    TableHeader,
    Heading,
    ModulePath,
    Submodule,
    Name,
    Type,
    /// Titles of `help("advanced")` recipes.
    Recipe,
    /// Function groups on a module page, a step below headings.
    Group,
    /// Search matches, added on top of the text's own color.
    SearchMatch,
    StatusBar,
}

impl Role {
    /// The role's style in the default theme.
    pub fn code(self) -> &'static str {
        match self {
            Role::Plain => "",
            Role::Number => "[36m",
            Role::Str | Role::Regex | Role::Type => "[32m",
            Role::Bool | Role::Nil | Role::Keyword => "[35m",
            Role::Command => "[1;34m",
            Role::Flag => "[34m",
            Role::Dim | Role::Prompt | Role::ResultLabel => "[2m",
            Role::TableHeader => "[1m",
            Role::Heading => "[1;33m",
            Role::ModulePath => "[1;34m",
            Role::Submodule => "[34m",
            Role::Name => "[1;36m",
            Role::Recipe => "[1;35m",
            Role::Group => "[33m",
            Role::SearchMatch | Role::StatusBar => "[7m",
        }
    }
}

pub const RESET: &str = "[0m";

/// Whether to print colors: the host's output is a terminal and `NO_COLOR` is unset.
pub fn color_on() -> bool {
    let host = crate::host::get();
    host.terminal().is_some() && host.env("NO_COLOR").is_none()
}

/// Terminal width, or 80 without one.
pub fn columns() -> usize {
    crate::host::get().terminal().map_or(80, |(w, _)| w)
}

/// A value's role by type, as REPL results and help color it.
pub fn tint(v: &Value) -> Role {
    match v {
        Value::Int(..) | Value::Big(..) | Value::Frac(..) | Value::Float(_) | Value::Qty(..) | Value::Unc(..) | Value::Cplx(..) | Value::Mod(_) => Role::Number,
        Value::Str(_) | Value::Bytes(_) => Role::Str,
        Value::Regex(_) => Role::Regex,
        Value::Bool(_) => Role::Bool,
        Value::Nil => Role::Nil,
        _ => Role::Plain,
    }
}

/// `src` with numbers, strings and keywords colored, and in command lines the names, flags and quoted words; unlexable
/// bits stay plain.
pub fn highlight(src: &str) -> String {
    use crate::parser::Mark;
    let mut marks = vec![];
    let mut at = 0;
    for (line, words) in crate::parser::marks(src) {
        tokens(src, at..line.start, &mut marks);
        for (span, mark) in words {
            match mark {
                Mark::Name => marks.push((span, Role::Command)),
                Mark::Flags => marks.push((span, Role::Flag)),
                Mark::Str => marks.push((span, Role::Str)),
                Mark::Expr => tokens(src, span, &mut marks),
            }
        }
        at = line.end;
    }
    tokens(src, at..src.len(), &mut marks);
    let mut out = String::with_capacity(src.len() * 2);
    let mut last = 0;
    for (span, role) in marks {
        out.push_str(&src[last..span.start]);
        out.push_str(role.code());
        out.push_str(&src[span.clone()]);
        out.push_str(RESET);
        last = span.end;
    }
    out.push_str(&src[last..]);
    out
}

/// The colored tokens of `src[range]`, lexed as zil.
fn tokens(src: &str, range: std::ops::Range<usize>, out: &mut Vec<(std::ops::Range<usize>, Role)>) {
    use logos::Logos;
    for (tok, span) in Tok::lexer(&src[range.clone()]).spanned() {
        let role = match tok {
            Ok(Tok::Int(_) | Tok::Float(_) | Tok::Dec(_) | Tok::Dur(_) | Tok::Clock(_) | Tok::Based(_) | Tok::Currency(_)) => Role::Number,
            Ok(Tok::Str(_) | Tok::Bytes(_)) => Role::Str,
            Ok(Tok::Regex(_)) => Role::Regex,
            Ok(Tok::True | Tok::False) => Role::Bool,
            Ok(Tok::Nil) => Role::Nil,
            Ok(Tok::Fn | Tok::If | Tok::Else | Tok::While | Tok::For | Tok::In | Tok::To | Tok::Of | Tok::Mod | Tok::Return | Tok::Break | Tok::Continue) => {
                Role::Keyword
            }
            _ => continue,
        };
        out.push((range.start + span.start..range.start + span.end, role));
    }
}

/// Terminal codes that take no width: colors and cursor moves (`ESC [ ... X`), and hyperlinks (`ESC ] 8 ; ; url ESC \`).
pub static ESCAPES: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\x1b\[([0-9;?]*)([A-Za-z])|\x1b\]8;;([^\x1b]*)\x1b\\").expect("valid regex"));

/// Remove terminal color and cursor codes, and hyperlinks (keeping their text).
pub fn strip_ansi(s: &str) -> String {
    ESCAPES.replace_all(s, "").into_owned()
}

/// `text` as a terminal hyperlink (OSC 8) to `url`: the sandbox makes its `zil:` links clickable.
pub fn link(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Terminal text as HTML for the browser build: colors and styles become inline-styled spans, the 16 basic colors
/// `var(--ansi-N)` so the page can theme them, hyperlinks `<a data-zil="url">`; cursor codes vanish.
#[cfg(any(target_arch = "wasm32", test))]
pub fn to_html(s: &str) -> String {
    let re = &*ESCAPES;
    let basic = |n: u16| format!("var(--ansi-{n})");
    let (mut fg, mut bg, mut flags): (Option<String>, Option<String>, [bool; 5]) = (None, None, [false; 5]);
    // `css` is the style codes asked for, `open` the span written so far: spans change only where text follows,
    // so ariadne's char-by-char colors and reset-then-set pairs collapse into one span.
    let (mut out, mut last, mut open, mut css) = (String::new(), 0, String::new(), String::new());
    let mut linked = false;
    let text = |out: &mut String, open: &mut String, css: &str, t: &str| {
        if t.is_empty() {
            return;
        }
        if css != open {
            if !open.is_empty() {
                out.push_str("</span>");
            }
            if !css.is_empty() {
                *out += &format!("<span style=\"{css}\">");
            }
            *open = css.to_string();
        }
        // Glyphs past Latin-1 often come from a fallback font of another width, skewing boxes and columns; each run of
        // them is pinned to the cells a terminal gives it.
        let far = |c: char| c as u32 > 0xFF;
        let mut wide = false;
        for (i, c) in t.char_indices() {
            if far(c) != wide {
                wide = !wide;
                if wide {
                    let n: usize = t[i..].chars().take_while(|&c| far(c)).filter_map(unicode_width::UnicodeWidthChar::width).sum();
                    *out += &format!("<span style=\"display:inline-block;width:{n}ch\">");
                } else {
                    out.push_str("</span>");
                }
            }
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                c => out.push(c),
            }
        }
        if wide {
            out.push_str("</span>");
        }
    };
    for c in re.captures_iter(s) {
        let m = c.get_match();
        text(&mut out, &mut open, &css, &s[last..m.start()]);
        last = m.end();
        // A link closes the span around it, so spans and links nest; the text after opens a fresh one.
        if let Some(url) = c.get(3) {
            if !open.is_empty() {
                out.push_str("</span>");
                open.clear();
            }
            if url.is_empty() {
                if linked {
                    out.push_str("</a>");
                }
            } else {
                out += &format!("<a data-zil=\"{}\">", url.as_str().replace(['"', '<', '>', '&'], ""));
            }
            linked = !url.is_empty();
            continue;
        }
        if &c[2] != "m" {
            continue;
        }
        let ps: Vec<u16> = c[1].split(';').map(|p| p.parse().unwrap_or(0)).collect();
        let mut it = ps.iter().copied();
        while let Some(p) = it.next() {
            match p {
                0 => (fg, bg, flags) = (None, None, [false; 5]),
                // bold, dim, italic, underline, reverse
                1..=4 => flags[p as usize - 1] = true,
                7 => flags[4] = true,
                22 => (flags[0], flags[1]) = (false, false),
                23 | 24 => flags[p as usize - 21] = false,
                27 => flags[4] = false,
                30..=37 => fg = Some(basic(p - 30)),
                90..=97 => fg = Some(basic(p - 82)),
                40..=47 => bg = Some(basic(p - 40)),
                100..=107 => bg = Some(basic(p - 92)),
                39 => fg = None,
                49 => bg = None,
                38 | 48 => {
                    let color = match it.next() {
                        Some(5) => it.next().map(|n| match n {
                            0..=15 => basic(n),
                            16..=231 => {
                                let k = |x: u16| if x == 0 { 0 } else { 55 + 40 * x };
                                format!("rgb({},{},{})", k((n - 16) / 36), k((n - 16) / 6 % 6), k((n - 16) % 6))
                            }
                            _ => format!("rgb({0},{0},{0})", 8 + 10 * (n.min(255) - 232)),
                        }),
                        Some(2) => Some(format!("rgb({},{},{})", it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))),
                        _ => None,
                    };
                    *if p == 38 { &mut fg } else { &mut bg } = color;
                }
                _ => {}
            }
        }
        let (f, b) =
            if flags[4] { (bg.clone().or(Some("var(--ansi-bg)".into())), fg.clone().or(Some("var(--ansi-fg)".into()))) } else { (fg.clone(), bg.clone()) };
        css.clear();
        for (on, rule) in flags[..4].iter().zip(["font-weight:600", "opacity:.6", "font-style:italic", "text-decoration:underline"]) {
            if *on {
                css += rule;
                css.push(';');
            }
        }
        css += &f.map_or(String::new(), |c| format!("color:{c};"));
        css += &b.map_or(String::new(), |c| format!("background:{c};"));
    }
    text(&mut out, &mut open, &css, &s[last..]);
    if !open.is_empty() {
        out.push_str("</span>");
    }
    if linked {
        out.push_str("</a>");
    }
    out
}

/// Terminal columns `s` takes: color codes take none, wide glyphs two.
pub fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(strip_ansi(s).as_str())
}

/// `body` in a rounded box with `label` on the top edge; `body` may carry colors and wide glyphs.
pub fn boxed(label: &str, body: &str, dim: &str, reset: &str) -> String {
    let inner = body.lines().map(width).max().unwrap_or(0).max(width(label) + 2);
    let mut out = format!("{dim}╭─{reset} {label} {dim}{}╮{reset}\n", "─".repeat(inner - width(label) - 1));
    for line in body.lines() {
        out += &format!("{dim}│{reset} {line}{} {dim}│{reset}\n", " ".repeat(inner - width(line)));
    }
    out + &format!("{dim}╰{}╯{reset}", "─".repeat(inner + 2))
}

#[cfg(test)]
mod tests {
    #[test]
    fn highlights_command_lines() {
        use super::{RESET as R, Role::*, highlight};
        let c = |r: super::Role, s: &str| format!("{}{s}{R}", r.code());
        assert_eq!(
            highlight(r#"ls -la "a b" 5 {x + 1} | grep in"#),
            format!("{} {} {} 5 {{x + {}}} | {} in", c(Command, "ls"), c(Flag, "-la"), c(Str, "\"a b\""), c(Number, "1"), c(Command, "grep"))
        );
        assert_eq!(highlight("x = 5; !git log"), format!("x = {}; {} log", c(Number, "5"), c(Command, "!git")));
        assert_eq!(highlight("ls(\"a\")"), format!("ls({})", c(Str, "\"a\"")));
    }

    #[test]
    fn to_html() {
        assert_eq!(super::to_html("a < b"), "a &lt; b");
        assert_eq!(super::to_html("[31ma[0m[31mb[0m"), r#"<span style="color:var(--ansi-1);">ab</span>"#);
        assert_eq!(super::to_html("\x1b[1;36mx\x1b[0m y"), r#"<span style="font-weight:600;color:var(--ansi-6);">x</span> y"#);
        assert_eq!(
            super::to_html("\x1b[38;2;1;2;3m\x1b[48;5;196m▀\x1b[0m"),
            r#"<span style="color:rgb(1,2,3);background:rgb(255,0,0);"><span style="display:inline-block;width:1ch">▀</span></span>"#
        );
        assert_eq!(super::to_html("é ｗ𝐛→x"), r#"é <span style="display:inline-block;width:4ch">ｗ𝐛→</span>x"#);
        assert_eq!(super::to_html("\x1b[7mhit\x1b[27m\x1b[2K"), r#"<span style="color:var(--ansi-bg);background:var(--ansi-fg);">hit</span>"#);
        let link = super::link("zil:help/upper", "\x1b[36mupper\x1b[0m");
        assert_eq!(super::to_html(&format!("see {link}!")), r#"see <a data-zil="zil:help/upper"><span style="color:var(--ansi-6);">upper</span></a>!"#);
        assert_eq!(super::width(&link), 5);
    }
}
