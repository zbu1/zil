//! Numbers: rounding, roots, logs; trig, stats, number theory, bits, bases, formatting, randomness, linear algebra and calculus below.

pub mod bases;
pub mod bits;
pub mod calculus;
pub mod complex;
pub mod formatting;
pub mod linalg;
pub mod numtheory;
pub mod random;
pub mod stats;
pub mod trig;
pub mod uncertainty;

use crate::ast::BinOp;
use crate::ast::Radix;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::ops::binary as op;
use crate::value::{Value, compare, exact, num, ratio};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "math",
    about: "rounding, roots, logs, interpolation, constants; exact fractions and big ints",
    #[rustfmt::skip]
    examples: &[
        ("math", &[
            ("exact fractions", r#"(1..=6).map(|x| 1/x).sum to frac"#),
            ("decimals are exact", "0.1 + 0.2 == 0.3"),
            ("1/3 * 3 is exactly 1", "1/3 * 3 == 1"),
            ("decimal to fraction", "0.75 to frac"),
            ("big ints never overflow", "2 ** 100"),
            ("add a percent", "80 + 15%"),
            ("percent of", "15% of 64.50"),
            ("halfway between dates", r#"lerp(date("2026-01-01"), date("2026-12-31"), 1/2)"#),
            ("°C to a 0-255 byte", "remap(25, 0, 40, 0, 255).round"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("constants", &[("pi e tau phi inf nan", "")]),
        ("operators", &[("n%", "20%"), ("x + n%, x - n%", "50 + 10%"), ("n% of x", "20% of 50")]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("rounding", &["abs", "round", "sig", "floor", "ceil", "trunc", "sign", "clamp"]),
        ("powers", &["sqrt", "cbrt", "exp", "ln", "log", "hypot", "is_nan"]),
        ("in between", &["lerp", "remap"]),
    ],
    call,
    #[rustfmt::skip]
    consts: &[
        ("pi", std::f64::consts::PI), ("e", std::f64::consts::E), ("tau", std::f64::consts::TAU),
        ("phi", std::f64::consts::GOLDEN_RATIO), ("inf", f64::INFINITY), ("nan", f64::NAN),
    ],
    children: &[
        stats::MODULE,
        trig::MODULE,
        numtheory::MODULE,
        bits::MODULE,
        bases::MODULE,
        formatting::MODULE,
        random::MODULE,
        uncertainty::MODULE,
        complex::MODULE,
        linalg::MODULE,
        calculus::MODULE,
    ],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("sqrt", "sqrt(x: num|complex)", "square root", &["sqrt(2)"], &["cbrt"]),
    doc("abs", "abs(x: num|quantity|complex)", "absolute value; keeps units. A complex number's magnitude", &["abs(-3)", "abs(-2 km)"], &["round", "sign"]),
    doc("round", "round(x: num|quantity, digits?: int) / round(x: num|quantity, step: num|quantity)", "round to nearest; keeps units. A non-int second arg rounds to a multiple of it", &["round(pi, 2)", "round(2.5 km)", "round(7.3, 0.25)", "round(17 min, 15 min)"], &["floor", "ceil", "trunc"]),
    doc("sig", "sig(x: num|quantity, digits: int)", "round to that many significant figures; keeps units", &["3.14159.sig(3)", "123456.sig(2)", "0.0004567.sig(2)", "(1 mi to m).sig(3)"], &["round", "sci"]),
    doc("floor", "floor(x: num|quantity)", "round down", &["floor(2.7)"], &["ceil", "round"]),
    doc("ceil", "ceil(x: num|quantity)", "round up", &["ceil(2.1)"], &["floor", "round"]),
    doc("trunc", "trunc(x: num|quantity)", "round toward zero; keeps units", &["trunc(-2.7)"], &["floor", "round"]),
    doc("sign", "sign(x: num|quantity)", "-1, 0 or 1", &["sign(-5 km)", "sign(0)"], &["abs"]),
    doc("clamp", "clamp(x: any, lo: any, hi: any)", "limit x to [lo, hi]", &["clamp(15, 0, 10)", "clamp(5 m, 1 m, 2 m)"], &["min", "max"]),
    doc("cbrt", "cbrt(x: num)", "cube root", &["cbrt(27)"], &["sqrt"]),
    doc("exp", "exp(x: num|complex)", "e to the power x", &["exp(1)"], &["ln"]),
    doc("ln", "ln(x: num|complex)", "natural log", &["ln(e)"], &["log", "exp"]),
    doc("log", "log(x: num, base?: num)", "log base 10, or another base", &["log(1000)", "log(8, 2)"], &["ln"]),
    doc("hypot", "hypot(x: num|quantity, y: num|quantity)", "sqrt(x² + y²) without overflow; keeps units", &["hypot(3, 4)", "hypot(3 m, 4 m)"], &["sqrt", "atan2"]),
    doc("is_nan", "is_nan(x: any)", "whether x is nan (nan != nan)", &["is_nan(nan)", "is_nan(inf - inf)"], &[]),
    doc("lerp", "lerp(a: any, b: any, t: num)", "a + (b - a) * t: the point a fraction t of the way from a to b; numbers, units or dates", &["lerp(10, 20, 0.25)", "lerp(0 C, 100 C, 0.37)"], &["remap"]),
    doc("remap", "remap(x: any, lo: any, hi: any, to_lo: any, to_hi: any)", "x moved from the range lo..hi to the range to_lo..to_hi, proportionally", &["remap(5, 0, 10, 100, 200)", "remap(72 F, 32 F, 212 F, 0, 100)"], &["lerp", "normalize"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("sqrt", [v]) if num(v).is_some_and(|x| x < 0.0) => {
            return Err(Fail::Err(crate::Error::new(format!("sqrt: `{v}` is negative"), span.clone()).help(format!("for a complex root, `csqrt({v})`"))));
        }
        ("sqrt", [v]) if num(v).is_some() => Float(num(v).unwrap().sqrt()),
        ("abs" | "round" | "floor" | "ceil" | "trunc", [v]) => round_like(name, v, 0).ok_or(Fail::BadArgs)?,
        ("round", [v, Int(n, _)]) => round_like(name, v, *n).ok_or(Fail::BadArgs)?,
        ("round", [v, step]) => round_to(v, step).ok_or(Fail::BadArgs)?,
        ("sig", [v, Int(n, _)]) if (1..=17).contains(n) => {
            let sig = |x: f64| {
                if x == 0.0 || !x.is_finite() {
                    return x;
                }
                let p = 10f64.powi(*n as i32 - 1 - x.abs().log10().floor() as i32);
                (x * p).round() / p
            };
            match v {
                Qty(x, u) => Qty(sig(*x), u.clone()),
                v => Float(sig(num(v).ok_or(Fail::BadArgs)?)),
            }
        }
        ("sig", [_, Int(n, _)]) => return Err(Fail::Arg(1, format!("digits must be from 1 to 17, got {n}"))),
        ("sign", [v]) => {
            let x = match v {
                Qty(x, _) => *x,
                _ => num(v).ok_or(Fail::BadArgs)?,
            };
            if x.is_nan() {
                Float(x)
            } else {
                Value::int(if x > 0.0 {
                    1
                } else if x < 0.0 {
                    -1
                } else {
                    0
                })
            }
        }
        ("clamp", [v, lo, hi]) => {
            let cmp = |a: &Value, b: &Value| compare(a, b).ok_or_else(|| format!("cannot compare {} and {}", crate::error::short(a), crate::error::short(b)));
            if cmp(lo, hi)?.is_gt() {
                return Err(Fail::Arg(1, format!("lo {lo:?} is greater than hi {hi:?}")));
            }
            if cmp(v, lo)?.is_lt() {
                lo.clone()
            } else if cmp(v, hi)?.is_gt() {
                hi.clone()
            } else {
                v.clone()
            }
        }
        ("cbrt" | "exp", [v]) if num(v).is_some() => {
            let x = num(v).unwrap();
            Float(if name == "cbrt" { x.cbrt() } else { x.exp() })
        }
        ("is_nan", [v]) => Bool(matches!(v, Float(x) | Qty(x, _) if x.is_nan())),
        ("ln", [v]) if num(v).is_some() => Float(num(v).unwrap().ln()),
        ("log", [v]) if num(v).is_some() => Float(num(v).unwrap().log10()),
        ("log", [v, b]) if num(v).is_some() && num(b).is_some() => Float(num(v).unwrap().log(num(b).unwrap())),
        ("hypot", [Qty(x, u), Qty(y, w)]) if u.dim() == w.dim() => Qty(x.hypot(u.value_from_si(w.to_si(*y))), u.clone()),
        ("hypot", [x, y]) if num(x).is_some() && num(y).is_some() => Float(num(x).unwrap().hypot(num(y).unwrap())),
        ("lerp", [a, b, t]) => op(BinOp::Add, a, &op(BinOp::Mul, &op(BinOp::Sub, b, a)?, t)?)?,
        ("remap", [x, lo, hi, to_lo, to_hi]) => {
            let t = op(BinOp::Div, &op(BinOp::Sub, x, lo)?, &op(BinOp::Sub, hi, lo)?)?;
            op(BinOp::Add, to_lo, &op(BinOp::Mul, &op(BinOp::Sub, to_hi, to_lo)?, &t)?)?
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// abs/round/floor/ceil/trunc on numbers and quantities (keeping the unit).
fn round_like(name: &str, v: &Value, digits: i64) -> Option<Value> {
    let f = |x: f64| -> f64 {
        let p = 10f64.powi(digits as i32);
        match name {
            "abs" => x.abs(),
            "round" => (x * p).round() / p,
            "floor" => x.floor(),
            "trunc" => x.trunc(),
            _ => x.ceil(),
        }
    };
    Some(match v {
        Value::Int(n, b) if name == "abs" => Value::Int(n.checked_abs()?, *b),
        Value::Int(..) if digits >= 0 => v.clone(),
        Value::Big(n, b) if name == "abs" => exact(BigRational::from_integer(n.abs()), *b, false),
        Value::Big(..) if digits >= 0 => v.clone(),
        Value::Frac(r, f) if name == "abs" => Value::Frac(Rc::new(r.abs()), *f),
        Value::Frac(r, _) if digits == 0 => exact(
            match name {
                "round" => r.round(),
                "floor" => r.floor(),
                "trunc" => r.trunc(),
                _ => r.ceil(),
            },
            Radix::DEC,
            false,
        ),
        Value::Frac(..) => Value::Float(f(num(v)?)),
        Value::Float(x) if digits == 0 && name != "abs" && f(*x).abs() < 9.2e18 => Value::int(f(*x) as i64),
        Value::Float(x) => Value::Float(f(*x)),
        Value::Qty(x, u) => Value::Qty(f(*x), u.clone()),
        _ => return None,
    })
}

/// `round(x, step)`: the nearest multiple of `step`, exact for ints and fractions.
fn round_to(v: &Value, step: &Value) -> Option<Value> {
    if let (Some(x), Some(s)) = (ratio(v), ratio(step)) {
        return (!s.is_zero()).then(|| exact((x / &s).round() * s, Radix::DEC, false));
    }
    Some(match (v, step) {
        (Value::Qty(x, u), Value::Qty(s, w)) if u.dim() == w.dim() => {
            let s = u.value_from_si(w.to_si(*s));
            Value::Qty((x / s).round() * s, u.clone())
        }
        (Value::Qty(x, u), s) => Value::Qty((x / num(s)?).round() * num(s)?, u.clone()),
        _ => Value::Float((num(v)? / num(step)?).round() * num(step)?),
    })
}

pub fn int(v: &Value) -> Result<BigInt, Fail> {
    match v {
        Value::Int(n, _) => Ok((*n).into()),
        Value::Big(n, _) => Ok((**n).clone()),
        _ => Err(Fail::BadArgs),
    }
}

pub fn big(n: BigInt) -> Value {
    exact(BigRational::from_integer(n), Radix::DEC, false)
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn math() {
        assert_eq!(show("sqrt(16)"), "4");
        assert_eq!(show("round(pi, 2)"), "3.14");
        assert_eq!(show("round(2.5 km)"), "3 km");
        assert_eq!(show("log(1000)"), "3");
        assert_eq!(
            show("[3.14159.sig(3), 123456.sig(2), 0.0004567.sig(2), -987.sig(1), 0.sig(3), (1 mi to m).sig(3)]"),
            "[3.14, 120000, 0.00046, -1000, 0, 1610 m]"
        );
    }

    #[test]
    fn more_math() {
        assert_eq!(show("atan2(1, -1) to deg"), "135 deg");
        assert_eq!(show("[hypot(3, 4), hypot(3 m, 400 cm), cbrt(27), exp(0)]"), "[5, 5 m, 3, 1]");
        assert_eq!(show("[clamp(15, 0, 10), clamp(-1, 0, 10), clamp(5 m, 1 m, 2 m)]"), "[10, 0, 2 m]");
        assert_eq!(show("[sign(-5 km), sign(0), sign(2/3), trunc(-2.7), trunc(7/2), trunc(-2.7 m)]"), "[-1, 0, 1, -2, 3, -2 m]");
        assert_eq!(show("[round(7.3, 0.25), round(7, 1/2), round(17 min, 15 min), round(80 min, 1 h)]"), "[7.25, 7, 15 min, 60 min]");
        assert_eq!(show("[is_nan(nan), is_nan(1), nan == nan, inf > 10 ** 300, tau / pi]"), "[true, false, false, true, 2]");
        assert_eq!(show("[lerp(10, 20, 1/4), lerp(0 m, 1 km, 0.5), remap(5, 0, 10, 100, 200)]"), "[12.5, 500 m, 150]");
        assert_eq!(show(r#"lerp(date("2026-01-01"), date("2026-01-03"), 1/2)"#), "2026-01-02");
        assert!(try_eval("clamp(1, 5, 0)").is_err());
    }
}
