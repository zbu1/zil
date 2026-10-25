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

use crate::ast::Radix;
use crate::modules::{Fail, Module};
use crate::value::{Value, exact, num, ratio};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "math",
    about: "rounding, roots, powers and logs, percentages, and the exact numbers zil uses",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Everyday math: rounding, roots, powers and logs, and percentages."),
        ("writing", "\
The constants `pi`, `e` (2.718...), `tau` (2 pi), `phi` (the golden ratio), `inf` (infinity) and `nan` (not a number, \
the result of something undefined like inf - inf) are always available."),
        ("good to know", "\
A percentage on its own is a fraction, so `20%` is 0.2. Added to or taken from a number, it's a percentage of that \
number, so `50 + 10%` is 55, the way a calculator does it."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("math", &[
            ("exact fractions", r#"(1..=6).map(|x| 1/x).sum to frac"#),
            ("decimals are exact", "0.1 + 0.2 == 0.3"),
            ("1/3 * 3 is exactly 1", "1/3 * 3 == 1"),
            ("a decimal as a fraction", "0.75 to frac"),
            ("big ints never overflow", "2 ** 100"),
            ("add a percent", "80 + 15%"),
            ("a percentage of a number", "15% of 64.50"),
            ("halfway between dates", r#"lerp(date("2026-01-01"), date("2026-12-31"), 1/2)"#),
            ("25 °C on a 0-40 °C scale, as a 0-255 byte", "remap(25, 0, 40, 0, 255).round"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("operators", &[("n% is n hundredths", "20%"), ("x + n% adds n% of x, x - n% takes it off", "50 + 10%"), ("n% of x", "20% of 50")]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("rounding", &["abs", "round", "sig", "floor", "ceil", "trunc", "sign", "clamp"]),
        ("powers", &["sqrt", "cbrt", "exp", "ln", "log", "hypot", "is_nan"]),
        ("in between", &["lerp", "remap"]),
    ],
    call: fns::call,
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

#[zil_macros::builtins]
mod fns {
    use super::{complex, round_like, round_to};
    use crate::Error;
    use crate::ast::BinOp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::ops::binary as op;
    use crate::signature::{As, Complex, Float, Num, Or, Qty};
    use crate::value::{Value, compare, num};

    /// square root
    ///
    /// Use csqrt for the square root of a negative number
    #[builtin(examples = ["sqrt(2)"], see = ["cbrt"])]
    fn sqrt(x: Or<Num, Complex>, span: &Span) -> Result<Or<f64, Complex>, Fail> {
        let v = match x {
            Or::A(v) => v,
            Or::B(z) => return Ok(Or::B(complex::sqrt((z.0, z.1)).into())),
        };
        let x = num(&v).ok_or(Fail::BadArgs)?;
        if x < 0.0 {
            let v = &*v;
            return Err(Fail::Err(Error::new(format!("sqrt: `{v}` is negative"), span.clone()).help(format!("for a complex root, `csqrt({v})`"))));
        }
        Ok(Or::A(x.sqrt()))
    }

    /// the value without its sign, so -3 becomes 3
    ///
    /// Keeps units. For a complex number, its distance from 0
    #[builtin(examples = ["abs(-3)", "abs(-2 km)"], see = ["round", "sign"])]
    fn abs(x: Or<Or<Num, Qty>, Complex>) -> Result<As<Or<Num, Qty>>, Fail> {
        match x {
            Or::A(v) => round_like("abs", v.value(), 0).map(As::from).ok_or(Fail::BadArgs),
            Or::B(z) => Ok(Value::Float(z.0.hypot(z.1)).into()),
        }
    }

    /// round to the nearest whole number, or to digits decimal places
    ///
    /// Halves round away from zero. Keeps units. If the second argument isn't a whole number, like 0.25 or 15 min, rounds to the nearest multiple of it instead
    #[builtin(examples = ["round(pi, 2)", "round(2.5 km)", "round(7.3, 0.25)", "round(17 min, 15 min)"], see = ["floor", "ceil", "trunc"])]
    fn round(x: Or<Num, Qty>, #[default(0)] digits: i64) -> Result<As<Or<Num, Qty>>, Fail> {
        round_like("round", x.value(), digits).map(As::from).ok_or(Fail::BadArgs)
    }

    #[builtin(name = "round")]
    fn round_step(x: Or<Num, Qty>, step: Or<Num, Qty>) -> Result<As<Or<Num, Qty>>, Fail> {
        round_to(x.value(), step.value()).map(As::from).ok_or(Fail::BadArgs)
    }

    /// round to that many significant figures (digits counted from the first non-zero one)
    ///
    /// Keeps units
    #[builtin(examples = ["3.14159.sig(3)", "123456.sig(2)", "0.0004567.sig(2)", "(1 mi to m).sig(3)"], see = ["round", "sci"])]
    fn sig(x: Or<Num, Qty>, digits: i64) -> Result<As<Or<Float, Qty>>, Fail> {
        if !(1..=17).contains(&digits) {
            return Err(Fail::Arg(1, format!("digits must be from 1 to 17, got {digits}")));
        }
        let sig = |x: f64| {
            if x == 0.0 || !x.is_finite() {
                return x;
            }
            let p = 10f64.powi(digits as i32 - 1 - x.abs().log10().floor() as i32);
            (x * p).round() / p
        };
        Ok(match x {
            Or::B(q) => Value::Qty(sig(q.get().0), q.get().1.clone()),
            Or::A(v) => Value::Float(sig(num(&v).ok_or(Fail::BadArgs)?)),
        }
        .into())
    }

    /// round down to a whole number
    #[builtin(examples = ["floor(2.7)"], see = ["ceil", "round"])]
    fn floor(x: Or<Num, Qty>) -> Result<As<Or<Num, Qty>>, Fail> {
        round_like("floor", x.value(), 0).map(As::from).ok_or(Fail::BadArgs)
    }

    /// round up to a whole number
    #[builtin(examples = ["ceil(2.1)"], see = ["floor", "round"])]
    fn ceil(x: Or<Num, Qty>) -> Result<As<Or<Num, Qty>>, Fail> {
        round_like("ceil", x.value(), 0).map(As::from).ok_or(Fail::BadArgs)
    }

    /// drop the fractional part, rounding toward zero
    ///
    /// Keeps units
    #[builtin(examples = ["trunc(-2.7)"], see = ["floor", "round"])]
    fn trunc(x: Or<Num, Qty>) -> Result<As<Or<Num, Qty>>, Fail> {
        round_like("trunc", x.value(), 0).map(As::from).ok_or(Fail::BadArgs)
    }

    /// -1 if x is negative, 0 if it's zero, 1 if it's positive
    ///
    /// nan for nan
    #[builtin(examples = ["sign(-5 km)", "sign(0)"], see = ["abs"])]
    fn sign(x: Or<Num, Qty>) -> Result<As<Or<i64, Float>>, Fail> {
        let x = match x {
            Or::A(v) => num(&v).ok_or(Fail::BadArgs)?,
            Or::B(q) => q.get().0,
        };
        Ok(if x.is_nan() {
            Value::Float(x)
        } else {
            Value::int(if x > 0.0 {
                1
            } else if x < 0.0 {
                -1
            } else {
                0
            })
        }
        .into())
    }

    /// x, but no lower than lo and no higher than hi
    #[builtin(examples = ["clamp(15, 0, 10)", "clamp(5 m, 1 m, 2 m)"], see = ["min", "max"])]
    fn clamp(x: &Value, lo: &Value, hi: &Value) -> Result<Value, Fail> {
        let cmp = |a: &Value, b: &Value| compare(a, b).ok_or_else(|| format!("cannot compare {} and {}", crate::error::short(a), crate::error::short(b)));
        if cmp(lo, hi)?.is_gt() {
            return Err(Fail::Arg(1, format!("lo {lo:?} is greater than hi {hi:?}")));
        }
        Ok(if cmp(x, lo)?.is_lt() {
            lo.clone()
        } else if cmp(x, hi)?.is_gt() {
            hi.clone()
        } else {
            x.clone()
        })
    }

    /// cube root
    #[builtin(examples = ["cbrt(27)"], see = ["sqrt"])]
    fn cbrt(x: f64) -> f64 {
        x.cbrt()
    }

    /// e (2.718...) to the power x
    #[builtin(examples = ["exp(1)"], see = ["ln"])]
    fn exp(x: Or<f64, Complex>) -> Or<f64, Complex> {
        match x {
            Or::A(x) => Or::A(x.exp()),
            Or::B(z) => Or::B(complex::exp((z.0, z.1)).into()),
        }
    }

    /// natural logarithm, the log base e
    ///
    /// The power e must be raised to to give x
    #[builtin(examples = ["ln(e)"], see = ["log", "exp"])]
    fn ln(x: Or<f64, Complex>) -> Or<f64, Complex> {
        match x {
            Or::A(x) => Or::A(x.ln()),
            Or::B(z) => Or::B(complex::ln((z.0, z.1)).into()),
        }
    }

    /// logarithm base 10, or in the base given
    ///
    /// The power the base must be raised to to give x
    #[builtin(examples = ["log(1000)", "log(8, 2)"], see = ["ln"])]
    fn log(x: f64, base: Option<f64>) -> f64 {
        base.map_or(x.log10(), |b| x.log(b))
    }

    /// the length of the long side of a right triangle with sides x and y, sqrt(x² + y²)
    ///
    /// It doesn't overflow for huge values. Keeps units
    #[builtin(examples = ["hypot(3, 4)", "hypot(3 m, 4 m)"], see = ["sqrt", "atan2"])]
    fn hypot(x: Qty, y: Qty) -> Result<As<Qty>, Fail> {
        let ((x, u), (y, w)) = (x.get(), y.get());
        if u.dim() != w.dim() {
            return Err(Fail::BadArgs);
        }
        Ok(Value::Qty(x.hypot(u.value_from_si(w.to_si(y))), u.clone()).into())
    }

    #[builtin(name = "hypot")]
    fn hypot_num(x: f64, y: f64) -> f64 {
        x.hypot(y)
    }

    /// whether x is nan, the "not a number" float
    ///
    /// Use this because nan is never equal to anything, even itself
    #[builtin(examples = ["is_nan(nan)", "is_nan(inf - inf)"])]
    fn is_nan(x: &Value) -> bool {
        matches!(x, Value::Float(x) | Value::Qty(x, _) if x.is_nan())
    }

    /// the point a fraction t of the way from a to b, so 0 gives a, 1 gives b and 0.5 halfway
    ///
    /// Works on numbers, quantities and dates
    #[builtin(examples = ["lerp(10, 20, 0.25)", "lerp(0 C, 100 C, 0.37)"], see = ["remap"])]
    fn lerp(a: &Value, b: &Value, t: Num) -> Result<Value, String> {
        op(BinOp::Add, a, &op(BinOp::Mul, &op(BinOp::Sub, b, a)?, &t)?)
    }

    /// where x would be on a different scale
    ///
    /// x's position between lo and hi, carried over to between to_lo and to_hi
    #[builtin(examples = ["remap(5, 0, 10, 100, 200)", "remap(72 F, 32 F, 212 F, 0, 100)"], see = ["lerp", "normalize"])]
    fn remap(x: &Value, lo: &Value, hi: &Value, to_lo: &Value, to_hi: &Value) -> Result<Value, String> {
        let t = op(BinOp::Div, &op(BinOp::Sub, x, lo)?, &op(BinOp::Sub, hi, lo)?)?;
        op(BinOp::Add, to_lo, &op(BinOp::Mul, &op(BinOp::Sub, to_hi, to_lo)?, &t)?)
    }
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
