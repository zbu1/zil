//! Text: case, splitting, search and regex.

pub mod ciphers;
pub mod compare;
pub mod encoding;
pub mod fancy;
pub mod hash;
pub mod layout;
pub mod placeholder;
pub mod translation;

use super::{Call, Claim, Doc, Fail, Module, doc, flags};
use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
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
    doc("upper", "upper(s: str) -> str", "the string in UPPERCASE", &[r#""hello".upper"#, r#"upper("zil")"#], &["lower", "capitalize"]),
    doc("lower", "lower(s: str) -> str", "the string in lowercase", &[r#""HeLLo".lower"#], &["upper", "capitalize"]),
    doc("trim", "trim(s: str) -> str", "remove spaces, tabs and newlines from both ends", &[r#""  hi  ".trim"#], &["split"]),
    doc("capitalize", "capitalize(s: str) -> str", "make the first character uppercase", &[r#""hello world".capitalize"#], &["upper"]),
    doc("split", "split(s: str, sep?: str|regex) -> list", "split into a list of pieces wherever sep appears (a string or a regex)", &[r#""a b  c".split"#, r#""x,y;z".split(r"[,;]")"#], &["join", "lines", "chars"])
        .detail("Without sep, split on runs of whitespace"),
    doc("lines", "lines(s: str) -> list", "split into a list of lines", &[r#""a\nb".lines"#], &["split"]),
    doc("chars", "chars(s: str) -> list", "split into a list of single characters", &[r#""abc".chars"#], &["split"]),
    doc("join", "join(xs: list, sep?: str) -> str", "join a list's items into one string, with sep between them (nothing by default)", &[r#"["a", "b"].join("-")"#, "[1, 2].join"], &["split"]),
    doc("replace", "replace(s: str, pat: str|regex, with: str) -> str", "replace every match of pat with the given text", &[r#""a.b".replace(".", "-")"#, r#""a  b   c".replace(r"\s+", " ")"#], &["find_all", "match"])
        .detail("With a regex, $1 in it stands for what the first capture group matched, $2 the second, and so on"),
    doc("starts_with", "starts_with(s: str, prefix: str) -> bool", "whether s starts with prefix", &[r#""abc".starts_with("a")"#], &["ends_with", "contains"]),
    doc("ends_with", "ends_with(s: str, suffix: str) -> bool", "whether s ends with suffix", &[r#""file.zil".ends_with(".zil")"#], &["starts_with"]),
    doc("match", "match(s: str, re: regex) -> str|list|nil", "the first match of a regex, or nil if there's none", &[r#""2026-10-06".match(r"(\d+)-(\d+)")"#, r#""id 42".match(r"\d+")"#], &["find_all", "replace"])
        .detail("If the regex has capture groups (parentheses), a list of what each captured instead"),
    doc("find_all", "find_all(s: str, pat: str|regex) -> list", "a list of every match of pat", &[r#""a1b22c333".find_all(r"\d+")"#], &["match", "count"]),
    doc("grep", "grep(v: str|list, pat: str|regex) -> list|int", "the lines of a string, or items of a list, that contain pat", &[r#""ok\nERROR 1\nERROR 2".grep("ERROR")"#, r#"["a1", "b", "c22"].grep(r"\d")"#], &["lines", "filter", "contains"])
        .detail("As a command, -i ignores case, -v keeps the lines that don't match instead, and -c counts them. In a call, pass a map like {i: true}"),
    doc("repeat", "repeat(s: str, n: int) -> str", "the string repeated n times", &[r#""ab".repeat(3)"#], &[]),
    doc("nums", "nums(s: str) -> list", "every number found in a string, as a list of numbers", &[r#""x=3, y=-2.5; 1e3".nums"#], &["find_all", "parse"]),
    doc("strip_ansi", "strip_ansi(s: str) -> str", "remove the hidden codes that color terminal text or move the cursor, leaving the plain text", &[r#"gradient("ab", "red", "blue").strip_ansi"#], &["rainbow", "len"]),
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
        ("grep", [v @ (Str(_) | List(_)), p @ (Str(_) | Regex(_)), ..]) => {
            let (args, f) = flags(name, args)?;
            if args.len() > 2 {
                return Err(Fail::BadArgs);
            }
            let pat = if let Regex(r) = p { r.as_str().to_string() } else { regex::escape(&p.to_string()) };
            let re = regex::Regex::new(&format!("{}{pat}", if f.contains('i') { "(?i)" } else { "" })).map_err(|e| Fail::Arg(1, e.to_string()))?;
            let hit = |t: &str| re.is_match(t) != f.contains('v');
            let hits: Vec<Value> = match v {
                Str(s) => s.lines().filter(|l| hit(l)).map(Value::str).collect(),
                List(l) => l.borrow().iter().filter(|v| hit(&v.to_string())).cloned().collect(),
                _ => unreachable!(),
            };
            if f.contains('c') { Value::int(hits.len() as i64) } else { Value::list(hits) }
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
