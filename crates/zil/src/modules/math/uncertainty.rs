//! Values with an uncertainty, `5 ± 0.1 m`, carried through arithmetic, math functions and `to`.
//! Propagation is first order with independent errors: each input's partial derivative (a central
//! difference) times its error, added in quadrature.

use crate::ast::{BinOp, Target};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::units::Unit;
use crate::modules::{self, Call, Claim, Doc, Fail, Module, doc};
use crate::ops::{binary as op, mismatch};
use crate::value::{Value, compare, num};
use std::cmp::Ordering;

pub const MODULE: Module = Module {
    name: "uncertainty",
    about: "measurements with a margin of error, like `5 ± 0.1 m`, carried through calculations",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Measurements with a margin of error. A length measured as 5 m might really be anywhere from 4.9 m to 5.1 m, written `5 \
± 0.1 m`. Use these values in calculations as usual, and zil works out how far off the result could be."),
        ("good to know", "\
Errors are taken to be independent and random, the standard way in science, so they partly cancel: adding 5 ± 0.3 and \
2 ± 0.4 gives 7 ± 0.5, not 7 ± 0.7."),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("x ± e, also typed x +- e", "5 ± 0.1 m"), ("x ± n%: an error of n% of x", "20 ± 5%")]),
        ("pretty", &[
            ("long, the default: in the best-fitting unit", "pretty(1500 ± 20 m)"),
            ("short: K, M, B and T for thousands up to trillions", r#"pretty(1234567 ± 1000, "short")"#),
        ]),
        ("propagation", &[
            ("adding: errors combine as sqrt(0.3² + 0.4²)", "(5 ± 0.3) + (2 ± 0.4)"),
            ("multiplying: percentage errors combine", "(10 ± 1 m) * (2 ± 0.1 m)"),
            ("functions carry the error through", "sqrt(16 ± 1)"),
            ("and so do unit conversions", "5 ± 0.1 km to mi"),
        ]),
    ],
    #[rustfmt::skip]
    examples: &[
        ("uncertainty", &[
            ("area of a measured rectangle", "(2 ± 0.05 m) * (3 ± 0.05 m)"),
            ("a 1 m pendulum's swing time, in seconds", "2 * pi * sqrt((1 ± 0.01) / 9.81)"),
        ]),
    ],
    fns: FNS,
    call,
    binary: Some(binary),
    compare: Some(compare_hook),
    convert: Some(convert),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("value", "value(x: uncertain) -> num|quantity", "the measured value without its error: 5 for 5 ± 0.1", &["(5 ± 0.1 m).value"], &["error"]),
    doc("error", "error(x: uncertain) -> num|quantity", "the error alone, in the same unit: 0.1 for 5 ± 0.1", &["(5 ± 0.1 m).error", "(20 ± 5%).error"], &["value"]),
];

/// Builtins that pass an uncertainty through, via `propagate`.
pub const LIFTED: &[&str] =
    &["sqrt", "cbrt", "exp", "ln", "log", "abs", "hypot", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "simplify"];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    match (name, args) {
        ("value", [Value::Unc(c)]) => Ok(Value::qty(c.0, c.2.clone())),
        ("error", [Value::Unc(c)]) => Ok(Value::qty(c.1, c.2.clone())),
        _ => Err(Fail::BadArgs),
    }
}

/// Value, error and unit of anything numeric; plain numbers and quantities have no error.
fn parts(v: &Value) -> Option<(f64, f64, Unit)> {
    match v {
        Value::Unc(c) => Some((**c).clone()),
        Value::Qty(x, u) => Some((*x, 0.0, u.clone())),
        _ => Some((num(v)?, 0.0, Unit::default())),
    }
}

/// `f(args)` with the uncertainty carried through. Inputs without one are passed as they are,
/// so `x ** 2` still sees an int exponent.
pub fn propagate<E: From<String>>(args: &[Value], mut f: impl FnMut(&[Value]) -> Result<Value, E>) -> Result<Value, E> {
    let shifted = |i: usize, h: f64| -> Vec<Value> {
        args.iter()
            .enumerate()
            .map(|(j, a)| match a {
                Value::Unc(c) => Value::qty(c.0 + if i == j { h } else { 0.0 }, c.2.clone()),
                a => a.clone(),
            })
            .collect()
    };
    let (y, _, w) = parts(&f(&shifted(usize::MAX, 0.0))?).ok_or_else(|| "the result can't hold an uncertainty".to_string())?;
    // The result's value in `w`, the unit of the unshifted result.
    let at = |v: Value| match v {
        Value::Qty(y, u) if u.dim() == w.dim() => Some(w.value_from_si(u.to_si(y))),
        v => num(&v),
    };
    let mut var = 0.0;
    for (i, a) in args.iter().enumerate() {
        let Value::Unc(c) = a else { continue };
        let (x, e) = (c.0, c.1);
        if e == 0.0 {
            continue;
        }
        let h = (e * 1e-3).max(x.abs() * 1e-9);
        let (hi, lo) = (at(f(&shifted(i, h))?), at(f(&shifted(i, -h))?));
        let (hi, lo) = hi.zip(lo).ok_or_else(|| "the result can't hold an uncertainty".to_string())?;
        var += ((hi - lo) / (2.0 * h) * e).powi(2);
    }
    Ok(Value::unc(y, var.sqrt(), w))
}

/// `x ± e`, where a unit on either side applies to both: `5 ± 0.1 m`, `5 m ± 10 cm`.
fn make(a: &Value, b: &Value) -> Option<Result<Value, String>> {
    let (x, e) = match (a, b) {
        (Value::Unc(..), _) | (_, Value::Unc(..)) => return Some(Err("cannot attach an uncertainty to a value that already has one".into())),
        (Value::Qty(_, u), Value::Qty(_, w)) if u.dim() != w.dim() => return Some(Err(mismatch(BinOp::PlusMinus, a, b))),
        _ => (parts(a)?, parts(b)?),
    };
    let ((x, _, u), (e, _, w)) = (x, e);
    Some(Ok(match (u.0.is_empty(), w.0.is_empty()) {
        (true, false) => Value::unc(x, e.abs(), w),
        // Only the scale matters for an error: `20 C ± 1 C` is 1 kelvin wide, not 274.
        (false, false) => Value::unc(x, (e * w.scale() / u.scale()).abs(), u),
        _ => Value::unc(x, e.abs(), u),
    }))
}

fn binary(o: BinOp, a: &Value, b: &Value) -> Claim {
    if o == BinOp::PlusMinus {
        return make(a, b);
    }
    let ours = matches!(a, Value::Unc(..)) || matches!(b, Value::Unc(..));
    if !ours || !matches!(o, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow) || parts(a).is_none() || parts(b).is_none() {
        return None;
    }
    Some(propagate(&[a.clone(), b.clone()], |v| op(o, &v[0], &v[1])))
}

/// Ordered by central value.
fn compare_hook(a: &Value, b: &Value) -> Option<Ordering> {
    let central = |v: &Value| match v {
        Value::Unc(c) => Value::qty(c.0, c.2.clone()),
        v => v.clone(),
    };
    if !matches!(a, Value::Unc(..)) && !matches!(b, Value::Unc(..)) {
        return None;
    }
    compare(&central(a), &central(b))
}

fn convert(v: &Value, t: &Target) -> Claim {
    match (v, t) {
        (Value::Unc(..), Target::Unit(_)) => {
            Some(propagate(std::slice::from_ref(v), |v| modules::convert(&v[0], t).unwrap_or_else(|| Err("cannot convert".into()))))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn uncertainty() {
        assert_eq!(show("[5 ± 0.1 m, 5 +- 0.1, 5 m ± 10 cm, 20 ± 5%, -(2 ± 1)]"), "[5 ± 0.1 m, 5 ± 0.1, 5 ± 0.1 m, 20 ± 1, -2 ± 1]");
        assert_eq!(show("(5 ± 0.3) + (2 ± 0.4)"), "7 ± 0.5");
        assert_eq!(show("(5 ± 0.3) - (2 ± 0.4)"), "3 ± 0.5");
        assert_eq!(show("(10 ± 1 m) * (2 ± 0.1 m)"), "20 ± 2.23607 m^2");
        assert_eq!(show("(3 ± 0.1) ** 2"), "9 ± 0.6");
        assert_eq!(show("(2 ± 0.1 m) ** 2"), "4 ± 0.4 m^2");
        assert_eq!(show("sqrt(16 ± 1)"), "4 ± 0.125");
        assert_eq!(show("sin(30 ± 1 deg)"), "0.5 ± 0.015115");
        assert_eq!(show("5 ± 0.1 km to m"), "5000 ± 100 m");
        assert_eq!(show("20 ± 1 C to F"), "68 ± 1.8 F");
        assert_eq!(show("[(5 ± 0.1 m).value, (5 ± 0.1 m).error, [1 ± 1, 2].sum]"), "[5 m, 0.1 m, 3 ± 1]");
        assert_eq!(show("[1 ± 1 < 2, (1 ± 0.5) == (1 ± 0.5), type(1 ± 1)]"), r#"[true, true, "uncertain"]"#);
        assert!(try_eval("5 m ± 1 kg").is_err());
        assert!(try_eval("(1 ± 1) ± 1").is_err());
    }
}
