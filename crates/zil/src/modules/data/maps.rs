//! Maps: keys, values, lookups, merging and transforming.

use crate::ast::BinOp;
use crate::modules::{Claim, Module};
use crate::value::Value;
use indexmap::IndexMap;

pub const MODULE: Module = Module {
    name: "maps",
    about: "look up, change, merge and transform maps of keys to values",
    #[rustfmt::skip]
    examples: &[
        ("maps", &[
            ("sum a map", "{a: 1, b: 2}.values.sum"),
            ("as [key, value] pairs", "{a: 1, b: 2}.list"),
            ("defaults with some overridden", "{color: \"red\", size: 1} + {size: 3}"),
            ("loop over entries", "out = []; for [k, v] in {a: 1, b: 2}.entries { out.push(\"{k}={v}\") }; out"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("map + map merges them, and the right side wins on shared keys", "{a: 1} + {a: 2, b: 3}")]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("look up", &["keys", "values", "get", "has"]),
        ("change", &["del", "merge"]),
        ("convert", &["entries", "from_entries", "invert"]),
        ("transform", &["map_values", "filter_keys"]),
    ],
    call: fns::call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{key, merge};
    use crate::error::short;
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::modules::dev::colors::{self, Color};
    use crate::signature::{As, Func, List, Map, Rest};
    use crate::value::Value;
    use indexmap::IndexMap;

    /// the map's keys, as a list
    #[builtin(examples = ["{a: 1, b: 2}.keys"], see = ["values"])]
    fn keys(m: &Map) -> Vec<Value> {
        m.borrow().keys().map(|k| Value::str(k.as_str())).collect()
    }

    /// the map's values, as a list
    #[builtin(examples = ["{a: 1, b: 2}.values"], see = ["keys"])]
    fn values(m: &Map) -> Vec<Value> {
        m.borrow().values().cloned().collect()
    }

    /// the value under key k, or default if there's none
    #[builtin(examples = ["{a: 1}.get(\"a\")", "{a: 1}.get(\"b\", 0)"], see = ["has"])]
    fn get(m: &Map, k: &Value, default: Option<&Value>) -> Value {
        m.borrow().get(&key(k)).cloned().unwrap_or_else(|| default.cloned().unwrap_or(Value::Nil))
    }

    /// whether the map has the key k
    #[builtin(examples = ["{a: 1}.has(\"a\")"], see = ["get", "contains"])]
    fn has(m: &Map, k: &Value) -> bool {
        m.borrow().contains_key(&key(k))
    }

    /// remove key k and its value from the map, changing it, and return the map
    #[builtin(examples = ["{a: 1, b: 2}.del(\"a\")"], see = ["merge"], mutates)]
    fn del(m: &Map, k: &Value) -> Map {
        m.borrow_mut().shift_remove(&key(k));
        m.clone()
    }

    /// a new map with every entry of every map
    ///
    /// When maps share a key, the later one's value wins. Same as a + b
    #[builtin(name = "merge", examples = ["merge({a: 1, b: 2}, {b: 3})", "merge({a: 1}, {b: 2}, {a: 3})"], see = ["del"])]
    fn merge_all(maps: Rest<&Map>) -> Result<As<Map>, String> {
        merge(maps.0.iter().map(|m| &***m).collect()).map(As::from)
    }

    /// the map as a list of [key, value] pairs, the same as list(m)
    #[builtin(examples = ["{a: 1, b: 2}.entries"], see = ["from_entries", "list"])]
    fn entries(m: &Map) -> Vec<Value> {
        m.borrow().iter().map(|(k, v)| Value::list(vec![Value::str(k.as_str()), v.clone()])).collect()
    }

    /// a map from a list of [key, value] pairs, the reverse of entries
    ///
    /// If a key repeats, the last one wins
    #[builtin(examples = ["[[\"a\", 1], [\"b\", 2]].from_entries", "zip([\"x\", \"y\"], [1, 2]).from_entries"], see = ["entries"])]
    fn from_entries(xs: &List) -> Result<IndexMap<String, Value>, Fail> {
        let mut out = IndexMap::new();
        for v in xs.borrow().iter() {
            match v {
                Value::List(p) if p.borrow().len() == 2 => {
                    let p = p.borrow();
                    out.insert(key(&p[0]), p[1].clone());
                }
                v => return Err(Fail::Arg(0, format!("expected [key, value] pairs, got {}", short(v)))),
            }
        }
        Ok(out)
    }

    /// for a map, swap keys and values, so each value becomes a key (as a string)
    ///
    /// For a color, the opposite color on the color wheel, like white for black
    #[builtin(examples = ["{a: 1, b: 2}.invert"], see = ["entries", "grayscale"])]
    fn invert(m: &Map) -> IndexMap<String, Value> {
        m.borrow().iter().map(|(k, v)| (key(v), Value::str(k.as_str()))).collect()
    }

    #[builtin(name = "invert", examples = [r#"invert("navy")"#])]
    fn invert_color(c: Color) -> Result<String, Fail> {
        colors::invert(&c)
    }

    /// a new map with the same keys and f(value) for every value
    #[builtin(examples = ["{a: 1, b: 2}.map_values(|v| v * 10)"], see = ["map", "filter"])]
    fn map_values(it: &mut Interp, m: &Map, f: Func, span: &Span) -> Result<IndexMap<String, Value>, Fail> {
        let mut out = IndexMap::new();
        for (k, v) in m.borrow().clone() {
            out.insert(k, it.call(&f, vec![v], span)?);
        }
        Ok(out)
    }

    /// keep only the entries whose key passes the test f
    #[builtin(examples = ["{a: 1, bb: 2}.filter_keys(|k| k.len > 1)"], see = ["filter"])]
    fn filter_keys(it: &mut Interp, m: &Map, f: Func, span: &Span) -> Result<IndexMap<String, Value>, Fail> {
        let mut out = IndexMap::new();
        for (k, v) in m.borrow().clone() {
            if it.call(&f, vec![Value::str(k.as_str())], span)?.truthy() {
                out.insert(k, v);
            }
        }
        Ok(out)
    }
}

/// Map keys are strings; other values are keyed by how they display (as `group_by` does).
fn key(v: &Value) -> String {
    v.to_string()
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
