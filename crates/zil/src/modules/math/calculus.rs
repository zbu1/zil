//! Numeric calculus on functions: roots, derivatives, limits, integrals.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "calculus",
    about: "solve f(x) = 0, find slopes, limits and areas under curves, numerically",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Solving equations, finding slopes, and measuring areas under curves, for any function of one number. `root`, \
`deriv` and `integrate` take that function as a lambda, like `|x| x ** 2 - 2`."),
        ("good to know", "\
`root`, `deriv`, `limit` and `integrate` work numerically, not by algebra: they try values until the answer is accurate to \
many digits, so their answers are floats that are very close but not always exact."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("calculus", &[
            ("√2 as the x where x² - 2 = 0", "root(|x| x ** 2 - 2, 1)"),
            ("where cos(x) = x, searching between 0 and 1", "root(|x| cos(x) - x, 0, 1)"),
            ("slope of x³ at 2", "deriv(|x| x ** 3, 2)"),
            ("sin(x)/x near 0, where it can't be computed directly", "limit(|x| sin(x) / x, 0)"),
            ("the area under a bell curve, squared, is pi", "integrate(|x| exp(-x ** 2), -10, 10) ** 2"),
            ("distance from speed: units carry through", "integrate(|t| 9.8 m/s^2 * t, 0 s, 3 s)"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use crate::ast::BinOp;
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::ops::binary;
    use crate::signature::{As, Func, Num, Or, Qty};
    use crate::value::{Value, num};

    fn at(it: &mut Interp, f: &Func, x: f64, span: &Span) -> Result<f64, Fail> {
        let y = it.call(f, vec![Value::Float(x)], span)?;
        num(&y).ok_or_else(|| format!("f must return a number, got {}", y.type_name()).into())
    }

    /// an x where f(x) = 0
    ///
    /// Given a starting guess, finds the nearest one it can (Newton's method). Given lo and hi where f is negative at one and positive at the other, always finds one in between (bisection)
    #[builtin(examples = ["root(|x| x ** 2 - 2, 1)"], see = ["deriv"])]
    fn root(it: &mut Interp, f: Func, guess: f64, span: &Span) -> Result<f64, Fail> {
        let mut x = guess;
        for _ in 0..100 {
            let (y, h) = (at(it, &f, x, span)?, 1e-7 * x.abs().max(1.0));
            if y == 0.0 {
                return Ok(x);
            }
            let slope = (at(it, &f, x + h, span)? - at(it, &f, x - h, span)?) / (2.0 * h);
            let step = y / slope;
            if !step.is_finite() {
                break;
            }
            x -= step;
            if step.abs() < 1e-13 * x.abs().max(1.0) {
                return Ok(x);
            }
        }
        Err("no root found near the guess; try another guess, or root(f, lo, hi)".into())
    }

    #[builtin(name = "root", examples = ["root(|x| x ** 3 - x - 1, 1, 2)"])]
    fn root_between(it: &mut Interp, f: Func, lo: f64, hi: f64, span: &Span) -> Result<f64, Fail> {
        let (mut lo, mut hi) = (lo, hi);
        let mut flo = at(it, &f, lo, span)?;
        if flo.signum() == at(it, &f, hi, span)?.signum() && flo != 0.0 {
            return Err("f(lo) and f(hi) have the same sign, so there may be no root between them".into());
        }
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            let fm = at(it, &f, mid, span)?;
            if fm == 0.0 || hi - lo < 1e-15 * mid.abs().max(1.0) {
                return Ok(mid);
            }
            if fm.signum() == flo.signum() {
                (lo, flo) = (mid, fm);
            } else {
                hi = mid;
            }
        }
        Ok((lo + hi) / 2.0)
    }

    /// the slope of f at x (its derivative), estimated from f just either side of x
    #[builtin(examples = ["deriv(|x| x ** 2, 3)", "deriv(sin, 0)"], see = ["integrate", "root"])]
    fn deriv(it: &mut Interp, f: Func, x: f64, span: &Span) -> Result<f64, Fail> {
        let h = 1e-5 * x.abs().max(1.0);
        // Five-point stencil: error O(h^4).
        let mut f = |x| at(it, &f, x, span);
        Ok((f(x - 2.0 * h)? - 8.0 * f(x - h)? + 8.0 * f(x + h)? - f(x + 2.0 * h)?) / (12.0 * h))
    }

    /// what f(x) approaches as x approaches a, which can be inf or -inf
    ///
    /// Samples f closer and closer to a and extrapolates (Richardson). side is "both", "left" (from below) or "right" (from above), and when the two sides differ, that is an error
    #[builtin(
        examples = ["limit(|x| sin(x) / x, 0)", "limit(|x| (1 + 1 / x) ** x, inf)", r#"limit(|x| abs(x) / x, 0, "right")"#, "limit(|x| 1 / x ** 2, 0)"],
        see = ["deriv"]
    )]
    fn limit(it: &mut Interp, f: Func, a: f64, #[default("both")] side: &str, span: &Span) -> Result<f64, Fail> {
        let sides: &[f64] = match side {
            _ if a.is_infinite() => &[-a.signum()],
            "both" => &[-1.0, 1.0],
            "left" => &[-1.0],
            "right" => &[1.0],
            _ => return Err(Fail::Arg(2, r#"side must be "both", "left" or "right""#.into())),
        };
        let mut found = vec![];
        for &s in sides {
            // At ±inf, f(1/t) as t approaches 0 from the matching side.
            let x = |h: f64| if a.is_infinite() { 1.0 / (-s * h) } else { a + s * h * a.abs().max(1.0) };
            found.push(one_side(|h| at(it, &f, x(h), span))?);
        }
        match found[..] {
            [l, r] if l != r && (l.is_infinite() || r.is_infinite() || (l - r).abs() > 1e-6 * l.abs().max(1.0)) => {
                Err(format!("the limit from the left ({}) differs from the limit from the right ({})", Value::Float(l), Value::Float(r)).into())
            }
            [x, ..] => Ok(x),
            [] => unreachable!("there is at least one side"),
        }
    }

    /// The limit of `g(h)` as `h` shrinks to 0, rounded to the digits it's sure of. First Richardson's table over 10
    /// halvings, kept if it agrees with the samples; else (`sqrt`, `x log x` at 0) the samples down to h = 1e-13, which
    /// either settle, or grow without bound (±inf).
    fn one_side(mut g: impl FnMut(f64) -> Result<f64, Fail>) -> Result<f64, Fail> {
        const N: usize = 10;
        let ys = (0..=40).map(|k| g(0.5 / 2f64.powi(k))).collect::<Result<Vec<_>, _>>()?;
        let round = |x: f64, err: f64| {
            let q = 10f64.powf((10.0 * err).log10().ceil());
            if err > 0.0 { q * (x / q).round() + 0.0 } else { x }
        };
        let mut row = ys[..N].to_vec();
        let (mut best, mut change) = (ys[N - 1], f64::INFINITY);
        for j in 1..N {
            let prev = row[j - 1];
            // `row[k]` holds column `j - 1` for sample `k`; column `j` cancels the h^j error term.
            for k in (j..N).rev() {
                row[k] = row[k] + (row[k] - row[k - 1]) / (2f64.powi(j as i32) - 1.0);
            }
            let d = (row[j] - prev).abs();
            if d < change {
                (best, change) = (row[j], d);
            }
        }
        let scale = best.abs().max(1.0);
        if best.is_finite() && change < 1e-6 * scale && (best - ys[N - 1]).abs() <= 4.0 * (ys[N - 1] - ys[N - 2]).abs() + 1e-9 * scale {
            return Ok(round(best, change));
        }
        let [.., a, b] = ys[..] else { unreachable!() };
        if b.is_finite() && (a - b).abs() < 1e-6 * b.abs().max(1.0) {
            return Ok(round(b, (a - b).abs()));
        }
        let tail = &ys[30..];
        if tail.windows(2).all(|w| w[0].signum() == w[1].signum() && w[1].abs() >= w[0].abs()) && (b.is_infinite() || b.abs() > 2.0 * tail[0].abs()) {
            return Ok(f64::INFINITY.copysign(b));
        }
        Err("no limit found: f keeps changing as x gets closer".into())
    }

    /// the area under f between a and b (its integral), added up from 1000 thin slices with Simpson's rule
    ///
    /// Units carry through, so speed over time gives distance
    #[builtin(examples = ["integrate(|x| x ** 2, 0, 3)", "integrate(sin, 0, pi)"], see = ["deriv"])]
    fn integrate(it: &mut Interp, f: Func, a: Or<Num, Qty>, b: Or<Num, Qty>, span: &Span) -> Result<As<Or<f64, Qty>>, Fail> {
        let float = |v: &Value| num(v).map_or_else(|| v.clone(), Value::Float);
        let (a, b) = (float(a.value()), float(b.value()));
        let o = |op, x: &Value, y: &Value| binary(op, x, y).map_err(Fail::from);
        const N: i64 = 1000;
        let h = o(BinOp::Div, &o(BinOp::Sub, &b, &a)?, &Value::int(N))?;
        let mut acc: Option<Value> = None;
        for i in 0..=N {
            let x = o(BinOp::Add, &a, &o(BinOp::Mul, &h, &Value::int(i))?)?;
            let y = it.call(&f, vec![x], span)?;
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
        Ok(o(BinOp::Mul, &acc.unwrap(), &o(BinOp::Div, &h, &Value::int(3))?)?.into())
    }
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

    #[test]
    fn limits() {
        assert_eq!(
            show("[limit(|x| sin(x) / x, 0), limit(|x| (1 - cos(x)) / x ** 2, 0), limit(|x| (x ** 2 - 4) / (x - 2), 2), limit(|x| (1 + 1 / x) ** x, inf)]"),
            "[1, 0.5, 4, 2.71828]"
        );
        assert_eq!(show(r#"[limit(|x| x * log(x), 0, "right"), limit(sqrt, 0, "right"), limit(|x| x * sin(1 / x), 0), limit(exp, -inf)]"#), "[0, 0, 0, 0]");
        assert_eq!(show(r#"[limit(|x| 1 / x ** 2, 0), limit(|x| 1 / x, 0, "left"), limit(exp, inf), limit(|x| abs(x) / x, 0, "right")]"#), "[∞, -∞, ∞, 1]");
        assert!(try_eval("limit(|x| abs(x) / x, 0)").unwrap_err().msg.contains("differs"));
        assert!(try_eval("limit(|x| 1 / x, 0)").unwrap_err().msg.contains("differs"));
        assert!(try_eval("limit(|x| sin(1 / x), 0)").unwrap_err().msg.contains("no limit"));
    }
}
