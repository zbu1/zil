//! Collections: functions shared by strings, lists, maps and sets, with each of those below.

pub mod charts;
pub mod lists;
pub mod maps;
pub mod sets;
pub mod tables;

use crate::modules::Module;
use crate::value::{Value, compare};
use std::cmp::Ordering;

pub const MODULE: Module = Module {
    name: "data",
    about: "work with collections of values: sort, search and pick from lists, maps, sets and tables, and chart them",
    #[rustfmt::skip]
    examples: &[
        ("data", &[
            ("drop repeats, keeping the order", "[3, 1, 3, 2, 1].unique"),
            ("sort lengths in different units", "[1 km, 900 m, 1 mi].sort"),
            ("sort dates", r#"[date("2026-12-25"), date("2026-01-01")].sort"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("list", "[1, 2, 3]"), ("map", "{a: 1, b: 2}")]),
        ("operators", &[("x in v: is x an item of a list or set, or a key of a map?", "2 in [1, 2]")]),
        ("pretty", &[
            ("long, the default: one item per line", r#"pretty([1234567, "a"])"#),
            ("short: all on one line, each item short", r#"pretty([1234567, "a", {b: 2500 g}], "short")"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("size", &["len"]),
        ("search", &["contains", "find", "count"]),
        ("order", &["sort", "sort_desc", "reverse", "unique"]),
        ("pick", &["first", "last", "take", "drop"]),
        ("test", &["any", "all"]),
    ],
    call: fns::call,
    children: &[lists::MODULE, maps::MODULE, sets::MODULE, tables::MODULE, charts::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{char_index, sort_keyed};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::{As, Func, List, Map, Or, Or5, Set};
    use crate::value::Value;
    use regex::Regex;

    /// how many items a list, map or set has, characters a string has, or bytes a bytes value has
    #[builtin(examples = [r#""héllo".len"#, "[1, 2, 3].len", "{a: 1}.len"])]
    fn len(v: Or5<&str, &[u8], &List, &Map, &Set>) -> i64 {
        (match v {
            Or5::A(s) => s.chars().count(),
            Or5::B(b) => b.len(),
            Or5::C(l) => l.borrow().len(),
            Or5::D(m) => m.borrow().len(),
            Or5::E(s) => s.borrow().len(),
        }) as i64
    }

    /// whether x is in v: text (or a regex match) in a string, an item in a list or set, or a key in a map
    #[builtin(examples = [r#""price: $12".contains(r"\$\d+")"#, "[1, 2].contains(2)"], see = ["find", "starts_with"])]
    fn contains(s: &str, x: Or<&str, &Regex>) -> bool {
        match x {
            Or::A(sub) => s.contains(sub),
            Or::B(r) => r.is_match(s),
        }
    }

    #[builtin(name = "contains")]
    fn contains_item(xs: Or<&List, &Set>, x: &Value) -> bool {
        match xs {
            Or::A(l) => l.borrow().contains(x),
            Or::B(s) => s.borrow().contains(x),
        }
    }

    #[builtin(name = "contains")]
    fn contains_key(m: &Map, k: &str) -> bool {
        m.borrow().contains_key(k)
    }

    /// the position of the first x in v, counting from 0, or nil if it isn't there
    #[builtin(examples = [r#""hello".find("l")"#, "[5, 6].find(6)", r#""abc".find("z")"#], see = ["contains", "count"])]
    fn find(s: &str, x: Or<&str, &Regex>) -> Option<As<i64>> {
        let at = match x {
            Or::A(sub) => s.find(sub),
            Or::B(r) => r.find(s).map(|m| m.start()),
        };
        at.map(|i| char_index(s, i).into())
    }

    #[builtin(name = "find")]
    fn find_item(xs: &List, x: &Value) -> Option<i64> {
        xs.borrow().iter().position(|v| v == x).map(|i| i as i64)
    }

    /// how many times x appears in a string or list
    #[builtin(examples = [r#""banana".count("a")"#, "[1, 2, 1].count(1)"], see = ["find"])]
    fn count(s: &str, x: Or<&str, &Regex>) -> Result<i64, Fail> {
        Ok(match x {
            Or::A("") => return Err(Fail::BadArgs),
            Or::A(sub) => s.matches(sub).count(),
            Or::B(r) => r.find_iter(s).count(),
        } as i64)
    }

    #[builtin(name = "count")]
    fn count_item(xs: &List, x: &Value) -> i64 {
        xs.borrow().iter().filter(|v| *v == x).count() as i64
    }

    /// reverse a string or list
    #[builtin(examples = [r#""abc".reverse"#, "[1, 2, 3].reverse"], see = ["sort"])]
    fn reverse(v: Or<&str, &List>) -> Or<String, Vec<Value>> {
        match v {
            Or::A(s) => Or::A(s.chars().rev().collect()),
            Or::B(l) => Or::B(l.borrow().iter().rev().cloned().collect()),
        }
    }

    fn sorted(it: &mut Interp, xs: Or<&List, &Set>, key: Option<Func>, span: &Span, desc: bool) -> Result<As<Vec<Value>>, Fail> {
        let items: Vec<Value> = match xs {
            Or::A(l) => l.borrow().clone(),
            Or::B(s) => s.borrow().iter().cloned().collect(),
        };
        let mut keyed = Vec::new();
        for v in items {
            keyed.push((if let Some(f) = &key { it.call(f, vec![v.clone()], span)? } else { v.clone() }, v));
        }
        Ok(sort_keyed(&mut keyed, desc)?.into())
    }

    /// a sorted copy, smallest first
    ///
    /// Given a function, items are sorted by what it returns for each, like |w| w.len to sort words by length
    #[builtin(examples = ["[3, 1, 2].sort", r#"["ccc", "a", "bb"].sort(|w| w.len)"#], see = ["reverse", "unique"])]
    fn sort(it: &mut Interp, xs: Or<&List, &Set>, key: Option<Func>, span: &Span) -> Result<As<Vec<Value>>, Fail> {
        sorted(it, xs, key, span, false)
    }

    /// a sorted copy, largest first, with the function working as in sort
    #[builtin(examples = ["[3, 1, 2].sort_desc", r#"["bb", "a", "ccc"].sort_desc(|w| w.len)"#], see = ["sort"])]
    fn sort_desc(it: &mut Interp, xs: Or<&List, &Set>, key: Option<Func>, span: &Span) -> Result<As<Vec<Value>>, Fail> {
        sorted(it, xs, key, span, true)
    }

    /// drop repeated items, keeping the first of each in its place
    #[builtin(examples = ["[1, 2, 1, 3].unique"], see = ["sort", "count"])]
    fn unique(xs: &List) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        for v in xs.borrow().iter() {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
        out
    }

    /// the first item, or nil if the list is empty
    #[builtin(examples = ["[7, 8].first"], see = ["last"])]
    fn first(xs: &List) -> Value {
        xs.borrow().first().cloned().unwrap_or(Value::Nil)
    }

    /// the last item, or nil if the list is empty
    #[builtin(examples = ["[7, 8].last"], see = ["first"])]
    fn last(xs: &List) -> Value {
        xs.borrow().last().cloned().unwrap_or(Value::Nil)
    }

    fn count_arg(n: i64) -> Result<usize, Fail> {
        usize::try_from(n).map_err(|_| Fail::Arg(1, format!("count must be at least 0, got {n}")))
    }

    /// the first n items or characters
    #[builtin(examples = ["[1, 2, 3].take(2)", r#""hello".take(3)"#], see = ["drop", "first"])]
    fn take(v: Or<&str, &List>, n: i64) -> Result<Or<String, Vec<Value>>, Fail> {
        let n = count_arg(n)?;
        Ok(match v {
            Or::A(s) => Or::A(s.chars().take(n).collect()),
            Or::B(l) => Or::B(l.borrow().iter().take(n).cloned().collect()),
        })
    }

    /// everything except the first n items or characters
    #[builtin(examples = ["[1, 2, 3].drop(2)", r#""hello".drop(3)"#], see = ["take", "last"])]
    fn drop(v: Or<&str, &List>, n: i64) -> Result<Or<String, Vec<Value>>, Fail> {
        let n = count_arg(n)?;
        Ok(match v {
            Or::A(s) => Or::A(s.chars().skip(n).collect()),
            Or::B(l) => Or::B(l.borrow().iter().skip(n).cloned().collect()),
        })
    }

    fn test(it: &mut Interp, xs: &List, f: Option<Func>, span: &Span, want: bool) -> Result<bool, Fail> {
        for v in xs.borrow().clone() {
            let t = match &f {
                Some(f) => it.call(f, vec![v], span)?.truthy(),
                None => v.truthy(),
            };
            if t == want {
                return Ok(want);
            }
        }
        Ok(!want)
    }

    /// whether at least one item passes the test f, or without f, whether any item is itself truthy
    #[builtin(examples = ["[0, 5, 12].any(|x| x > 10)", "[nil, false].any"], see = ["all", "filter"])]
    fn any(it: &mut Interp, xs: &List, f: Option<Func>, span: &Span) -> Result<bool, Fail> {
        test(it, xs, f, span, true)
    }

    /// whether every item passes the test f, or without f, whether every item is itself truthy
    ///
    /// An empty list gives true
    #[builtin(examples = ["[2, 4, 6].all(|x| x % 2 == 0)", "[].all"], see = ["any", "filter"])]
    fn all(it: &mut Interp, xs: &List, f: Option<Func>, span: &Span) -> Result<bool, Fail> {
        test(it, xs, f, span, false)
    }
}

fn char_index(s: &str, byte: usize) -> Value {
    Value::int(s[..byte].chars().count() as i64)
}

/// Stable sort of (key, value) pairs by key, largest first if `desc`; errors on incomparable keys.
fn sort_keyed(keyed: &mut [(Value, Value)], desc: bool) -> Result<Value, String> {
    let mut bad = None;
    keyed.sort_by(|(a, _), (b, _)| {
        let ord = compare(a, b).unwrap_or_else(|| {
            bad = Some(format!("cannot compare {} and {}", crate::error::short(a), crate::error::short(b)));
            Ordering::Equal
        });
        if desc { ord.reverse() } else { ord }
    });
    match bad {
        Some(e) => Err(e),
        None => Ok(Value::list(keyed.iter().map(|(_, v)| v.clone()).collect())),
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn data() {
        assert_eq!(show("[3, 1, 2].sort"), "[1, 2, 3]");
        assert_eq!(show(r#"["bb", "a", "ccc"].sort(|s| s.len)"#), r#"["a", "bb", "ccc"]"#);
        assert_eq!(show("[1, 2, 2, 3].unique"), "[1, 2, 3]");
        assert_eq!(show(r#"["héllo".len, "banana".count("a"), "hello".find("l"), [5, 6].find(7)]"#), "[5, 3, 2, nil]");
        assert_eq!(show(r#"[[3, 1, 2].sort_desc, [[1, "a"], [2, "b"], [1, "c"]].sort_desc(|p| p[0])]"#), r#"[[3, 2, 1], [[2, "b"], [1, "a"], [1, "c"]]]"#);
        assert_eq!(show(r#"[[1, 2, 3].take(5), [1, 2, 3].drop(1), "hello".take(2), "hello".drop(9)]"#), r#"[[1, 2, 3], [2, 3], "he", ""]"#);
        assert_eq!(show("[[1, nil].any, [1, nil].all, [].any, [].all, [1, 2].all(|x| x > 0)]"), "[true, false, false, true, true]");
        assert_eq!(show(r#"["abc".reverse, [1, 2].reverse, {k: 1}.contains("k")]"#), r#"["cba", [2, 1], true]"#);
    }
}
