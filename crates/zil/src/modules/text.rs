//! Text: case, splitting, search and regex.

pub mod ciphers;
pub mod compare;
pub mod encoding;
pub mod fancy;
pub mod hash;
pub mod layout;
pub mod placeholder;
pub mod translation;

use super::{Claim, Module};
use crate::ast::BinOp;
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "text",
    about: "change case, split and join, search and replace in text, with regular expressions",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Working with strings: changing case, splitting and joining, and searching and replacing."),
        ("writing", "\
A regular expression (regex) is a pattern for matching text, written `r\"...\"`: `r\"\\d+\"` matches a run of digits. \
Functions that search accept either a plain string, matched exactly, or a regex. Parentheses in a regex capture the \
part they match. `match` returns the captured parts, and in `replace`, `$1` stands for the first one."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("text", &[
            ("numbers out of text", r#""a1b22c333".nums.sum"#),
            ("the parts a regex captures", r#""2026-10-06".match(r"(\d+)-(\d+)-(\d+)")"#),
            ("a space before each capital, $1 being the capture", r#""CamelCaseName".replace(r"(\B[A-Z])", " $1")"#),
            ("only the lines that match", r#""ok\nERROR disk full\nok".grep(r"ERROR")"#),
            ("initials", r#""Ada Lovelace".split.map(|w| w[0]).join"#),
            ("printf-style formatting", r#"format("%-6s|%5.2f", "pi", pi)"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("str: text in double quotes", r#""héllo\tworld""#), ("regex: a pattern, r and then quotes", r#"r"\d+""#)]),
        ("operators", &[("str + anything joins them as text", r#""v" + 2"#), ("str * n repeats it n times", r#""ab" * 3"#)]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("shape", &["upper", "lower", "capitalize", "trim", "repeat", "strip_ansi"]),
        ("split", &["split", "lines", "chars", "join"]),
        ("search", &["starts_with", "ends_with", "match", "find_all", "grep", "replace", "nums"]),
    ],
    call: fns::call,
    binary: Some(binary),
    children: &[layout::MODULE, compare::MODULE, translation::MODULE, encoding::MODULE, hash::MODULE, ciphers::MODULE, fancy::MODULE, placeholder::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{repeat, strs};
    use crate::modules::Fail;
    use crate::signature::{Flags, List, Or};
    use crate::value::Value;
    use regex::Regex;

    /// the string in UPPERCASE
    #[builtin(examples = [r#""hello".upper"#, r#"upper("zil")"#], see = ["lower", "capitalize"])]
    fn upper(s: &str) -> String {
        s.to_uppercase()
    }

    /// the string in lowercase
    #[builtin(examples = [r#""HeLLo".lower"#], see = ["upper", "capitalize"])]
    fn lower(s: &str) -> String {
        s.to_lowercase()
    }

    /// remove spaces, tabs and newlines from both ends
    #[builtin(examples = [r#""  hi  ".trim"#], see = ["split"])]
    fn trim(s: &str) -> String {
        s.trim().into()
    }

    /// make the first character uppercase
    #[builtin(examples = [r#""hello world".capitalize"#], see = ["upper"])]
    fn capitalize(s: &str) -> String {
        let mut c = s.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    }

    /// split into a list of pieces wherever sep appears (a string or a regex)
    ///
    /// Without sep, split on runs of whitespace
    #[builtin(examples = [r#""a b  c".split"#, r#""x,y;z".split(r"[,;]")"#], see = ["join", "lines", "chars"])]
    fn split(s: &str, sep: Option<Or<&str, &Regex>>) -> Vec<Value> {
        match sep {
            None => strs(s.split_whitespace()),
            Some(Or::A(sep)) => strs(s.split(sep)),
            Some(Or::B(r)) => strs(r.split(s)),
        }
    }

    /// split into a list of lines
    #[builtin(examples = [r#""a\nb".lines"#], see = ["split"])]
    fn lines(s: &str) -> Vec<Value> {
        strs(s.lines())
    }

    /// split into a list of single characters
    #[builtin(examples = [r#""abc".chars"#], see = ["split"])]
    fn chars(s: &str) -> Vec<Value> {
        s.chars().map(|c| Value::str(c.to_string())).collect()
    }

    /// join a list's items into one string, with sep between them
    #[builtin(examples = [r#"["a", "b"].join("-")"#, "[1, 2].join"], see = ["split"])]
    fn join(xs: &List, #[default("")] sep: &str) -> String {
        xs.borrow().iter().map(Value::to_string).collect::<Vec<_>>().join(sep)
    }

    /// replace every match of pat with the given text
    ///
    /// With a regex, $1 in it stands for what the first capture group matched, $2 the second, and so on
    #[builtin(examples = [r#""a.b".replace(".", "-")"#, r#""a  b   c".replace(r"\s+", " ")"#], see = ["find_all", "match"])]
    fn replace(s: &str, pat: Or<&str, &Regex>, with: &str) -> String {
        match pat {
            Or::A(from) => s.replace(from, with),
            Or::B(r) => r.replace_all(s, with).into_owned(),
        }
    }

    /// whether s starts with prefix
    #[builtin(examples = [r#""abc".starts_with("a")"#], see = ["ends_with", "contains"])]
    fn starts_with(s: &str, prefix: &str) -> bool {
        s.starts_with(prefix)
    }

    /// whether s ends with suffix
    #[builtin(examples = [r#""file.zil".ends_with(".zil")"#], see = ["starts_with"])]
    fn ends_with(s: &str, suffix: &str) -> bool {
        s.ends_with(suffix)
    }

    /// the first match of a regex, or nil if there's none
    ///
    /// If the regex has capture groups (parentheses), a list of what each captured instead
    #[builtin(examples = [r#""2026-10-06".match(r"(\d+)-(\d+)")"#, r#""id 42".match(r"\d+")"#], see = ["find_all", "replace"])]
    fn r#match(s: &str, re: &Regex) -> Option<Or<String, Vec<Value>>> {
        let c = re.captures(s)?;
        Some(match c.len() {
            1 => Or::A(c[0].into()),
            _ => Or::B(c.iter().skip(1).map(|g| g.map_or(Value::Nil, |g| Value::str(g.as_str()))).collect()),
        })
    }

    /// a list of every match of pat
    #[builtin(examples = [r#""a1b22c333".find_all(r"\d+")"#], see = ["match", "count"])]
    fn find_all(s: &str, pat: Or<&str, &Regex>) -> Result<Vec<Value>, Fail> {
        match pat {
            Or::A("") => Err(Fail::BadArgs),
            Or::A(sub) => Ok(strs(s.matches(sub))),
            Or::B(r) => Ok(strs(r.find_iter(s).map(|m| m.as_str()))),
        }
    }

    /// the lines of a string, or items of a list, that contain pat
    #[builtin(
        examples = [r#""ok\nERROR 1\nERROR 2".grep("ERROR")"#, r#"["a1", "b", "c22"].grep(r"\d")"#],
        see = ["lines", "filter", "contains"],
        flags = [("i", "ignore_case", "ignore case"), ("v", "invert", "keep the lines that don't match instead"), ("c", "count", "count the matches instead")],
        command = "grep -i err app.log"
    )]
    fn grep(v: Or<&str, &List>, pat: Or<&str, &Regex>, f: Flags) -> Result<Or<Vec<Value>, i64>, Fail> {
        let pat = match pat {
            Or::A(s) => regex::escape(s),
            Or::B(r) => r.as_str().to_string(),
        };
        let re = crate::state::regex(&format!("{}{pat}", if f.has('i') { "(?i)" } else { "" })).map_err(|e| Fail::Arg(1, e.to_string()))?;
        let hit = |t: &str| re.is_match(t) != f.has('v');
        let hits: Vec<Value> = match v {
            Or::A(s) => s.lines().filter(|l| hit(l)).map(Value::str).collect(),
            Or::B(l) => l.borrow().iter().filter(|v| hit(&v.to_string())).cloned().collect(),
        };
        Ok(if f.has('c') { Or::B(hits.len() as i64) } else { Or::A(hits) })
    }

    /// the string repeated n times
    #[builtin(name = "repeat", examples = [r#""ab".repeat(3)"#])]
    fn repeat_n(s: &str, n: i64) -> Result<String, String> {
        repeat(s, n)
    }

    /// every number found in a string, as a list of numbers
    #[builtin(examples = [r#""x=3, y=-2.5; 1e3".nums"#], see = ["find_all", "parse"])]
    fn nums(s: &str) -> Vec<Value> {
        // A `-` glued to a word or number (2026-10-06) is a separator, not a sign.
        let re = Regex::new(r"-?\d+(\.\d+)?([eE][+-]?\d+)?").unwrap();
        let num = |m: regex::Match| {
            let glued = s[..m.start()].ends_with(|c: char| c.is_alphanumeric());
            let t = if glued { m.as_str().trim_start_matches('-') } else { m.as_str() };
            t.parse().map_or_else(|_| Value::Float(t.parse().unwrap_or(f64::NAN)), Value::int)
        };
        re.find_iter(s).map(num).collect()
    }

    /// remove the hidden codes that color terminal text or move the cursor, leaving the plain text
    #[builtin(examples = [r#"gradient("ab", "red", "blue").strip_ansi"#], see = ["rainbow", "len"])]
    fn strip_ansi(s: &str) -> String {
        crate::ansi::strip_ansi(s)
    }
}

/// `+` with a string on either side concatenates; `str * n` repeats.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::*;
    Some(match (op, a, b) {
        (BinOp::Add, Bytes(x), Bytes(y)) => crate::state::check_len(x.len() + y.len(), "bytes").map(|_| Value::bytes([&x[..], y].concat())),
        (BinOp::Mul, Bytes(x), Int(n, _)) | (BinOp::Mul, Int(n, _), Bytes(x)) => {
            let n = (*n).max(0) as usize;
            crate::state::check_len(x.len().saturating_mul(n), "bytes").map(|_| Value::bytes(x.repeat(n)))
        }
        // Bytes never turn into text by accident: `"a" + b"b"` is an error, not `ab"b"`.
        (BinOp::Add, Bytes(_), _) | (BinOp::Add, _, Bytes(_)) => Err(crate::ops::mismatch(op, a, b)),
        (BinOp::Add, Str(_), _) | (BinOp::Add, _, Str(_)) => {
            let s = format!("{a}{b}");
            crate::state::check_len(s.len(), "bytes").map(|_| Value::str(s))
        }
        (BinOp::Mul, Str(s), Int(n, _)) | (BinOp::Mul, Int(n, _), Str(s)) => repeat(s, *n).map(Value::str),
        _ => return None,
    })
}

fn repeat(s: &str, n: i64) -> Result<String, String> {
    let n = n.max(0) as usize;
    crate::state::check_len(s.len().saturating_mul(n), "bytes")?;
    Ok(s.repeat(n))
}

fn strs<'a>(it: impl Iterator<Item = &'a str>) -> Vec<Value> {
    it.map(Value::str).collect()
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn strings() {
        assert_eq!(show(r##""a1b22c333".replace(r"\d+", "#")"##), "a#b#c#");
        assert_eq!(show(r#""a.b.c".replace(".", "-")"#), "a-b-c");
        assert_eq!(show(r#""2026-10-06".match(r"(\d+)-(\d+)-(\d+)")"#), r#"["2026", "10", "06"]"#);
        assert_eq!(show(r#""x1 y22 z333".find_all(r"\d+")"#), r#"["1", "22", "333"]"#);
        assert_eq!(show(r#""  a  b ".split"#), r#"["a", "b"]"#);
        assert_eq!(show(r#""hello world".capitalize"#), "Hello world");
        assert_eq!(show(r#""ok\nERROR 1\nfine\nERROR 2".grep("ERROR")"#), r#"["ERROR 1", "ERROR 2"]"#);
        assert_eq!(show(r#"["a1", "b", "c22"].grep(r"\d")"#), r#"["a1", "c22"]"#);
        assert_eq!(show(r#""x=3, y=-2.5 on 2026-10-06, 1e3".nums"#), "[3, -2.5, 2026, 10, 6, 1000]");
    }
}
