//! Comparing text: edit distance, similarity, nearest match, line diffs.

use crate::error::edits;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "compare",
    about: "how different two strings are, the closest match from a list, and line-by-line differences",
    #[rustfmt::skip]
    intro: &[
        ("", "\
How different two strings are, and what changed between two texts. The edit distance between two strings is the fewest \
single-character changes that turn one into the other, where a change is adding a character, removing one, replacing \
one, or swapping two that are next to each other. \"form\" and \"from\" are 1 apart, and \"kitten\" and \"sitting\" 3, \
so it's a good measure of how likely one is a typo of the other."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("compare", &[
            ("fix a typo", r#""recieve".closest(["deceive", "receive", "recipe"])"#),
            ("how alike two spellings are", r#"similarity("color", "colour").percent(0)"#),
            ("what changed", r#"text_diff("a\nb\nc", "a\nB\nc\nd")"#),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("distance", "distance(a: str, b: str) -> int", "the edit distance: how many single-character changes turn a into b", &[r#"distance("kitten", "sitting")"#, r#"distance("form", "from")"#], &["similarity", "closest"]),
    doc("similarity", "similarity(a: str, b: str) -> float", "how alike two strings are, from 1 (identical) to 0 (nothing in common)", &[r#"similarity("color", "colour")"#], &["distance"])
        .detail("It's the edit distance as a share of the longer string's length, taken from 1"),
    doc("closest", "closest(s: str, options: list) -> str|nil", "the option with the smallest edit distance to s, ignoring upper and lower case, or nil for an empty list", &[r#""aple".closest(["apple", "maple", "ape"])"#], &["distance"]),
    doc("text_diff", "text_diff(a: str, b: str) -> str", "what changed from a to b, line by line", &[r#"text_diff("a\nb\nc", "a\nc\nd")"#], &["distance"])
        .detail("Removed lines start with -, added lines with +, and unchanged lines with two spaces"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("distance", [Str(a), Str(b)]) => Value::int(edits(a, b) as i64),
        ("similarity", [Str(a), Str(b)]) => {
            let longest = a.chars().count().max(b.chars().count());
            Float(if longest == 0 { 1.0 } else { 1.0 - edits(a, b) as f64 / longest as f64 })
        }
        ("closest", [Str(s), List(l)]) => {
            let s = s.to_lowercase();
            l.borrow().iter().min_by_key(|v| edits(&s, &v.to_string().to_lowercase())).cloned().unwrap_or(Nil)
        }
        ("text_diff", [Str(a), Str(b)]) => Value::str(diff(&a.lines().collect::<Vec<_>>(), &b.lines().collect::<Vec<_>>())),
        _ => return Err(Fail::BadArgs),
    })
}

/// LCS line diff. ponytail: O(n*m) table, fine for thousands of lines; Myers if files get big.
fn diff(a: &[&str], b: &[&str]) -> String {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push(format!("  {}", a[i]));
            (i, j) = (i + 1, j + 1);
        } else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn compare() {
        assert_eq!(show(r#"[distance("kitten", "sitting"), distance("", "abc"), distance("form", "from")]"#), "[3, 3, 1]");
        assert_eq!(show(r#"[similarity("abc", "abc"), similarity("", ""), similarity("ab", "cd")]"#), "[1, 1, 0]");
        assert_eq!(show(r#"["BANAN".closest(["bandana", "banana", "cabana"]), "x".closest([])]"#), r#"["banana", nil]"#);
        assert_eq!(show(r#"text_diff("a\nb\nc", "a\nB\nc\nd")"#), "  a\n- b\n+ B\n  c\n+ d");
    }
}
