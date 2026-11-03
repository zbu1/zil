//! Comparing text: edit distance, similarity, nearest match, line diffs.

use crate::ansi::{RESET, Role};
use crate::modules::Module;
use crate::value::{Table, Value};
use std::ops::Range;

pub const MODULE: Module = Module {
    name: "compare",
    tags: &["searching"],
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
            ("what changed", r#"diff("a\nb\nc", "a\nB\nc\nd")"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{text_diff, text_table};
    use crate::error::edits;
    use crate::signature::List;
    use crate::value::Value;

    /// the edit distance: how many single-character changes turn a into b
    #[builtin(examples = [r#"distance("kitten", "sitting")"#, r#"distance("form", "from")"#], see = ["similarity", "closest"])]
    fn distance(a: &str, b: &str) -> i64 {
        edits(a, b) as i64
    }

    /// how many positions two equal-length strings differ at, the Hamming distance
    ///
    /// Bytes count differing bits instead, the measure for guessing a repeating XOR key's length
    #[builtin(examples = [r#"hamming("karolin", "kathrin")"#, r#"hamming(b"this is a test", b"wokka wokka!!!")"#], see = ["distance", "xor"])]
    fn hamming(a: &str, b: &str) -> Result<i64, String> {
        let (n, m) = (a.chars().count(), b.chars().count());
        if n != m {
            return Err(format!("the strings must be the same length, got {n} and {m} characters"));
        }
        Ok(a.chars().zip(b.chars()).filter(|(x, y)| x != y).count() as i64)
    }

    #[builtin(name = "hamming")]
    fn hamming_bits(a: &[u8], b: &[u8]) -> Result<i64, String> {
        if a.len() != b.len() {
            return Err(format!("the bytes must be the same length, got {} and {}", a.len(), b.len()));
        }
        Ok(a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones() as i64).sum())
    }

    /// how alike two strings are, from 1 (identical) to 0 (nothing in common)
    ///
    /// It's the edit distance as a share of the longer string's length, taken from 1
    #[builtin(examples = [r#"similarity("color", "colour")"#], see = ["distance"])]
    fn similarity(a: &str, b: &str) -> f64 {
        let longest = a.chars().count().max(b.chars().count());
        if longest == 0 { 1.0 } else { 1.0 - edits(a, b) as f64 / longest as f64 }
    }

    /// the option with the smallest edit distance to s, ignoring upper and lower case, or nil for an empty list
    #[builtin(examples = [r#""aple".closest(["apple", "maple", "ape"])"#], see = ["distance"])]
    fn closest(s: &str, options: &List) -> Value {
        let s = s.to_lowercase();
        options.borrow().iter().min_by_key(|v| edits(&s, &v.to_string().to_lowercase())).cloned().unwrap_or(Value::Nil)
    }

    /// what changed from text a to b, line by line, or nil if nothing did
    ///
    /// Changes come in hunks headed @@ with their line numbers, with 3 unchanged lines around them. Removed lines start
    /// with - and added lines with +, colored on a terminal with the changed words picked out. With the table flag, it is
    /// instead a table with a row per changed line, each with both sides' line numbers and lines, a removed line next to
    /// the one that replaced it and nil on a side that has none
    #[builtin(
        examples = [r#"diff("a\nb\nc", "a\nc\nd")"#, r#"diff("a\nb\nc", "a\nB\nc\nd", table: true)"#],
        see = ["distance"],
        command = "diff notes.txt notes2.txt"
    )]
    #[allow(unused_variables)]
    fn diff(
        a: &str,
        b: &str,
        /// no colors, even on a terminal
        #[flag('p')]
        plain: bool,
        /// the command reads the files as bytes, for the bytes diff
        #[flag('b')]
        bytes: bool,
        /// a table of changed lines instead of hunks
        #[flag('t')]
        table: bool,
    ) -> Value {
        if table { Value::table(text_table(a, b)) } else { text_diff(a, b, !plain && crate::ansi::color_on()).map_or(Value::Nil, Value::str) }
    }
}

fn text_table(a: &str, b: &str) -> Table {
    let (a, b): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    let line = |n: Option<&usize>| n.map_or(Value::Nil, |&n| Value::int(n as i64 + 1));
    let text = |side: &[&str], n: Option<&usize>| n.map_or(Value::Nil, |&n| Value::str(side[n]));
    let mut rows = vec![];
    for run in lcs(&a, &b).split(|o| o.0.is_some() && o.1.is_some()) {
        let dels: Vec<usize> = run.iter().filter_map(|o| o.0).collect();
        let adds: Vec<usize> = run.iter().filter_map(|o| o.1).collect();
        for n in 0..dels.len().max(adds.len()) {
            let (i, j) = (dels.get(n), adds.get(n));
            rows.push(vec![line(i), line(j), text(&a, i), text(&b, j)]);
        }
    }
    Table { cols: ["line_a", "line_b", "old", "new"].map(String::from).to_vec(), rows }
}

fn text_diff(a: &str, b: &str, color: bool) -> Option<String> {
    let (a, b): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    let at = |r: Range<usize>| if r.is_empty() { format!("{},0", r.start) } else { format!("{},{}", r.start + 1, r.len()) };
    unified(&a, &b, color, |ra, rb| format!("@@ -{} +{} @@", at(ra), at(rb)), |i, j| words(a[i], b[j]))
}

/// The changed words of line `a` and the `b` that replaced it, or none if they share no word and the whole line changed.
fn words(a: &str, b: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (ta, tb) = (tokens(a), tokens(b));
    let ops = lcs(&ta.iter().map(|r| &a[r.clone()]).collect::<Vec<_>>(), &tb.iter().map(|r| &b[r.clone()]).collect::<Vec<_>>());
    if !ops.iter().any(|&(i, j)| j.is_some() && i.is_some_and(|i| !a[ta[i].clone()].trim().is_empty())) {
        return (vec![], vec![]);
    }
    let only = |t: &[Range<usize>], k: Option<usize>, other: Option<usize>| k.filter(|_| other.is_none()).map(|k| t[k].clone());
    (ops.iter().filter_map(|&(i, j)| only(&ta, i, j)).collect(), ops.iter().filter_map(|&(i, j)| only(&tb, j, i)).collect())
}

/// Runs of letters, digits and `_`, runs of whitespace, and each other character on its own.
fn tokens(s: &str) -> Vec<Range<usize>> {
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            1
        } else if c.is_whitespace() {
            2
        } else {
            0
        }
    };
    let (mut out, mut last): (Vec<Range<usize>>, _) = (vec![], 0);
    for (i, c) in s.char_indices() {
        let k = class(c);
        match out.last_mut() {
            Some(r) if k != 0 && k == last => r.end = i + c.len_utf8(),
            _ => out.push(i..i + c.len_utf8()),
        }
        last = k;
    }
    out
}

/// A unified diff of lines `a` and `b`, in hunks with 3 lines of context, or `None` if they're the same. `header` names
/// a hunk from the line ranges it covers on each side, and `marks` gives the changed spans of `a[i]` and the `b[j]` that
/// replaced it.
pub fn unified(
    a: &[&str],
    b: &[&str],
    color: bool,
    header: impl Fn(Range<usize>, Range<usize>) -> String,
    marks: impl Fn(usize, usize) -> (Vec<Range<usize>>, Vec<Range<usize>>),
) -> Option<String> {
    const CONTEXT: usize = 3;
    let ops = lcs(a, b);
    let same = |k: usize| ops[k].0.is_some() && ops[k].1.is_some();
    let mut hunks: Vec<Range<usize>> = vec![];
    for k in (0..ops.len()).filter(|&k| !same(k)) {
        let r = k.saturating_sub(CONTEXT)..(k + CONTEXT + 1).min(ops.len());
        match hunks.last_mut() {
            Some(h) if r.start <= h.end => h.end = r.end,
            _ => hunks.push(r),
        }
    }
    if hunks.is_empty() {
        return None;
    }
    let line = |sign: char, role: Role, s: &str, spans: &[Range<usize>]| {
        if !color {
            return format!("{sign} {s}");
        }
        let (mut out, mut last) = (format!("{}{sign} ", role.code()), 0);
        for r in spans {
            out += &format!("{}{}{}\x1b[27m", &s[last..r.start], Role::Changed.code(), &s[r.clone()]);
            last = r.end;
        }
        format!("{out}{}{RESET}", &s[last..])
    };
    let mut out = vec![];
    for h in hunks {
        let side = |new: bool| {
            let count = |os: &[(Option<usize>, Option<usize>)]| os.iter().filter(|o| if new { o.1 } else { o.0 }.is_some()).count();
            let start = count(&ops[..h.start]);
            start..start + count(&ops[h.clone()])
        };
        let head = header(side(false), side(true));
        out.push(if color { format!("{}{head}{RESET}", Role::Heading.code()) } else { head });
        let mut k = h.start;
        while k < h.end {
            if let (Some(i), Some(_)) = ops[k] {
                out.push(format!("  {}", a[i]));
                k += 1;
                continue;
            }
            let end = (k..h.end).find(|&e| same(e)).unwrap_or(h.end);
            let dels: Vec<usize> = ops[k..end].iter().filter_map(|o| o.0).collect();
            let adds: Vec<usize> = ops[k..end].iter().filter_map(|o| o.1).collect();
            let pairs: Vec<_> = dels.iter().zip(&adds).map(|(&i, &j)| marks(i, j)).collect();
            for (n, &i) in dels.iter().enumerate() {
                out.push(line('-', Role::Removed, a[i], pairs.get(n).map_or(&[], |p| &p.0)));
            }
            for (n, &j) in adds.iter().enumerate() {
                out.push(line('+', Role::Added, b[j], pairs.get(n).map_or(&[], |p| &p.1)));
            }
            k = end;
        }
    }
    Some(out.join("\n"))
}

/// The steps from `a` to `b` along a longest common subsequence: `(Some(i), Some(j))` where `a[i] == b[j]`,
/// `(Some(i), None)` where `a[i]` goes and `(None, Some(j))` where `b[j]` comes in.
/// ponytail: O(n*m) table, fine for thousands of lines; Myers if files get big.
fn lcs(a: &[&str], b: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
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
            out.push((Some(i), Some(j)));
            (i, j) = (i + 1, j + 1);
        } else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push((Some(i), None));
            i += 1;
        } else {
            out.push((None, Some(j)));
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{eval, show, try_eval};
    use crate::value::Value;

    #[test]
    fn compare() {
        assert_eq!(show(r#"[distance("kitten", "sitting"), distance("", "abc"), distance("form", "from")]"#), "[3, 3, 1]");
        assert_eq!(show(r#"[similarity("abc", "abc"), similarity("", ""), similarity("ab", "cd")]"#), "[1, 1, 0]");
        assert_eq!(show(r#"["BANAN".closest(["bandana", "banana", "cabana"]), "x".closest([])]"#), r#"["banana", nil]"#);
    }

    #[test]
    fn hamming() {
        assert_eq!(show(r#"hamming("karolin", "kathrin")"#), "3");
        assert_eq!(show(r#"hamming(b"this is a test", b"wokka wokka!!!")"#), "37");
        assert!(try_eval(r#"hamming("ab", "abc")"#).is_err());
        assert!(try_eval(r#"hamming(b"ab", b"abc")"#).is_err());
    }

    #[test]
    fn diff() {
        assert_eq!(show(r#"diff("a\nb\nc", "a\nB\nc\nd")"#), "@@ -1,3 +1,4 @@\n  a\n- b\n+ B\n  c\n+ d");
        assert_eq!(show(r#"diff("1\n2\n3\n4\n5\n6\n7\n8\n9", "1\n2\n3\n4\n5\n6\n7\n8\nX")"#), "@@ -6,4 +6,4 @@\n  6\n  7\n  8\n- 9\n+ X");
        assert_eq!(show(r#"diff("", "a")"#), "@@ -0,0 +1,1 @@\n+ a");
        assert_eq!(show(r#"[diff("a\nb", "a\nb"), diff(b"\xff", b"\xff")]"#), "[nil, nil]");
        assert!(show(r#"diff(b"hi", b"ho")"#).starts_with("@@ -0x0,2 +0x0,2 @@\n- 00000000: 6869"));
        assert!(try_eval(r#"diff(b"hi", "ho")"#).is_err());
        assert_eq!(show(r#"diff(date("2025-08-03"), date("2026-10-06 04:00"))"#), "1 yr 2 mo 3 d 4 h");
        assert!(try_eval(r#"diff("a", date("2025-08-03"))"#).is_err());
        let painted = super::text_diff("let total = price * qty", "let total = cost * qty", true).unwrap();
        assert!(painted.contains("\x1b[31m- let total = \x1b[7mprice\x1b[27m * qty\x1b[0m"), "{painted:?}");
        assert!(painted.contains("\x1b[32m+ let total = \x1b[7mcost\x1b[27m * qty\x1b[0m"), "{painted:?}");
        let page = crate::help::help(Some("diff"), false).unwrap();
        for sig in [
            "diff(a: str, b: str, *, plain: bool = false, bytes: bool = false, table: bool = false)",
            "diff(a: bytes, b: bytes, *, plain: bool = false, table: bool = false)",
            "diff(a: date, b: date)",
        ] {
            assert!(page.contains(sig), "{sig} missing from help diff:\n{page}");
        }
        let whole = super::text_diff("abc", "xyz", true).unwrap();
        assert!(!whole.contains("\x1b[7m"), "{whole:?}");
    }

    #[test]
    fn diff_table() {
        let rows = |src: &str| {
            let Value::Table(t) = eval(src) else { panic!("{src}: not a table") };
            t.rows.iter().map(|r| r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>()
        };
        assert_eq!(rows(r#"diff("a\nb\nc\nx\ny", "a\nB\nc", table: true)"#), [r#"2 2 b B"#, "4 nil x nil", "5 nil y nil"]);
        assert!(rows(r#"diff("a", "a", table: true)"#).is_empty());
        assert_eq!(rows(r#"diff(b"\x7fELF", b"\x7fELG\x00", table: true)"#), ["0x3 0x46 0x47", "0x4 nil 0x0"]);
        assert!(try_eval(r#"diff("a", b"a", table: true)"#).is_err());
    }
}
