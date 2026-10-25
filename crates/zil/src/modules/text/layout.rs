//! Text layout and case: padding, wrapping, truncating, and snake/camel/kebab/title/slug.

use crate::modules::Module;

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
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("layout", &["pad", "pad_left", "center", "truncate", "wrap", "dedent"]),
        ("case", &["title", "snake", "camel", "kebab", "slug"]),
        ("words", &["words", "word_count", "wc"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{parts, words as split_words, wrap as wrap_line};
    use crate::modules::Fail;
    use crate::signature::{Flags, Or};
    use crate::value::Value;
    use indexmap::IndexMap;

    /// `s` as text, `fill` added on the left and right until it's `width` characters; `left` is the share of the gap on the left.
    fn fit(s: &Value, width: i64, fill: &str, left: fn(usize) -> usize) -> Result<String, Fail> {
        let mut f = fill.chars();
        let (Some(fill), None) = (f.next(), f.next()) else { return Err(Fail::Arg(2, "fill must be one character".into())) };
        let s = s.to_string();
        crate::state::check_len(width.max(0) as usize, "characters")?;
        let gap = (width.max(0) as usize).saturating_sub(s.chars().count());
        let l = left(gap);
        let f = |n| fill.to_string().repeat(n);
        Ok(format!("{}{s}{}", f(l), f(gap - l)))
    }

    /// add fill to the right until the text is width characters, so it lines up on the left
    #[builtin(examples = [r#""ab".pad(5) + "|""#, r#""ab".pad(5, ".")"#], see = ["pad_left", "center"])]
    fn pad(s: &Value, width: i64, #[default(" ")] fill: &str) -> Result<String, Fail> {
        fit(s, width, fill, |_| 0)
    }

    /// add fill to the left until the text is width characters, so it lines up on the right
    ///
    /// Numbers work too
    #[builtin(examples = [r#"42.pad_left(6, "0")"#], see = ["pad", "center"])]
    fn pad_left(s: &Value, width: i64, #[default(" ")] fill: &str) -> Result<String, Fail> {
        fit(s, width, fill, |gap| gap)
    }

    /// center the text in width characters
    ///
    /// When the fill can't split evenly, the extra goes on the right
    #[builtin(examples = [r#""hi".center(8, "*")"#], see = ["pad", "pad_left"])]
    fn center(s: &Value, width: i64, #[default(" ")] fill: &str) -> Result<String, Fail> {
        fit(s, width, fill, |gap| gap / 2)
    }

    fn at_least_1(n: i64) -> Result<usize, Fail> {
        if n >= 1 { Ok(n as usize) } else { Err(Fail::Arg(1, format!("expected at least 1, got {n}"))) }
    }

    /// shorten to at most n characters, ending in … if anything was cut
    #[builtin(examples = [r#""a long sentence here".truncate(10)"#], see = ["wrap"])]
    fn truncate(s: &str, n: i64) -> Result<String, Fail> {
        let n = at_least_1(n)?;
        Ok(if s.chars().count() <= n { s.into() } else { s.chars().take(n - 1).collect::<String>() + "…" })
    }

    /// break lines between words so none is longer than width characters
    #[builtin(examples = [r#""the quick brown fox jumps over the lazy dog".wrap(15)"#], see = ["truncate", "dedent"])]
    fn wrap(s: &str, width: i64) -> Result<String, Fail> {
        let w = at_least_1(width)?;
        Ok(s.lines().map(|l| wrap_line(l, w)).collect::<Vec<_>>().join("\n"))
    }

    /// remove the indentation that every non-blank line has in common
    #[builtin(examples = [r#""    a\n      b\n    c".dedent"#], see = ["trim", "wrap"])]
    fn dedent(s: &str) -> String {
        let indent = s.lines().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        s.lines().map(|l| l.get(indent..).unwrap_or("").trim_end()).collect::<Vec<_>>().join("\n")
    }

    /// Capitalize The First Letter Of Each Word
    #[builtin(examples = [r#""the lord of the rings".title"#], see = ["capitalize", "snake"])]
    fn title(s: &str) -> String {
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
        out
    }

    fn joined(s: &str, sep: &str) -> String {
        parts(s).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(sep)
    }

    /// rewrite as snake_case: lowercase words joined by _
    ///
    /// Words are split at spaces, punctuation and where a lowercase letter meets a capital
    #[builtin(examples = [r#""parseHTTPRequest".snake"#, r#""Total Price".snake"#], see = ["camel", "kebab"])]
    fn snake(s: &str) -> String {
        joined(s, "_")
    }

    /// rewrite as camelCase: words joined with no separator, each after the first capitalized
    #[builtin(examples = [r#""user_id_v2".camel"#], see = ["snake", "kebab"])]
    fn camel(s: &str) -> String {
        let cap =
            |w: &str| w.chars().next().map(|c| c.to_uppercase().chain(w.chars().skip(1).flat_map(char::to_lowercase)).collect::<String>()).unwrap_or_default();
        parts(s).iter().enumerate().map(|(i, w)| if i == 0 { w.to_lowercase() } else { cap(w) }).collect()
    }

    /// rewrite as kebab-case: lowercase words joined by -
    #[builtin(examples = [r#""MyComponentName".kebab"#], see = ["snake", "slug"])]
    fn kebab(s: &str) -> String {
        joined(s, "-")
    }

    /// a lowercase version for use in a URL: only letters and digits, with - between words
    #[builtin(examples = [r#""Hello, World! It's 2026".slug"#], see = ["kebab"])]
    fn slug(s: &str) -> String {
        joined(s, "-")
    }

    /// a list of the words, with punctuation removed except apostrophes
    #[builtin(examples = [r#""Hello, world! Don't panic.".words"#], see = ["word_count", "split"])]
    fn words(s: &str) -> Vec<Value> {
        split_words(s).map(Value::str).collect()
    }

    /// count lines, words and bytes, like the Unix wc tool
    #[builtin(
        examples = [r#""a b\nc\n".wc"#, r#"wc("a b\nc\n", {words: true})"#],
        see = ["word_count", "lines", "read_file"],
        flags = [("l", "lines", "just the line count"), ("w", "words", "just the word count"), ("c", "bytes", "just the byte count")],
        command = "wc -l notes.txt"
    )]
    fn wc(v: Or<&str, &[u8]>, f: Flags) -> Or<IndexMap<String, Value>, i64> {
        let (s, len) = match v {
            Or::A(s) => (s.to_string(), s.len()),
            Or::B(b) => (String::from_utf8_lossy(b).into_owned(), b.len()),
        };
        let counts = [("lines", 'l', s.lines().count()), ("words", 'w', s.split_whitespace().count()), ("bytes", 'c', len)];
        let mut picked: Vec<_> = counts.iter().filter(|c| f.has(c.1)).collect();
        if picked.len() == 1 {
            return Or::B(picked[0].2 as i64);
        }
        if picked.is_empty() {
            picked = counts.iter().collect();
        }
        Or::A(picked.iter().map(|c| (c.0.to_string(), Value::int(c.2 as i64))).collect())
    }

    /// how many words there are, counted as words does
    #[builtin(examples = [r#""Hello, world! Don't panic.".word_count"#], see = ["words"])]
    fn word_count(s: &str) -> i64 {
        split_words(s).count() as i64
    }
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
