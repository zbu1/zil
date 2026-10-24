//! Collections: functions shared by strings, lists, maps and sets, with each of those below.

pub mod charts;
pub mod lists;
pub mod maps;
pub mod sets;
pub mod tables;

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::{Value, compare};
use std::cmp::Ordering;

pub const MODULE: Module = Module {
    name: "data",
    about: "sorting and searching any collection; lists, maps, tables, charts below",
    #[rustfmt::skip]
    examples: &[
        ("data", &[
            ("unique, order kept", "[3, 1, 3, 2, 1].unique"),
            ("sort quantities", "[1 km, 900 m, 1 mi].sort"),
            ("sort dates", r#"[date("2026-12-25"), date("2026-01-01")].sort"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("list", "[1, 2, 3]"), ("map", "{a: 1, b: 2}")]),
        ("operators", &[("x in v", "2 in [1, 2]")]),
        ("pretty", &[
            ("long: one item per line, indented, items pretty, strings quoted", ""),
            ("short: one line, items short", r#"pretty([1234567, "a", {b: 2500 g}], "short")"#),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("size", &["len"]),
        ("search", &["contains", "find", "count"]),
        ("order", &["sort", "sort_desc", "reverse", "unique"]),
        ("pick", &["first", "last", "take", "drop"]),
        ("test", &["any", "all"]),
    ],
    call,
    children: &[lists::MODULE, maps::MODULE, sets::MODULE, tables::MODULE, charts::MODULE],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("len", "len(v: str|bytes|list|map|set)", "length of a string, bytes, list, map or set",&[r#""héllo".len"#, "[1, 2, 3].len", "{a: 1}.len"], &[]),
    doc("contains", "contains(v: str|list|map|set, x: any)", "substring/regex in a string, item in a list or set, key in a map", &[r#""price: $12".contains(r"\$\d+")"#, "[1, 2].contains(2)"], &["find", "starts_with"]),
    doc("find", "find(v: str|list, x: any)", "index of the first match, or nil", &[r#""hello".find("l")"#, "[5, 6].find(6)", r#""abc".find("z")"#], &["contains", "count"]),
    doc("count", "count(v: str|list, x: any)", "number of matches in a string or list", &[r#""banana".count("a")"#, "[1, 2, 1].count(1)"], &["find"]),
    doc("reverse", "reverse(v: str|list)", "reverse a string or list", &[r#""abc".reverse"#, "[1, 2, 3].reverse"], &["sort"]),
    doc("sort", "sort(xs: list|set, key?: fn)", "sorted list, optionally by key function", &["[3, 1, 2].sort", r#"["ccc", "a", "bb"].sort(|w| w.len)"#], &["reverse", "unique"]),
    doc("sort_desc", "sort_desc(xs: list|set, key?: fn)", "like sort, largest first", &["[3, 1, 2].sort_desc", r#"["bb", "a", "ccc"].sort_desc(|w| w.len)"#], &["sort"]),
    doc("unique", "unique(xs: list)", "drop duplicates, keeping first occurrences", &["[1, 2, 1, 3].unique"], &["sort", "count"]),
    doc("first", "first(xs: list)", "first item, or nil", &["[7, 8].first"], &["last"]),
    doc("last", "last(xs: list)", "last item, or nil", &["[7, 8].last"], &["first"]),
    doc("take", "take(v: str|list, n: int)", "the first n items or characters", &["[1, 2, 3].take(2)", r#""hello".take(3)"#], &["drop", "first"]),
    doc("drop", "drop(v: str|list, n: int)", "everything after the first n items or characters", &["[1, 2, 3].drop(2)", r#""hello".drop(3)"#], &["take", "last"]),
    doc("any", "any(xs: list, f?: fn)", "true if f (or the item itself) is truthy for some item", &["[0, 5, 12].any(|x| x > 10)", "[nil, false].any"], &["all", "filter"]),
    doc("all", "all(xs: list, f?: fn)", "true if f (or the item itself) is truthy for every item", &["[2, 4, 6].all(|x| x % 2 == 0)", "[].all"], &["any", "filter"]),
];

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("len", [Str(s)]) => Value::int(s.chars().count() as i64),
        ("len", [Bytes(b)]) => Value::int(b.len() as i64),
        ("len", [List(l)]) => Value::int(l.borrow().len() as i64),
        ("len", [Map(m)]) => Value::int(m.borrow().len() as i64),
        ("len", [Set(s)]) => Value::int(s.borrow().len() as i64),
        ("contains", [Str(s), Str(sub)]) => Bool(s.contains(&**sub)),
        ("contains", [Str(s), Regex(r)]) => Bool(r.is_match(s)),
        ("contains", [List(l), v]) => Bool(l.borrow().contains(v)),
        ("contains", [Map(m), Str(k)]) => Bool(m.borrow().contains_key(&**k)),
        ("contains", [Set(s), v]) => Bool(s.borrow().contains(v)),
        ("sort" | "sort_desc", [Set(s), rest @ ..]) => {
            let xs = Value::list(s.borrow().iter().cloned().collect());
            return call(it, name, &[&[xs], rest].concat(), span);
        }
        ("find", [Str(s), Str(sub)]) => s.find(&**sub).map_or(Nil, |i| char_index(s, i)),
        ("find", [Str(s), Regex(r)]) => r.find(s).map_or(Nil, |m| char_index(s, m.start())),
        ("find", [List(l), v]) => l.borrow().iter().position(|x| x == v).map_or(Nil, |i| Value::int(i as i64)),
        ("count", [Str(s), Str(sub)]) if !sub.is_empty() => Value::int(s.matches(&**sub).count() as i64),
        ("count", [Str(s), Regex(r)]) => Value::int(r.find_iter(s).count() as i64),
        ("count", [List(l), v]) => Value::int(l.borrow().iter().filter(|x| *x == v).count() as i64),
        ("reverse", [Str(s)]) => Value::str(s.chars().rev().collect::<String>()),
        ("reverse", [List(l)]) => Value::list(l.borrow().iter().rev().cloned().collect()),
        ("sort" | "sort_desc", [List(l)]) => {
            let mut keyed: Vec<_> = l.borrow().iter().map(|v| (v.clone(), v.clone())).collect();
            sort_keyed(&mut keyed, name == "sort_desc")?
        }
        ("sort" | "sort_desc", [List(l), f]) => {
            let mut keyed = Vec::new();
            for v in l.borrow().clone() {
                keyed.push((it.call(f, vec![v.clone()], span)?, v));
            }
            sort_keyed(&mut keyed, name == "sort_desc")?
        }
        ("unique", [List(l)]) => {
            let mut out: Vec<Value> = Vec::new();
            for v in l.borrow().iter() {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Value::list(out)
        }
        ("first", [List(l)]) => l.borrow().first().cloned().unwrap_or(Nil),
        ("last", [List(l)]) => l.borrow().last().cloned().unwrap_or(Nil),
        ("take" | "drop", [_, Int(n, _)]) if *n < 0 => return Err(Fail::Arg(1, format!("count must be at least 0, got {n}"))),
        ("take", [List(l), Int(n, _)]) => Value::list(l.borrow().iter().take(*n as usize).cloned().collect()),
        ("drop", [List(l), Int(n, _)]) => Value::list(l.borrow().iter().skip(*n as usize).cloned().collect()),
        ("take", [Str(s), Int(n, _)]) => Value::str(s.chars().take(*n as usize).collect::<String>()),
        ("drop", [Str(s), Int(n, _)]) => Value::str(s.chars().skip(*n as usize).collect::<String>()),
        ("any" | "all", [List(l), f @ ..]) if f.len() <= 1 => {
            let want = name == "any";
            for v in l.borrow().clone() {
                let t = match f {
                    [f] => it.call(f, vec![v], span)?.truthy(),
                    _ => v.truthy(),
                };
                if t == want {
                    return Ok(Bool(want));
                }
            }
            Bool(!want)
        }
        _ => return Err(Fail::BadArgs),
    })
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
