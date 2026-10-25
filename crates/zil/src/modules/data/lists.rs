//! Lists: ranges and higher-order fns.

use crate::ast::BinOp;
use crate::modules::{Claim, Module};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "lists",
    about: "build lists, change every item, keep some, combine them, and group them",
    #[rustfmt::skip]
    examples: &[
        ("lists", &[
            ("every third number", "(1..=10).step(3)"),
            ("primes under 100", "(1..100).filter(is_prime).len"),
            ("the sum of the first 10 squares", r#"(1..=10).map(|x| x ** 2) |> sum"#),
            ("combine items one at a time, starting from 10", r#"[1, 2, 3].reduce(10, |acc, x| acc + x)"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("a..b counts from a up to just before b", "1..4"), ("a..=b includes b", "1..=4"), ("list + list joins them", "[1] + [2, 3]")]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("build", &["range", "push", "step"]),
        ("stack and queue", &["pop", "shift", "unshift"]),
        ("transform", &["map", "filter", "reduce", "flatten"]),
        ("combine", &["zip", "enumerate"]),
        ("group", &["group_by", "count_by", "chunks", "windows"]),
    ],
    call: fns::call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::range;
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::{Func, List, Map, Rest};
    use crate::value::Value;
    use indexmap::IndexMap;

    /// the integers from 0 up to n, or from a up to b, not including the end
    ///
    /// Same as a..b
    #[builtin(name = "range", examples = ["range(4)"], see = ["map"])]
    fn range_to(n: i64) -> Result<Vec<Value>, String> {
        range(0, n)
    }

    #[builtin(name = "range", examples = ["range(2, 5)"])]
    fn range_from(a: i64, b: i64) -> Result<Vec<Value>, String> {
        range(a, b)
    }

    /// add v to the end of the list, changing it, and return the list
    #[builtin(examples = ["[1, 2].push(3)"], see = ["pop", "unshift"], mutates)]
    fn push(xs: &List, v: &Value) -> List {
        xs.borrow_mut().push(v.clone());
        xs.clone()
    }

    /// take the last item off the list, changing it, and return that item, or nil if the list is empty
    #[builtin(examples = ["xs = [1, 2, 3]; [xs.pop, xs]"], see = ["push", "shift"], mutates)]
    fn pop(xs: &List) -> Value {
        xs.borrow_mut().pop().unwrap_or(Value::Nil)
    }

    /// take the first item off the list, changing it, and return that item, or nil if the list is empty
    #[builtin(examples = ["xs = [1, 2, 3]; [xs.shift, xs]"], see = ["unshift", "pop"], mutates)]
    fn shift(xs: &List) -> Value {
        // ponytail: O(n) shift/unshift on a Vec, VecDeque if queues get long
        let mut l = xs.borrow_mut();
        if l.is_empty() { Value::Nil } else { l.remove(0) }
    }

    /// add v to the front of the list, changing it, and return the list
    #[builtin(examples = ["[2, 3].unshift(1)"], see = ["shift", "push"], mutates)]
    fn unshift(xs: &List, v: &Value) -> List {
        xs.borrow_mut().insert(0, v.clone());
        xs.clone()
    }

    /// a new list of f(item) for every item
    #[builtin(examples = [r"[1, 2, 3].map(|x| x * 10)"], see = ["filter", "reduce"])]
    fn map(it: &mut Interp, xs: &List, f: Func, span: &Span) -> Result<Vec<Value>, Fail> {
        let items = xs.borrow().clone();
        let mut out = Vec::with_capacity(items.len());
        for v in items {
            out.push(it.call(&f, vec![v], span)?);
        }
        Ok(out)
    }

    /// keep only the items that pass the test f
    ///
    /// For a map, f is given each key and value, and the entries that pass are kept
    #[builtin(examples = [r"(1..10).filter(|x| x % 3 == 0)"], see = ["map", "reduce", "filter_keys"])]
    fn filter(it: &mut Interp, xs: &List, f: Func, span: &Span) -> Result<Vec<Value>, Fail> {
        let items = xs.borrow().clone();
        let mut out = Vec::new();
        for v in items {
            if it.call(&f, vec![v.clone()], span)?.truthy() {
                out.push(v);
            }
        }
        Ok(out)
    }

    #[builtin(name = "filter", examples = [r"{a: 1, b: 5}.filter(|k, v| v > 2)"])]
    fn filter_map(it: &mut Interp, m: &Map, f: Func, span: &Span) -> Result<IndexMap<String, Value>, Fail> {
        let mut out = IndexMap::new();
        for (k, v) in m.borrow().clone() {
            if it.call(&f, vec![Value::str(k.as_str()), v.clone()], span)?.truthy() {
                out.insert(k, v);
            }
        }
        Ok(out)
    }

    /// combine the items into one value
    ///
    /// Starting from init, f(so_far, item) is called for each item in turn, and what it returns is passed on as so_far to the next
    #[builtin(examples = [r"[1, 2, 3].reduce(10, |acc, x| acc + x)"], see = ["sum", "map"])]
    fn reduce(it: &mut Interp, xs: &List, init: &Value, f: Func, span: &Span) -> Result<Value, Fail> {
        let items = xs.borrow().clone();
        let mut acc = init.clone();
        for v in items {
            acc = it.call(&f, vec![acc, v], span)?;
        }
        Ok(acc)
    }

    /// replace each list inside the list with its items, one level deep
    #[builtin(examples = ["[[1, 2], [3], 4].flatten"], see = ["chunks"])]
    fn flatten(xs: &List) -> Result<Vec<Value>, String> {
        let l = xs.borrow();
        crate::state::check_len(l.iter().map(|v| if let Value::List(i) = v { i.borrow().len() } else { 1 }).sum(), "items")?;
        Ok(l.iter()
            .flat_map(|v| match v {
                Value::List(inner) => inner.borrow().clone(),
                v => vec![v.clone()],
            })
            .collect())
    }

    /// pair up items by position: the first of each list, then the second, and so on, stopping when the shortest list runs out
    ///
    /// zip(*rows) turns rows into columns
    #[builtin(examples = [r#"zip([1, 2, 3], ["a", "b"])"#, "zip(*[[1, 2], [3, 4], [5, 6]])"], see = ["enumerate"])]
    fn zip(lists: Rest<&List>) -> Vec<Value> {
        let ls: Vec<_> = lists.0.iter().map(|l| l.borrow().clone()).collect();
        let n = ls.iter().map(Vec::len).min().unwrap_or(0);
        (0..n).map(|i| Value::list(ls.iter().map(|l| l[i].clone()).collect())).collect()
    }

    /// each item paired with its position, as [index, item], counting from 0
    #[builtin(examples = [r#"["a", "b"].enumerate"#, r#"["a", "b"].enumerate.map(|[i, x]| "{i}:{x}")"#], see = ["zip"])]
    fn enumerate(xs: &List) -> Vec<Value> {
        xs.borrow().iter().enumerate().map(|(i, v)| Value::list(vec![Value::int(i as i64), v.clone()])).collect()
    }

    fn groups(it: &mut Interp, xs: &List, f: &Func, span: &Span) -> Result<IndexMap<String, Vec<Value>>, Fail> {
        let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
        for v in xs.borrow().clone() {
            groups.entry(it.call(f, vec![v.clone()], span)?.to_string()).or_default().push(v);
        }
        Ok(groups)
    }

    /// sort items into groups by what f returns for each
    ///
    /// The result maps each answer to the list of items that gave it
    #[builtin(examples = [r#"["apple", "avocado", "banana"].group_by(|w| w[0])"#, "(1..=6).group_by(|x| x % 2 == 0)"], see = ["count_by"])]
    fn group_by(it: &mut Interp, xs: &List, f: Func, span: &Span) -> Result<IndexMap<String, Value>, Fail> {
        Ok(groups(it, xs, &f, span)?.into_iter().map(|(k, vs)| (k, Value::list(vs))).collect())
    }

    /// count items by what f returns for each
    ///
    /// The result maps each answer to how many items gave it
    #[builtin(examples = [r#""mississippi".chars.count_by(|c| c)"#], see = ["group_by", "count"])]
    fn count_by(it: &mut Interp, xs: &List, f: Func, span: &Span) -> Result<IndexMap<String, Value>, Fail> {
        Ok(groups(it, xs, &f, span)?.into_iter().map(|(k, vs)| (k, Value::int(vs.len() as i64))).collect())
    }

    /// split into lists of n items each, where the last one may be shorter
    #[builtin(examples = ["(1..=7).chunks(3)"], see = ["windows", "flatten"])]
    fn chunks(xs: &List, n: i64) -> Result<Vec<Value>, Fail> {
        if n < 1 {
            return Err(Fail::Arg(1, format!("size must be at least 1, got {n}")));
        }
        Ok(xs.borrow().chunks(n as usize).map(|c| Value::list(c.to_vec())).collect())
    }

    /// every run of n items in a row, sliding along one at a time: [1, 2, 3] with 2 gives [1, 2] and [2, 3]
    #[builtin(examples = ["[1, 2, 3, 4].windows(2)", "[1, 4, 9, 16].windows(2).map(|[a, b]| b - a)"], see = ["chunks"])]
    fn windows(xs: &List, n: i64) -> Result<Vec<Value>, Fail> {
        if n < 1 {
            return Err(Fail::Arg(1, format!("size must be at least 1, got {n}")));
        }
        Ok(xs.borrow().windows(n as usize).map(|c| Value::list(c.to_vec())).collect())
    }

    /// every nth item, starting with the first
    #[builtin(examples = ["(0..=20).step(5)", "(1..10).step(2)"], see = ["range"])]
    fn step(xs: &List, n: i64) -> Result<Vec<Value>, Fail> {
        if n < 1 {
            return Err(Fail::Arg(1, format!("step must be at least 1, got {n}")));
        }
        Ok(xs.borrow().iter().step_by(n as usize).cloned().collect())
    }
}

/// `a..b` ranges and list concatenation.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::*;
    Some(match (op, a, b) {
        (BinOp::Range, Int(x, _), Int(y, _)) => range(*x, *y).map(Value::list),
        (BinOp::RangeIncl, Int(x, _), Int(y, _)) => y.checked_add(1).ok_or_else(|| "range too large".into()).and_then(|y| range(*x, y)).map(Value::list),
        (BinOp::Add, List(x), List(y)) => crate::state::check_len(x.borrow().len() + y.borrow().len(), "items")
            .map(|_| Value::list(x.borrow().iter().chain(y.borrow().iter()).cloned().collect())),
        _ => return None,
    })
}

fn range(a: i64, b: i64) -> Result<Vec<Value>, String> {
    crate::state::check_len(b.saturating_sub(a).try_into().unwrap_or(0), "items")?;
    Ok((a..b).map(Value::int).collect())
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
