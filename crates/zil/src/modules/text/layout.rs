//! Text layout and case: padding, wrapping, truncating, and snake/camel/kebab/title/slug.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc, flags};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "layout",
    about: "fit text to a width, convert between naming styles like snake_case and camelCase, and count words",
    #[rustfmt::skip]
    examples: &[
        ("layout", &[
            ("a receipt line", r#""coffee".pad(12, ".") + "$4.50".pad_left(8)"#),
            ("a banner", r#""menu".upper.center(20, "=")"#),
            ("a column name as a variable name", r#""Total Price (USD)".snake"#),
            ("a title as a URL slug", r#""Hello, World! It's 2026".slug"#),
            ("minutes to read 450 words at 200 a minute", r#"(lorem(450).word_count / 200).ceil"#),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("layout", &["pad", "pad_left", "center", "truncate", "wrap", "dedent"]),
        ("case", &["title", "snake", "camel", "kebab", "slug"]),
        ("words", &["words", "word_count", "wc"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("pad", "pad(s: any, width: int, fill?: str) -> str", "add fill (spaces by default) to the right until the text is width characters, so it lines up on the left", &[r#""ab".pad(5) + "|""#, r#""ab".pad(5, ".")"#], &["pad_left", "center"]),
    doc("pad_left", "pad_left(s: any, width: int, fill?: str) -> str", "add fill (spaces by default) to the left until the text is width characters, so it lines up on the right", &[r#"42.pad_left(6, "0")"#], &["pad", "center"])
        .detail("Numbers work too"),
    doc("center", "center(s: any, width: int, fill?: str) -> str", "center the text in width characters", &[r#""hi".center(8, "*")"#], &["pad", "pad_left"])
        .detail("When the fill can't split evenly, the extra goes on the right"),
    doc("truncate", "truncate(s: str, n: int) -> str", "shorten to at most n characters, ending in … if anything was cut", &[r#""a long sentence here".truncate(10)"#], &["wrap"]),
    doc("wrap", "wrap(s: str, width: int) -> str", "break lines between words so none is longer than width characters", &[r#""the quick brown fox jumps over the lazy dog".wrap(15)"#], &["truncate", "dedent"]),
    doc("dedent", "dedent(s: str) -> str", "remove the indentation that every non-blank line has in common", &[r#""    a\n      b\n    c".dedent"#], &["trim", "wrap"]),
    doc("title", "title(s: str) -> str", "Capitalize The First Letter Of Each Word", &[r#""the lord of the rings".title"#], &["capitalize", "snake"]),
    doc("snake", "snake(s: str) -> str", "rewrite as snake_case: lowercase words joined by _", &[r#""parseHTTPRequest".snake"#, r#""Total Price".snake"#], &["camel", "kebab"])
        .detail("Words are split at spaces, punctuation and where a lowercase letter meets a capital"),
    doc("camel", "camel(s: str) -> str", "rewrite as camelCase: words joined with no separator, each after the first capitalized", &[r#""user_id_v2".camel"#], &["snake", "kebab"]),
    doc("kebab", "kebab(s: str) -> str", "rewrite as kebab-case: lowercase words joined by -", &[r#""MyComponentName".kebab"#], &["snake", "slug"]),
    doc("slug", "slug(s: str) -> str", "a lowercase version for use in a URL: only letters and digits, with - between words", &[r#""Hello, World! It's 2026".slug"#], &["kebab"]),
    doc("words", "words(s: str) -> list", "a list of the words, with punctuation removed except apostrophes", &[r#""Hello, world! Don't panic.".words"#], &["word_count", "split"]),
    doc("wc", "wc(v: str|bytes) -> map|int", "count lines, words and bytes, like the Unix wc tool", &[r#""a b\nc\n".wc"#, r#"wc("a b\nc\n", {w: true})"#], &["word_count", "lines", "cat"])
        .detail("As a command, -l, -w or -c gives just that count. In a call, pass a map like {w: true}"),
    doc("word_count", "word_count(s: str) -> int", "how many words there are, counted as words does", &[r#""Hello, world! Don't panic.".word_count"#], &["words"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("pad" | "pad_left" | "center", [s, Int(w, _), rest @ ..]) if rest.len() <= 1 => {
            let fill = match rest {
                [] => ' ',
                [Str(f)] if f.chars().count() == 1 => f.chars().next().unwrap(),
                _ => return Err(Fail::Arg(2, "fill must be one character".into())),
            };
            let s = s.to_string();
            crate::state::check_len((*w).max(0) as usize, "characters")?;
            let gap = ((*w).max(0) as usize).saturating_sub(s.chars().count());
            let (l, r) = match name {
                "pad" => (0, gap),
                "pad_left" => (gap, 0),
                _ => (gap / 2, gap - gap / 2),
            };
            let f = |n| fill.to_string().repeat(n);
            Value::str(format!("{}{s}{}", f(l), f(r)))
        }
        ("truncate", [Str(s), Int(n, _)]) if *n >= 1 => {
            let n = *n as usize;
            if s.chars().count() <= n { Str(s.clone()) } else { Value::str(s.chars().take(n - 1).collect::<String>() + "…") }
        }
        ("wrap", [Str(s), Int(w, _)]) if *w >= 1 => Value::str(s.lines().map(|l| wrap(l, *w as usize)).collect::<Vec<_>>().join("\n")),
        ("dedent", [Str(s)]) => {
            let indent = s.lines().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
            Value::str(s.lines().map(|l| l.get(indent..).unwrap_or("").trim_end()).collect::<Vec<_>>().join("\n"))
        }
        ("title", [Str(s)]) => {
            let mut out = String::new();
            let mut start = true;
            for c in s.chars() {
                if start {
                    out.extend(c.to_uppercase())
                } else {
                    out.extend(c.to_lowercase())
                }
                start = !c.is_alphanumeric() && c != '\'';
            }
            Value::str(out)
        }
        ("snake" | "kebab" | "slug", [Str(s)]) => {
            Value::str(parts(s).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(if name == "snake" { "_" } else { "-" }))
        }
        ("camel", [Str(s)]) => {
            let ps = parts(s);
            let cap = |w: &str| {
                w.chars().next().map(|c| c.to_uppercase().chain(w.chars().skip(1).flat_map(char::to_lowercase)).collect::<String>()).unwrap_or_default()
            };
            Value::str(ps.iter().enumerate().map(|(i, w)| if i == 0 { w.to_lowercase() } else { cap(w) }).collect::<String>())
        }
        ("words", [Str(s)]) => Value::list(words(s).map(Value::str).collect()),
        ("wc", [v @ (Str(_) | Bytes(_)), ..]) => {
            let (args, f) = flags(name, args)?;
            if args.len() > 1 {
                return Err(Fail::BadArgs);
            }
            let (s, len) = match v {
                Str(s) => (s.to_string(), s.len()),
                Bytes(b) => (String::from_utf8_lossy(b).into_owned(), b.len()),
                _ => unreachable!(),
            };
            let counts = [("lines", 'l', s.lines().count()), ("words", 'w', s.split_whitespace().count()), ("bytes", 'c', len)];
            let mut picked: Vec<_> = counts.iter().filter(|c| f.contains(c.1)).collect();
            match picked.len() {
                1 => Value::int(picked[0].2 as i64),
                n => {
                    if n == 0 {
                        picked = counts.iter().collect();
                    }
                    Value::map(picked.iter().map(|c| (c.0.to_string(), Value::int(c.2 as i64))).collect())
                }
            }
        }
        ("word_count", [Str(s)]) => Value::int(words(s).count() as i64),
        ("truncate" | "wrap", [Str(_), Int(n, _)]) => return Err(Fail::Arg(1, format!("expected at least 1, got {n}"))),
        _ => return Err(Fail::BadArgs),
    })
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’').map(|w| w.trim_matches(['\'', '’'])).filter(|w| !w.is_empty())
}

/// Identifier pieces: split on anything not a letter or digit, and at camelCase humps (`HTTPServer` → HTTP, Server).
fn parts(s: &str) -> Vec<String> {
    let mut out = vec![];
    for chunk in s.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|c| !c.is_empty()) {
        let cs: Vec<char> = chunk.chars().filter(|c| *c != '\'').collect();
        let mut cur = String::new();
        for (i, &c) in cs.iter().enumerate() {
            let hump = i > 0
                && c.is_uppercase()
                && (cs[i - 1].is_lowercase() || cs[i - 1].is_ascii_digit() || cs.get(i + 1).is_some_and(|n| n.is_lowercase()) && cs[i - 1].is_uppercase());
            if hump && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        out.push(cur);
    }
    out
}

/// Greedy wrap of one paragraph; a word longer than the width gets a line to itself.
fn wrap(line: &str, width: usize) -> String {
    let mut out = String::new();
    let mut len = 0;
    for w in line.split_whitespace() {
        let n = w.chars().count();
        if len > 0 && len + 1 + n > width {
            out.push('\n');
            len = 0;
        } else if len > 0 {
            out.push(' ');
            len += 1;
        }
        out.push_str(w);
        len += n;
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn layout() {
        assert_eq!(
            show(r#"["ab".pad(4, "."), "ab".pad_left(4), "ab".center(5, "*"), "abcdef".pad(3), 7.pad_left(3, "0")]"#),
            r#"["ab..", "  ab", "*ab**", "abcdef", "007"]"#
        );
        assert_eq!(show(r#"["hello world".truncate(5), "hi".truncate(5)]"#), r#"["hell…", "hi"]"#);
        assert_eq!(show(r#""aaa bbb ccc ddd".wrap(7)"#), "aaa bbb\nccc ddd");
        assert_eq!(show(r#""  a\n    b\n\n  c".dedent"#), "a\n  b\n\nc");
        assert_eq!(show(r#""don't STOP me-now".title"#), "Don't Stop Me-Now");
        assert_eq!(
            show(r#"["parseHTTPRequest".snake, "XMLHttpRequest".kebab, "user_id v2".camel, "Hello, World! It's 2026".slug, "md5Sum".snake]"#),
            r#"["parse_http_request", "xml-http-request", "userIdV2", "hello-world-its-2026", "md5_sum"]"#
        );
        assert_eq!(show(r#""Hello, world! Don't 'quote' me.".words"#), r#"["Hello", "world", "Don't", "quote", "me"]"#);
        assert!(try_eval(r#""a".pad(3, "ab")"#).is_err());
        assert!(try_eval(r#""a".truncate(0)"#).is_err());
    }
}
