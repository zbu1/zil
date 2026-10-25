//! Sets: unique values in insertion order, with set algebra as operators.

use crate::ast::BinOp;
use crate::error::short;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::Value;
use indexmap::IndexSet;

pub const MODULE: Module = Module {
    name: "sets",
    about: "collections with no repeats: combine, compare and build sets",
    #[rustfmt::skip]
    intro: &[
        ("", "\
A set is a collection where each value appears at most once. Adding a value that's already there does nothing, and two \
sets are equal when they hold the same values in any order."),
        ("good to know", "\
Sets remember the order values were first added in, and show them in that order. They can hold integers, strings, \
true, false and nil, but not floats, lists or maps."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("sets", &[
            ("distinct words", r#""the cat and the hat".split.set"#),
            ("in both", "set(1, 2, 3) & set(2, 3, 4)"),
            ("in either, not both", "set(1, 2, 3) ^ set(2, 3, 4)"),
            ("order doesn't matter", "set(1, 2) == set(2, 1)"),
            ("squares, written like math's set-builder notation", "{x ** 2 | x in 1..=5}"),
            ("primes below 20", "{x in 1..20 | is_prime(x)}"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("set", "set(1, 2, 3)")]),
        ("syntax", &[("{f(x) | x in xs}: f(x) for each x", "{x * 2 | x in [1, 2, 2]}"), ("{x in xs | test}: each x that passes", "{c in \"hello\" | c != \"l\"}")]),
        ("operators", &[("a | b: in either (union)", "set(1, 2) | set(2, 3)"), ("a & b: in both (intersection)", "set(1, 2) & set(2, 3)"),
            ("a - b: in a but not b (difference)", "set(1, 2) - set(2, 3)"), ("a ^ b: in one but not both", "set(1, 2) ^ set(2, 3)"), ("x in s: is x in the set?", "2 in set(1, 2)")]),
        ("pretty", &[
            ("long, the default: one item per line", r#"pretty(set(1500, "a"))"#),
            ("short: all on one line, each item short", r#"pretty(set(1500, "a"), "short")"#),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("make", &["set", "power_set", "cartesian"]),
        ("compare", &["is_subset", "is_superset", "is_disjoint"]),
        ("change", &["add", "remove"]),
    ],
    call,
    targets: &[("set", "set")],
    binary: Some(binary),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("set", "set(xs: list|str|set) -> set / set(a?: any, ...) -> set", "a set of a list's items or a string's characters, or of all the arguments, with repeats dropped", &["[3, 1, 3, 2].set", "set(1, 2)", r#""hello" to set"#], &["unique", "list"]).pretty(),
    doc("power_set", "power_set(s: set) -> list", "every set that can be made from some of s's items, including the empty set and s itself, smallest first", &["set(1, 2, 3).power_set"], &["combinations", "cartesian"]),
    doc("cartesian", "cartesian(a: set|list, b: set|list) -> list", "every pair [x, y] with x from a and y from b", &[r#"cartesian(set(1, 2), set("a", "b"))"#], &["power_set", "combinations"]),
    doc("is_subset", "is_subset(a: set, b: set) -> bool", "whether every item of a is also in b", &["set(1, 2).is_subset(set(1, 2, 3))"], &["is_superset"]),
    doc("is_superset", "is_superset(a: set, b: set) -> bool", "whether a has every item of b", &["set(1, 2, 3).is_superset(set(1, 5))"], &["is_subset"]),
    doc("is_disjoint", "is_disjoint(a: set, b: set) -> bool", "whether a and b have no items in common", &["set(1, 2).is_disjoint(set(3))"], &["is_subset"]),
    doc("add", "add(s: set, v: any) -> set", "add v to the set, changing it, and return the set", &["set(1).add(2)"], &["remove", "push"]).mutates(),
    doc("remove", "remove(s: set, v: any) -> set", "remove v from the set if it's there, changing it, and return the set", &["set(1, 2).remove(1)"], &["add", "del"]).mutates(),
];

/// A set of `items`, blaming argument `arg` for the first one a set can't hold.
fn collect(items: impl IntoIterator<Item = Value>, arg: usize) -> Result<IndexSet<Value>, Fail> {
    items.into_iter().map(|v| if v.hashable() { Ok(v) } else { Err(Fail::Arg(arg, unhashable(&v))) }).collect()
}

fn unhashable(v: &Value) -> String {
    format!("sets hold int, str, bool or nil, got {}", short(v))
}

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("set", [List(l)]) => Value::set(collect(l.borrow().clone(), 0)?),
        ("set", [Str(s)]) => Value::set(s.chars().map(|c| Value::str(c.to_string())).collect()),
        ("set", [Set(s)]) => Value::set(s.borrow().clone()),
        ("set", items) => Value::set(collect(items.to_vec(), 0)?),
        ("power_set", [Set(s)]) => {
            let items: Vec<Value> = s.borrow().iter().cloned().collect();
            if items.len() > 30 {
                return Err(Fail::Arg(0, format!("a set of {} items has too many subsets", items.len())));
            }
            crate::state::check_len(1 << items.len(), "items")?;
            let mut subsets: Vec<u64> = (0..1u64 << items.len()).collect();
            subsets.sort_by_key(|m| m.count_ones());
            let pick = |m: u64| Value::set(items.iter().enumerate().filter(|(i, _)| m >> i & 1 == 1).map(|(_, v)| v.clone()).collect());
            Value::list(subsets.into_iter().map(pick).collect())
        }
        ("cartesian", [a, b]) => {
            let items = |v: &Value| match v {
                Set(s) => Some(s.borrow().iter().cloned().collect::<Vec<_>>()),
                List(l) => Some(l.borrow().clone()),
                _ => None,
            };
            let (a, b) = (items(a).ok_or(Fail::BadArgs)?, items(b).ok_or(Fail::BadArgs)?);
            crate::state::check_len(a.len() * b.len(), "items")?;
            Value::list(a.iter().flat_map(|x| b.iter().map(move |y| Value::list(vec![x.clone(), y.clone()]))).collect())
        }
        ("is_subset", [Set(a), Set(b)]) => Bool(a.borrow().is_subset(&b.borrow())),
        ("is_superset", [Set(a), Set(b)]) => Bool(a.borrow().is_superset(&b.borrow())),
        ("is_disjoint", [Set(a), Set(b)]) => Bool(a.borrow().is_disjoint(&b.borrow())),
        ("add", [Set(_), v]) if !v.hashable() => return Err(Fail::Arg(1, unhashable(v))),
        ("add", [Set(s), v]) => {
            s.borrow_mut().insert(v.clone());
            Set(s.clone())
        }
        ("remove", [Set(s), v]) => {
            s.borrow_mut().shift_remove(v);
            Set(s.clone())
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// `|` union, `&` intersection, `-` difference, `^` symmetric difference.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    let (Value::Set(x), Value::Set(y)) = (a, b) else { return None };
    let (x, y) = (x.borrow(), y.borrow());
    if matches!(op, BinOp::BitOr | BinOp::BitXor)
        && let Err(e) = crate::state::check_len(x.len() + y.len(), "items")
    {
        return Some(Err(e));
    }
    Some(Ok(Value::set(match op {
        BinOp::BitOr => x.union(&y).cloned().collect(),
        BinOp::BitAnd => x.intersection(&y).cloned().collect(),
        BinOp::Sub => x.difference(&y).cloned().collect(),
        BinOp::BitXor => x.symmetric_difference(&y).cloned().collect(),
        _ => return None,
    })))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn sets() {
        assert_eq!(show("[[3, 1, 3, 2].set, set(1, 2), set(), [1, 2] to set, \"aba\".set]"), r#"[set(3, 1, 2), set(1, 2), set(), set(1, 2), set("a", "b")]"#);
        assert_eq!(show("a = set(1, 2, 3); b = set(2, 3, 4); [a | b, a & b, a - b, a ^ b]"), "[set(1, 2, 3, 4), set(2, 3), set(1), set(1, 4)]");
        assert_eq!(show("[set(1, 2) == set(2, 1), set(1) == set(1, 2), 2 in set(1, 2), 0xff in set(255)]"), "[true, false, true, true]");
        assert_eq!(show("[set(1).is_subset(set(1, 2)), set(1).is_superset(set(1, 2)), set(1).is_disjoint(set(2))]"), "[true, false, true]");
        assert_eq!(show("s = set(1); s.add(2); s.add(1); s.remove(1); s.remove(9); s"), "set(2)");
        assert_eq!(show("s = set(3, 1, 2); [s.len, s.contains(1), s.sort, s.list, s.sort_desc]"), "[3, true, [1, 2, 3], [3, 1, 2], [3, 2, 1]]");
        assert_eq!(show("t = 0; for x in set(1, 2, 2) { t += x }; t"), "3");
        assert_eq!(show(r#"s = set(1, "a", nil, true); str(s).parse == s"#), "true");
        assert_eq!(
            show("[{x ** 2 | x in 1..=4}, {x % 3 | x in [1, 4, 5]}, {x in 1..20 | is_prime(x)}, {x in set(1, 2) | x > 5}, {x > 1 | x in [1, 2]}]"),
            "[set(1, 4, 9, 16), set(1, 2), set(2, 3, 5, 7, 11, 13, 17, 19), set(), set(false, true)]"
        );
        assert_eq!(
            show(
                "[{a: 1}.a, {}.len, {
  b: 2
}.b, {f(1) | f in [|x| x + 1]}]"
            ),
            "[1, 0, 2, set(2)]"
        );
        assert_eq!(show("set(1, 2, 3).power_set"), "[set(), set(1), set(2), set(3), set(1, 2), set(1, 3), set(2, 3), set(1, 2, 3)]");
        assert_eq!(show("[cartesian(set(1, 2), [3]), cartesian([], [1])]"), "[[[1, 3], [2, 3]], []]");
        for bad in ["{1}", "{x | 5}", "{1 | 2 in xs}", "{x in [1] | x", "set([1], 2)", "set(1).add([2])", "set(1.5)", "[[1]].set"] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }
}
