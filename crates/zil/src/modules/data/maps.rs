//! Maps: keys, values, lookups, merging and transforming.

use crate::ast::BinOp;
use crate::error::short;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::Value;
use indexmap::IndexMap;

pub const MODULE: Module = Module {
    name: "maps",
    about: "keys, values, lookups, merging and transforming maps",
    #[rustfmt::skip]
    examples: &[
        ("maps", &[
            ("sum a map", "{a: 1, b: 2}.values.sum"),
            ("as [key, value] pairs", "{a: 1, b: 2}.list"),
            ("defaults, overridden", "{color: \"red\", size: 1} + {size: 3}"),
            ("loop over entries", "out = []; for [k, v] in {a: 1, b: 2}.entries { out.push(\"{k}={v}\") }; out"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("map + map", "{a: 1} + {a: 2, b: 3}")]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("look up", &["keys", "values", "get", "has"]),
        ("change", &["del", "merge"]),
        ("convert", &["entries", "from_entries", "invert"]),
        ("transform", &["map_values", "filter_keys"]),
    ],
    call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("keys", "keys(m: map)", "list of map keys", &["{a: 1, b: 2}.keys"], &["values"]),
    doc("values", "values(m: map)", "list of map values", &["{a: 1, b: 2}.values"], &["keys"]),
    doc("get", "get(m: map, k: any, default?: any)", "value at k, or default (nil if not given)", &["{a: 1}.get(\"a\")", "{a: 1}.get(\"b\", 0)"], &["has"]),
    doc("has", "has(m: map, k: any)", "true if m has the key k", &["{a: 1}.has(\"a\")"], &["get", "contains"]),
    doc("del", "del(m: map, k: any)", "remove k in place and return the map", &["{a: 1, b: 2}.del(\"a\")"], &["merge"]).mutates(),
    doc("merge", "merge(a?: map, ...)", "new map with all of them; later maps win on shared keys, same as a + b", &["merge({a: 1, b: 2}, {b: 3})", "merge({a: 1}, {b: 2}, {a: 3})"], &["del"]),
    doc("entries", "entries(m: map)", "[key, value] pairs, same as list(m)", &["{a: 1, b: 2}.entries"], &["from_entries", "list"]),
    doc("from_entries", "from_entries(xs: list)", "map from [key, value] pairs; later keys win", &["[[\"a\", 1], [\"b\", 2]].from_entries", "zip([\"x\", \"y\"], [1, 2]).from_entries"], &["entries"]),
    doc("invert", "invert(m: map) / invert(c: str|list)", "swap a map's keys and values (values become string keys), or the opposite color", &["{a: 1, b: 2}.invert", r#"invert("navy")"#], &["entries", "grayscale"]),
    doc("map_values", "map_values(m: map, f: fn)", "apply f to every value, keeping keys", &["{a: 1, b: 2}.map_values(|v| v * 10)"], &["map", "filter"]),
    doc("filter_keys", "filter_keys(m: map, f: fn)", "keep entries whose key f is truthy for", &["{a: 1, bb: 2}.filter_keys(|k| k.len > 1)"], &["filter"]),
];

/// Map keys are strings; other values are keyed by how they display (as `group_by` does).
fn key(v: &Value) -> String {
    v.to_string()
}

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("keys", [Map(m)]) => Value::list(m.borrow().keys().map(|k| Value::str(k.as_str())).collect()),
        ("values", [Map(m)]) => Value::list(m.borrow().values().cloned().collect()),
        ("get", [Map(m), k]) => m.borrow().get(&key(k)).cloned().unwrap_or(Nil),
        ("get", [Map(m), k, d]) => m.borrow().get(&key(k)).cloned().unwrap_or_else(|| d.clone()),
        ("has", [Map(m), k]) => Bool(m.borrow().contains_key(&key(k))),
        ("del", [Map(m), k]) => {
            m.borrow_mut().shift_remove(&key(k));
            Map(m.clone())
        }
        ("merge", ms) if ms.iter().all(|m| matches!(m, Map(_))) => merge(ms.iter().map(|m| if let Map(m) = m { &**m } else { unreachable!() }).collect())?,
        ("entries", [Map(m)]) => Value::list(m.borrow().iter().map(|(k, v)| Value::list(vec![Value::str(k.as_str()), v.clone()])).collect()),
        ("from_entries", [List(l)]) => {
            let mut out = IndexMap::new();
            for v in l.borrow().iter() {
                match v {
                    List(p) if p.borrow().len() == 2 => {
                        let p = p.borrow();
                        out.insert(key(&p[0]), p[1].clone());
                    }
                    v => return Err(Fail::Arg(0, format!("expected [key, value] pairs, got {}", short(v)))),
                }
            }
            Value::map(out)
        }
        ("invert", [Map(m)]) => Value::map(m.borrow().iter().map(|(k, v)| (key(v), Value::str(k.as_str()))).collect()),
        ("invert", _) => return crate::modules::dev::colors::call(it, name, args, span),
        ("map_values", [Map(m), f]) => {
            let mut out = IndexMap::new();
            for (k, v) in m.borrow().clone() {
                out.insert(k, it.call(f, vec![v], span)?);
            }
            Value::map(out)
        }
        ("filter_keys", [Map(m), f]) => {
            let mut out = IndexMap::new();
            for (k, v) in m.borrow().clone() {
                if it.call(f, vec![Value::str(k.as_str())], span)?.truthy() {
                    out.insert(k, v);
                }
            }
            Value::map(out)
        }
        _ => return Err(Fail::BadArgs),
    })
}

fn merge(ms: Vec<&std::cell::RefCell<IndexMap<String, Value>>>) -> Result<Value, String> {
    crate::state::check_len(ms.iter().map(|m| m.borrow().len()).sum(), "entries")?;
    let mut out = IndexMap::new();
    for m in ms {
        out.extend(m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    Ok(Value::map(out))
}

/// `a + b` merges maps, right side winning.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    match (op, a, b) {
        (BinOp::Add, Value::Map(x), Value::Map(y)) => Some(merge(vec![x, y])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn maps() {
        assert_eq!(show(r#"[{a: 1}.get("a"), {a: 1}.get("b"), {a: 1}.get("b", 0), {a: 1}.has("a"), {a: 1}.has("b")]"#), "[1, nil, 0, true, false]");
        assert_eq!(show(r#"m = {a: 1, b: 2}; m.del("a"); m"#), "{b: 2}");
        assert_eq!(show("[{a: 1, b: 2} + {b: 3, c: 4}, merge({a: 1}, {a: 2})]"), "[{a: 1, b: 3, c: 4}, {a: 2}]");
        assert_eq!(show("m = {a: 1, b: [2]}; [m.entries.from_entries == m, m.list.from_entries == m]"), "[true, true]");
        assert_eq!(show("[{a: 1, b: 2}.invert, (1..=4).group_by(|x| x % 2).get(1)]"), "[{1: \"a\", 2: \"b\"}, [1, 3]]");
        assert_eq!(
            show("[{a: 1, b: 2}.map_values(|v| v * 10), {a: 1, bb: 2}.filter_keys(|k| k.len > 1), {a: 1, b: 2}.filter(|k, v| v > 1)]"),
            "[{a: 10, b: 20}, {bb: 2}, {b: 2}]"
        );
    }
}
