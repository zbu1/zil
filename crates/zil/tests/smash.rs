//! Every builtin called with odd arguments must return a value or an error, never panic.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use zil::Session;

const ARGS: &[&str] = &[
    "0",
    "-1",
    "9223372036854775807",
    "-9223372036854775808",
    "10 ** 30",
    "0.5",
    "-1e308",
    "1/3",
    "\"\"",
    "\"é%{\"",
    "[]",
    "[[1, 2], [3]]",
    "{}",
    "nil",
    "5 m",
    "today",
    "|x| x",
];

/// Calls every builtin with every combination of up to `arity` of `ARGS`.
fn smash(arity: usize) {
    let fresh = || {
        let mut s = Session::new(Rc::new(zil::host::NullHost));
        let l = s.limits();
        l.steps = 100_000;
        l.len = 10_000;
        l.bits = 10_000;
        s
    };
    let (mut layer, mut lists) = (vec![String::new()], vec![String::new()]);
    for _ in 0..arity {
        layer = layer.iter().flat_map(|l| ARGS.iter().map(move |a| if l.is_empty() { a.to_string() } else { format!("{l}, {a}") })).collect();
        lists.extend(layer.iter().cloned());
    }
    std::panic::set_hook(Box::new(|_| {}));
    let (mut it, mut panics) = (fresh(), vec![]);
    let names: Vec<String> = it.names().into_iter().filter(|n| matches!(it.get(n), Some(zil::Value::Named(..)))).collect();
    for name in names {
        for args in &lists {
            let src = format!("{name}({args})");
            if catch_unwind(AssertUnwindSafe(|| it.eval(&src))).is_err() {
                panics.push(src);
                it = fresh();
            }
        }
    }
    drop(std::panic::take_hook());
    assert!(panics.is_empty(), "{} panics:\n{}", panics.len(), panics.join("\n"));
}

#[test]
fn builtins_never_panic() {
    smash(2);
}

/// About a minute in debug.
#[test]
#[ignore]
fn builtins_never_panic_3() {
    smash(3);
}
