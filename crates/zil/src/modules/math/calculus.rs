//! Numeric calculus on functions: roots, derivatives, integrals.

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::ops::binary;
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "calculus",
    about: "solve f(x) = 0, find slopes and areas under curves, numerically",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Solving equations, finding slopes, and measuring areas under curves, for any function of one number. `root`, \
`deriv` and `integrate` take that function as a lambda, like `|x| x ** 2 - 2`."),
        ("good to know", "\
`root`, `deriv` and `integrate` work numerically, not by algebra: they try values until the answer is accurate to \
many digits, so their answers are floats that are very close but not always exact."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("calculus", &[
            ("√2 as the x where x² - 2 = 0", "root(|x| x ** 2 - 2, 1)"),
            ("where cos(x) = x, searching between 0 and 1", "root(|x| cos(x) - x, 0, 1)"),
            ("slope of x³ at 2", "deriv(|x| x ** 3, 2)"),
            ("the area under a bell curve, squared, is pi", "integrate(|x| exp(-x ** 2), -10, 10) ** 2"),
            ("distance from speed: units carry through", "integrate(|t| 9.8 m/s^2 * t, 0 s, 3 s)"),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("root", "root(f: fn, guess: num) -> float / root(f: fn, lo: num, hi: num) -> float", "an x where f(x) = 0", &["root(|x| x ** 2 - 2, 1)", "root(|x| x ** 3 - x - 1, 1, 2)"], &["deriv"])
        .detail("Given a starting guess, finds the nearest one it can (Newton's method). Given lo and hi where f is negative at one and positive at the other, always finds one in between (bisection)"),
    doc("deriv", "deriv(f: fn, x: num) -> float", "the slope of f at x (its derivative), estimated from f just either side of x", &["deriv(|x| x ** 2, 3)", "deriv(sin, 0)"], &["integrate", "root"]),
    doc("integrate", "integrate(f: fn, a: num|quantity, b: num|quantity) -> float|quantity", "the area under f between a and b (its integral), added up from 1000 thin slices with Simpson's rule", &["integrate(|x| x ** 2, 0, 3)", "integrate(sin, 0, pi)"], &["deriv"])
        .detail("Units carry through, so speed over time gives distance"),
];

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    let mut f = |x: f64| -> Result<f64, Fail> {
        let y = it.call(&args[0], vec![Value::Float(x)], span)?;
        num(&y).ok_or_else(|| format!("f must return a number, got {}", y.type_name()).into())
    };
    let x = |i: usize| num(&args[i]).ok_or(Fail::BadArgs);
    Ok(match (name, args.len()) {
        ("root", 2) => {
            let mut x = x(1)?;
            for _ in 0..100 {
                let (y, h) = (f(x)?, 1e-7 * x.abs().max(1.0));
                if y == 0.0 {
                    return Ok(Value::Float(x));
                }
                let slope = (f(x + h)? - f(x - h)?) / (2.0 * h);
                let step = y / slope;
                if !step.is_finite() {
                    break;
                }
                x -= step;
                if step.abs() < 1e-13 * x.abs().max(1.0) {
                    return Ok(Value::Float(x));
                }
            }
            return Err("no root found near the guess; try another guess, or root(f, lo, hi)".into());
        }
        ("root", 3) => {
            let (mut lo, mut hi) = (x(1)?, x(2)?);
            let mut flo = f(lo)?;
            if flo.signum() == f(hi)?.signum() && flo != 0.0 {
                return Err("f(lo) and f(hi) have the same sign, so there may be no root between them".into());
            }
            for _ in 0..200 {
                let mid = (lo + hi) / 2.0;
                let fm = f(mid)?;
                if fm == 0.0 || hi - lo < 1e-15 * mid.abs().max(1.0) {
                    return Ok(Value::Float(mid));
                }
                if fm.signum() == flo.signum() {
                    (lo, flo) = (mid, fm);
                } else {
                    hi = mid;
                }
            }
            Value::Float((lo + hi) / 2.0)
        }
        ("deriv", 2) => {
            let x = x(1)?;
            let h = 1e-5 * x.abs().max(1.0);
            // Five-point stencil: error O(h^4).
            Value::Float((f(x - 2.0 * h)? - 8.0 * f(x - h)? + 8.0 * f(x + h)? - f(x + 2.0 * h)?) / (12.0 * h))
        }
        ("integrate", 3) => {
            let float = |v: &Value| num(v).map_or_else(|| v.clone(), Value::Float);
            let (a, b) = (float(&args[1]), float(&args[2]));
            let o = |op, x: &Value, y: &Value| binary(op, x, y).map_err(Fail::from);
            const N: i64 = 1000;
            let h = o(BinOp::Div, &o(BinOp::Sub, &b, &a)?, &Value::int(N))?;
            let mut acc: Option<Value> = None;
            for i in 0..=N {
                let x = o(BinOp::Add, &a, &o(BinOp::Mul, &h, &Value::int(i))?)?;
                let y = it.call(&args[0], vec![x], span)?;
                let w = Value::int(if i == 0 || i == N {
                    1
                } else if i % 2 == 1 {
                    4
                } else {
                    2
                });
                let term = o(BinOp::Mul, &y, &w)?;
                acc = Some(match acc {
                    None => term,
                    Some(acc) => o(BinOp::Add, &acc, &term)?,
                });
            }
            o(BinOp::Mul, &acc.unwrap(), &o(BinOp::Div, &h, &Value::int(3))?)?
        }
        _ => return Err(Fail::BadArgs),
    })
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn calculus() {
        assert_eq!(show("[root(|x| x ** 2 - 2, 1), root(|x| x ** 2 - 2, 0, 5), root(|x| cos(x) - x, 0, 1)]"), "[1.41421, 1.41421, 0.739085]");
        assert_eq!(show("[deriv(|x| x ** 3, 2), deriv(sin, 0), deriv(exp, 1)]"), "[12, 1, 2.71828]");
        assert_eq!(show("[integrate(|x| x ** 2, 0, 3), integrate(sin, 0, pi)]"), "[9, 2]");
        assert_eq!(show("integrate(|t| 9.8 m/s^2 * t, 0 s, 3 s)"), "44.1 m");
        assert!(try_eval("root(|x| x ** 2 + 1, 0, 1)").is_err());
        assert!(try_eval("root(|x| x ** 2 + 1, 0)").is_err());
        assert!(try_eval(r#"deriv(|x| "a", 1)"#).is_err());
    }
}
