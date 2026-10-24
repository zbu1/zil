//! Text: case, splitting, search and regex.

pub mod ciphers;
pub mod compare;
pub mod encoding;
pub mod fancy;
pub mod hash;
pub mod layout;
pub mod placeholder;
pub mod translation;

use super::{Call, Claim, Doc, Fail, Module, doc};
use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "text",
    about: "case, splitting, search, regex; encodings, ciphers and more below",
    #[rustfmt::skip]
    examples: &[
        ("text", &[
            ("numbers out of text", r#""a1b22c333".nums.sum"#),
            ("regex captures", r#""2026-10-06".match(r"(\d+)-(\d+)-(\d+)")"#),
            ("regex replace with $1", r#""CamelCaseName".replace(r"(\B[A-Z])", " $1")"#),
            ("grep lines", r#""ok\nERROR disk full\nok".grep(r"ERROR")"#),
            ("initials", r#""Ada Lovelace".split.map(|w| w[0]).join"#),
            ("printf", r#"format("%-6s|%5.2f", "pi", pi)"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("str", r#""héllo\tworld""#), ("regex", r#"r"\d+""#)]),
        ("operators", &[("str + v", r#""v" + 2"#), ("str * int", r#""ab" * 3"#)]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("shape", &["upper", "lower", "capitalize", "trim", "repeat", "strip_ansi"]),
        ("split", &["split", "lines", "chars", "join"]),
        ("search", &["starts_with", "ends_with", "match", "find_all", "grep", "replace", "nums"]),
    ],
    call,
    binary: Some(binary),
    children: &[layout::MODULE, compare::MODULE, translation::MODULE, encoding::MODULE, hash::MODULE, ciphers::MODULE, fancy::MODULE, placeholder::MODULE],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("upper", "upper(s: str)", "uppercase a string", &[r#""hello".upper"#, r#"upper("zil")"#], &["lower", "capitalize"]),
    doc("lower", "lower(s: str)", "lowercase a string", &[r#""HeLLo".lower"#], &["upper", "capitalize"]),
    doc("trim", "trim(s: str)", "strip leading and trailing whitespace", &[r#""  hi  ".trim"#], &["split"]),
    doc("capitalize", "capitalize(s: str)", "uppercase the first character", &[r#""hello world".capitalize"#], &["upper"]),
    doc("split", "split(s: str, sep?: str|regex)", "split on sep (string or regex); whitespace if omitted", &[r#""a b  c".split"#, r#""x,y;z".split(r"[,;]")"#], &["join", "lines", "chars"]),
    doc("lines", "lines(s: str)", "split into lines", &[r#""a\nb".lines"#], &["split"]),
    doc("chars", "chars(s: str)", "list of characters", &[r#""abc".chars"#], &["split"]),
    doc("join", "join(xs: list, sep?: str)", "join items into a string", &[r#"["a", "b"].join("-")"#, "[1, 2].join"], &["split"]),
    doc("replace", "replace(s: str, pat: str|regex, with: str)", "replace all matches; regex replacements can use $1", &[r#""a.b".replace(".", "-")"#, r#""a  b   c".replace(r"\s+", " ")"#], &["find_all", "match"]),
    doc("starts_with", "starts_with(s: str, prefix: str)", "whether s starts with prefix", &[r#""abc".starts_with("a")"#], &["ends_with", "contains"]),
    doc("ends_with", "ends_with(s: str, suffix: str)", "whether s ends with suffix", &[r#""file.zil".ends_with(".zil")"#], &["starts_with"]),
    doc("match", "match(s: str, re: regex)", "first match, its capture groups, or nil", &[r#""2026-10-06".match(r"(\d+)-(\d+)")"#, r#""id 42".match(r"\d+")"#], &["find_all", "replace"]),
    doc("find_all", "find_all(s: str, pat: str|regex)", "list of all matches", &[r#""a1b22c333".find_all(r"\d+")"#], &["match", "count"]),
    doc("grep", "grep(v: str|list, pat: str|regex)", "lines of a string (or items of a list) containing pat", &[r#""ok\nERROR 1\nERROR 2".grep("ERROR")"#, r#"["a1", "b", "c22"].grep(r"\d")"#], &["lines", "filter", "contains"]),
    doc("repeat", "repeat(s: str, n: int)", "repeat a string n times", &[r#""ab".repeat(3)"#], &[]),
    doc("nums", "nums(s: str)", "every number in a string", &[r#""x=3, y=-2.5; 1e3".nums"#], &["find_all", "parse"]),
    doc("strip_ansi", "strip_ansi(s: str)", "remove terminal color and cursor codes", &[r#"gradient("ab", "red", "blue").strip_ansi"#], &["rainbow", "len"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("upper", [Str(s)]) => Value::str(s.to_uppercase()),
        ("lower", [Str(s)]) => Value::str(s.to_lowercase()),
        ("trim", [Str(s)]) => Value::str(s.trim()),
        ("capitalize", [Str(s)]) => {
            let mut c = s.chars();
            Value::str(c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default())
        }
        ("split", [Str(s)]) => strs(s.split_whitespace()),
        ("split", [Str(s), Str(sep)]) => strs(s.split(&**sep)),
        ("split", [Str(s), Regex(r)]) => strs(r.split(s)),
        ("lines", [Str(s)]) => strs(s.lines()),
        ("chars", [Str(s)]) => Value::list(s.chars().map(|c| Value::str(c.to_string())).collect()),
        ("join", [List(l)]) => Value::str(l.borrow().iter().map(Value::to_string).collect::<String>()),
        ("join", [List(l), Str(sep)]) => Value::str(l.borrow().iter().map(Value::to_string).collect::<Vec<_>>().join(sep)),
        ("replace", [Str(s), Str(from), Str(to)]) => Value::str(s.replace(&**from, to)),
        ("replace", [Str(s), Regex(r), Str(to)]) => Value::str(r.replace_all(s, &**to)),
        ("starts_with", [Str(s), Str(p)]) => Bool(s.starts_with(&**p)),
        ("ends_with", [Str(s), Str(p)]) => Bool(s.ends_with(&**p)),
        // Whole match, or the list of groups when the regex has capture groups.
        ("match", [Str(s), Regex(r)]) => match r.captures(s) {
            None => Nil,
            Some(c) if c.len() == 1 => Value::str(&c[0]),
            Some(c) => Value::list(c.iter().skip(1).map(|g| g.map_or(Nil, |g| Value::str(g.as_str()))).collect()),
        },
        ("find_all", [Str(s), Regex(r)]) => strs(r.find_iter(s).map(|m| m.as_str())),
        ("find_all", [Str(s), Str(sub)]) if !sub.is_empty() => strs(s.matches(&**sub)),
        ("grep", [Str(s), Str(p)]) => strs(s.lines().filter(|l| l.contains(&**p))),
        ("grep", [Str(s), Regex(r)]) => strs(s.lines().filter(|l| r.is_match(l))),
        ("grep", [List(l), p @ (Str(_) | Regex(_))]) => {
            let hit = |v: &Value| match (v.to_string(), p) {
                (t, Str(p)) => t.contains(&**p),
                (t, Regex(r)) => r.is_match(&t),
                _ => unreachable!(),
            };
            Value::list(l.borrow().iter().filter(|v| hit(v)).cloned().collect())
        }
        ("repeat", [Str(s), Int(n, _)]) => repeat(s, *n)?,
        // Every number in the text; a `-` glued to a word or number (2026-10-06) is a separator, not a sign.
        ("nums", [Str(s)]) => {
            let re = regex::Regex::new(r"-?\d+(\.\d+)?([eE][+-]?\d+)?").unwrap();
            let num = |m: regex::Match| {
                let glued = s[..m.start()].ends_with(|c: char| c.is_alphanumeric());
                let t = if glued { m.as_str().trim_start_matches('-') } else { m.as_str() };
                t.parse().map_or_else(|_| Float(t.parse().unwrap_or(f64::NAN)), Value::int)
            };
            Value::list(re.find_iter(s).map(num).collect())
        }
        ("strip_ansi", [Str(s)]) => Value::str(crate::ansi::strip_ansi(s)),
        _ => return Err(Fail::BadArgs),
    })
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
        (BinOp::Mul, Str(s), Int(n, _)) | (BinOp::Mul, Int(n, _), Str(s)) => repeat(s, *n),
        _ => return None,
    })
}

fn repeat(s: &str, n: i64) -> Result<Value, String> {
    let n = n.max(0) as usize;
    crate::state::check_len(s.len().saturating_mul(n), "bytes")?;
    Ok(Value::str(s.repeat(n)))
}

fn strs<'a>(it: impl Iterator<Item = &'a str>) -> Value {
    Value::list(it.map(Value::str).collect())
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
