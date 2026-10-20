//! Lists: ranges and higher-order fns.

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::Value;
use indexmap::IndexMap;

pub const MODULE: Module = Module {
    name: "lists",
    about: "ranges, map/filter/reduce, building lists",
    #[rustfmt::skip]
    examples: &[
        ("lists", &[
            ("stepped range", "(1..=10).step(3)"),
            ("primes under 100", "(1..100).filter(is_prime).len"),
            ("pipe into a function", r#"(1..=10).map(|x| x ** 2) |> sum"#),
            ("fold", r#"[1, 2, 3].reduce(10, |acc, x| acc + x)"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("a..b", "1..4"), ("a..=b", "1..=4"), ("list + list", "[1] + [2, 3]")]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("build", &["range", "push", "step"]),
        ("stack and queue", &["pop", "shift", "unshift"]),
        ("transform", &["map", "filter", "reduce", "flatten"]),
        ("combine", &["zip", "enumerate"]),
        ("group", &["group_by", "count_by", "chunks", "windows"]),
    ],
    call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("range", "range(n: int) / range(a: int, b: int)", "integers in [0, n) or [a, b); same as a..b", &["range(4)", "range(2, 5)"], &["map"]),
    doc("push", "push(xs: list, v: any)", "append v in place and return the list", &["[1, 2].push(3)"], &["pop", "unshift"]).mutates(),
    doc("pop", "pop(xs: list)", "remove and return the last item in place, or nil if empty", &["xs = [1, 2, 3]; [xs.pop, xs]"], &["push", "shift"]).mutates(),
    doc("shift", "shift(xs: list)", "remove and return the first item in place, or nil if empty", &["xs = [1, 2, 3]; [xs.shift, xs]"], &["unshift", "pop"]).mutates(),
    doc("unshift", "unshift(xs: list, v: any)", "insert v at the front in place and return the list", &["[2, 3].unshift(1)"], &["shift", "push"]).mutates(),
    doc("map", "map(xs: list, f: fn)", "apply f to every item", &[r"[1, 2, 3].map(|x| x * 10)"], &["filter", "reduce"]),
    doc("filter", "filter(xs: list, f: fn) / filter(m: map, f: fn)", "keep items where f is truthy; for maps, f gets (key, value)", &[r"(1..10).filter(|x| x % 3 == 0)", r"{a: 1, b: 5}.filter(|k, v| v > 2)"], &["map", "reduce", "filter_keys"]),
    doc("reduce", "reduce(xs: list, init: any, f: fn)", "fold with f(acc, item)", &[r"[1, 2, 3].reduce(10, |acc, x| acc + x)"], &["sum", "map"]),
    doc("flatten", "flatten(xs: list)", "unpack nested lists one level", &["[[1, 2], [3], 4].flatten"], &["chunks"]),
    doc("zip", "zip(a?: list, ...)", "group items by position, stopping at the shortest list; zip(*rows) gives the columns", &[r#"zip([1, 2, 3], ["a", "b"])"#, "zip(*[[1, 2], [3, 4], [5, 6]])"], &["enumerate"]),
    doc("enumerate", "enumerate(xs: list)", "[index, item] pairs", &[r#"["a", "b"].enumerate"#, r#"["a", "b"].enumerate.map(|[i, x]| "{i}:{x}")"#], &["zip"]),
    doc("group_by", "group_by(xs: list, f: fn)", "map from each key f gives to the items with that key", &[r#"["apple", "avocado", "banana"].group_by(|w| w[0])"#, "(1..=6).group_by(|x| x % 2 == 0)"], &["count_by"]),
    doc("count_by", "count_by(xs: list, f: fn)", "map from each key f gives to how many items have it", &[r#""mississippi".chars.count_by(|c| c)"#], &["group_by", "count"]),
    doc("chunks", "chunks(xs: list, n: int)", "split into lists of n items; the last may be shorter", &["(1..=7).chunks(3)"], &["windows", "flatten"]),
    doc("windows", "windows(xs: list, n: int)", "every run of n neighboring items", &["[1, 2, 3, 4].windows(2)", "[1, 4, 9, 16].windows(2).map(|[a, b]| b - a)"], &["chunks"]),
    doc("step", "step(xs: list, n: int)", "every nth item, starting with the first", &["(0..=20).step(5)", "(1..10).step(2)"], &["range"]),
];

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("range", [Int(n, _)]) => range(0, *n)?,
        ("range", [Int(a, _), Int(b, _)]) => range(*a, *b)?,
        ("push", [List(l), v]) => {
            l.borrow_mut().push(v.clone());
            List(l.clone())
        }
        ("pop", [List(l)]) => l.borrow_mut().pop().unwrap_or(Nil),
        // ponytail: O(n) shift/unshift on a Vec, VecDeque if queues get long
        ("shift", [List(l)]) => {
            let mut l = l.borrow_mut();
            if l.is_empty() { Nil } else { l.remove(0) }
        }
        ("unshift", [List(l), v]) => {
            l.borrow_mut().insert(0, v.clone());
            List(l.clone())
        }
        ("map", [List(l), f]) => {
            let items = l.borrow().clone();
            let mut out = Vec::with_capacity(items.len());
            for v in items {
                out.push(it.call(f, vec![v], span)?);
            }
            Value::list(out)
        }
        ("filter", [List(l), f]) => {
            let items = l.borrow().clone();
            let mut out = Vec::new();
            for v in items {
                if it.call(f, vec![v.clone()], span)?.truthy() {
                    out.push(v);
                }
            }
            Value::list(out)
        }
        ("filter", [Map(m), f]) => {
            let mut out = IndexMap::new();
            for (k, v) in m.borrow().clone() {
                if it.call(f, vec![Value::str(k.as_str()), v.clone()], span)?.truthy() {
                    out.insert(k, v);
                }
            }
            Value::map(out)
        }
        ("reduce", [List(l), init, f]) => {
            let items = l.borrow().clone();
            let mut acc = init.clone();
            for v in items {
                acc = it.call(f, vec![acc, v], span)?;
            }
            acc
        }
        ("flatten", [List(l)]) => {
            crate::state::check_len(l.borrow().iter().map(|v| if let List(i) = v { i.borrow().len() } else { 1 }).sum(), "items").map_err(Fail::Msg)?;
            Value::list(
                l.borrow()
                    .iter()
                    .flat_map(|v| match v {
                        List(inner) => inner.borrow().clone(),
                        v => vec![v.clone()],
                    })
                    .collect(),
            )
        }
        ("zip", ls) if ls.iter().all(|l| matches!(l, List(_))) => {
            let ls: Vec<_> = ls.iter().map(|l| if let List(l) = l { l.borrow().clone() } else { unreachable!() }).collect();
            let n = ls.iter().map(Vec::len).min().unwrap_or(0);
            Value::list((0..n).map(|i| Value::list(ls.iter().map(|l| l[i].clone()).collect())).collect())
        }
        ("enumerate", [List(l)]) => Value::list(l.borrow().iter().enumerate().map(|(i, v)| Value::list(vec![Value::int(i as i64), v.clone()])).collect()),
        ("group_by" | "count_by", [List(l), f]) => {
            let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
            for v in l.borrow().clone() {
                groups.entry(it.call(f, vec![v.clone()], span)?.to_string()).or_default().push(v);
            }
            let count = name == "count_by";
            Value::map(groups.into_iter().map(|(k, vs)| (k, if count { Value::int(vs.len() as i64) } else { Value::list(vs) })).collect())
        }
        ("chunks" | "windows", [List(_), Int(n, _)]) if *n < 1 => return Err(Fail::Arg(1, format!("size must be at least 1, got {n}"))),
        ("chunks", [List(l), Int(n, _)]) => Value::list(l.borrow().chunks(*n as usize).map(|c| Value::list(c.to_vec())).collect()),
        ("windows", [List(l), Int(n, _)]) => Value::list(l.borrow().windows(*n as usize).map(|c| Value::list(c.to_vec())).collect()),
        ("step", [List(l), Int(n, _)]) if *n > 0 => Value::list(l.borrow().iter().step_by(*n as usize).cloned().collect()),
        ("step", [List(_), Int(n, _)]) => return Err(Fail::Arg(1, format!("step must be at least 1, got {n}"))),
        _ => return Err(Fail::BadArgs),
    })
}

/// `a..b` ranges and list concatenation.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::*;
    Some(match (op, a, b) {
        (BinOp::Range, Int(x, _), Int(y, _)) => range(*x, *y),
        (BinOp::RangeIncl, Int(x, _), Int(y, _)) => y.checked_add(1).ok_or_else(|| "range too large".into()).and_then(|y| range(*x, y)),
        (BinOp::Add, List(x), List(y)) => crate::state::check_len(x.borrow().len() + y.borrow().len(), "items")
            .map(|_| Value::list(x.borrow().iter().chain(y.borrow().iter()).cloned().collect())),
        _ => return None,
    })
}

fn range(a: i64, b: i64) -> Result<Value, String> {
    crate::state::check_len(b.saturating_sub(a).try_into().unwrap_or(0), "items")?;
    Ok(Value::list((a..b).map(Value::int).collect()))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn lists() {
        assert_eq!(show("range(5).reduce(0, |acc, x| acc + x)"), "10");
        assert_eq!(show("(1..10).filter(|x| x % 3 == 0).map(|x| x * 2)"), "[6, 12, 18]");
        assert_eq!(show("[[1] + [2], (0..=6).step(3)]"), "[[1, 2], [0, 3, 6]]");
        assert_eq!(show("xs = [1, 2, 3]; [xs.pop, xs.shift, xs.unshift(0), [].pop, [].shift]"), "[3, 1, [0, 2], nil, nil]");
        assert_eq!(show("[[[1, 2], [3], 4].flatten, zip([1, 2, 3], [4, 5]), [7, 8].enumerate]"), "[[1, 2, 3, 4], [[1, 4], [2, 5]], [[0, 7], [1, 8]]]");
        assert_eq!(show(r#"[(1..=5).group_by(|x| x % 2), ["a", "b", "a"].count_by(|c| c)]"#), "[{1: [1, 3, 5], 0: [2, 4]}, {a: 2, b: 1}]");
        assert_eq!(show("[(1..=5).chunks(2), [1, 2, 3].windows(2), [1].windows(2)]"), "[[[1, 2], [3, 4], [5]], [[1, 2], [2, 3]], []]");
    }
}
