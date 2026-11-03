//! Numeric calculus on functions: roots, derivatives, limits, integrals.

use crate::modules::{Fail, Module};
use std::cell::Cell;

pub const MODULE: Module = Module {
    name: "calculus",
    tags: &["algebra"],
    about: "solve f(x) = 0, find slopes, limits and areas under curves, numerically",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Solving equations, finding slopes, and measuring areas under curves, for any function of one number. The function \
is given as a lambda, like `|x| x ** 2 - 2`."),
        ("good to know", "\
These work numerically, not by algebra: they try values until the answer is accurate to \
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
    use super::{adaptive, golden};
    use crate::ast::BinOp;
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::ops::binary;
    use crate::signature::{As, Func, List, Num, Or, Qty};
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
        // Newton stalled (a flat spot, say): the secant method needs no slope.
        let (mut a, mut b) = (guess, guess + 1e-3 * guess.abs().max(1.0));
        let (mut fa, mut fb) = (at(it, &f, a, span)?, at(it, &f, b, span)?);
        for _ in 0..100 {
            if fb == 0.0 || (b - a).abs() < 1e-13 * b.abs().max(1.0) {
                return Ok(b);
            }
            let next = b - fb * (b - a) / (fb - fa);
            if !next.is_finite() {
                break;
            }
            (a, fa, b) = (b, fb, next);
            fb = at(it, &f, b, span)?;
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

    /// the slope of f at x (its derivative), estimated from f just either side of x, or with n its nth derivative
    ///
    /// Orders above 1 are less accurate, and go up to 6
    #[builtin(examples = ["deriv(|x| x ** 2, 3)", "deriv(sin, 0)", "deriv(|x| x ** 4, 1, 2)"], see = ["integrate", "root"])]
    fn deriv(it: &mut Interp, f: Func, x: f64, n: Option<i64>, span: &Span) -> Result<f64, Fail> {
        let mut f = |x| at(it, &f, x, span);
        match n.unwrap_or(1) {
            1 => {
                let h = 1e-5 * x.abs().max(1.0);
                // Five-point stencil: error O(h^4).
                Ok((f(x - 2.0 * h)? - 8.0 * f(x - h)? + 8.0 * f(x + h)? - f(x + 2.0 * h)?) / (12.0 * h))
            }
            n @ 2..=6 => {
                let h = x.abs().max(1.0) * f64::EPSILON.powf(1.0 / (n as f64 + 2.0));
                // The nth central difference: sum of (-1)^k C(n, k) f(x + (n/2 - k) h), over h^n.
                let (mut sum, mut c) = (0.0, 1.0);
                for k in 0..=n {
                    sum += (if k % 2 == 0 { c } else { -c }) * f(x + (n as f64 / 2.0 - k as f64) * h)?;
                    c = c * (n - k) as f64 / (k + 1) as f64;
                }
                Ok(sum / h.powi(n as i32))
            }
            n => Err(Fail::Arg(2, format!("the order must be from 1 to 6, got {n}"))),
        }
    }

    /// the x between lo and hi where f is smallest
    ///
    /// Squeezes the range around the lowest point (golden-section search), so f should dip once between lo and hi. Accurate to about 8 digits
    #[builtin(examples = ["minimize(|x| (x - 2) ** 2 + 1, 0, 5)", "minimize(cos, 2, 5)"], see = ["maximize", "root"])]
    fn minimize(it: &mut Interp, f: Func, lo: f64, hi: f64, span: &Span) -> Result<f64, Fail> {
        golden(|x| at(it, &f, x, span), lo, hi)
    }

    /// the x between lo and hi where f is largest
    ///
    /// Same search as minimize, so f should have one peak between lo and hi
    #[builtin(examples = ["maximize(sin, 0, 3)", "maximize(|x| x * (10 - x), 0, 10)"], see = ["minimize", "root"])]
    fn maximize(it: &mut Interp, f: Func, lo: f64, hi: f64, span: &Span) -> Result<f64, Fail> {
        golden(|x| at(it, &f, x, span).map(|y| -y), lo, hi)
    }

    /// the point where every function in fs is 0 at once, found from a starting guess (Newton's method)
    ///
    /// Each function takes one number per unknown and the guess is a list with one number each, like `solve_system([|x, y| x + y - 3, |x, y| x - y - 1], [0, 0])`
    #[builtin(examples = ["solve_system([|x, y| x + y - 3, |x, y| x - y - 1], [0, 0])", "solve_system([|x, y| x ** 2 + y ** 2 - 4, |x, y| x - y], [1, 1])"], see = ["root"])]
    fn solve_system(it: &mut Interp, fs: &List, guess: &List, span: &Span) -> Result<Vec<Value>, Fail> {
        let fs: Vec<Value> = fs.borrow().clone();
        let mut x: Vec<f64> = guess.borrow().iter().map(|v| num(v).ok_or(Fail::Arg(1, "the guess must be numbers".into()))).collect::<Result<_, _>>()?;
        let n = x.len();
        if n == 0 || fs.len() != n {
            return Err(Fail::Arg(1, format!("{} functions need a guess with {} numbers", fs.len(), fs.len())));
        }
        let eval = |it: &mut Interp, x: &[f64]| -> Result<Vec<f64>, Fail> {
            fs.iter()
                .map(|f| {
                    let y = it.call(f, x.iter().map(|&v| Value::Float(v)).collect(), span)?;
                    num(&y).ok_or_else(|| format!("each function must return a number, got {}", y.type_name()).into())
                })
                .collect()
        };
        for _ in 0..100 {
            let y = eval(it, &x)?;
            if y.iter().all(|v| *v == 0.0) {
                break;
            }
            // Jacobian by forward differences, then solve J d = y by Gaussian elimination.
            let mut m = vec![vec![0.0; n + 1]; n];
            for j in 0..n {
                let h = 1e-7 * x[j].abs().max(1.0);
                let mut xh = x.clone();
                xh[j] += h;
                for (i, yh) in eval(it, &xh)?.into_iter().enumerate() {
                    m[i][j] = (yh - y[i]) / h;
                }
            }
            for i in 0..n {
                m[i][n] = y[i];
            }
            for c in 0..n {
                let p = (c..n).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs())).expect("a non-empty range");
                if m[p][c].abs() < 1e-10 {
                    return Err("the system has no unique solution near the guess; try another guess".into());
                }
                m.swap(c, p);
                for r in c + 1..n {
                    let k = m[r][c] / m[c][c];
                    let pivot = m[c].clone();
                    for (x, p) in m[r][c..].iter_mut().zip(&pivot[c..]) {
                        *x -= k * p;
                    }
                }
            }
            let mut d = vec![0.0; n];
            for i in (0..n).rev() {
                d[i] = (m[i][n] - (i + 1..n).map(|j| m[i][j] * d[j]).sum::<f64>()) / m[i][i];
            }
            let mut step = 0.0f64;
            for i in 0..n {
                x[i] -= d[i];
                step = step.max(d[i].abs() / x[i].abs().max(1.0));
            }
            if !x.iter().all(|v| v.is_finite()) {
                break;
            }
            if step < 1e-13 {
                return Ok(x.into_iter().map(Value::Float).collect());
            }
        }
        let y = eval(it, &x)?;
        if x.iter().all(|v| v.is_finite()) && y.iter().all(|v| v.abs() < 1e-9) {
            return Ok(x.into_iter().map(Value::Float).collect());
        }
        Err("no solution found near the guess; try another guess".into())
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

    /// the area under f between a and b (its integral), where a and b can be inf or -inf
    ///
    /// With plain numbers it splits the range finer where f bends most (adaptive Simpson's rule), and with quantities it adds up 1000 even slices, so units carry through and speed over time gives distance
    #[builtin(examples = ["integrate(|x| x ** 2, 0, 3)", "integrate(sin, 0, pi)", "integrate(|x| exp(-x ** 2), -inf, inf)"], see = ["deriv"])]
    fn integrate(it: &mut Interp, f: Func, a: Or<Num, Qty>, b: Or<Num, Qty>, span: &Span) -> Result<As<Or<f64, Qty>>, Fail> {
        if let (Or::A(a), Or::A(b)) = (&a, &b)
            && let (Some(a), Some(b)) = (num(a), num(b))
        {
            return Ok(Value::Float(adaptive(|x| at(it, &f, x, span), a, b)?).into());
        }
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
        Ok(o(BinOp::Mul, &acc.expect("N + 1 terms"), &o(BinOp::Div, &h, &Value::int(3))?)?.into())
    }
}

/// `f` integrated from `a` to `b` (either may be infinite) by adaptive Simpson's rule; an infinite range is mapped onto a finite one.
fn adaptive(mut f: impl FnMut(f64) -> Result<f64, Fail>, a: f64, b: f64) -> Result<f64, Fail> {
    if a.is_nan() || b.is_nan() {
        return Err(Fail::BadArgs);
    }
    if a == b {
        return Ok(0.0);
    }
    if a > b {
        return adaptive(f, b, a).map(|x| -x);
    }
    let infinite = a.is_infinite() || b.is_infinite();
    // A mapped range's ends are out of f's reach, so there it takes the value just inside.
    let mut g = |t: f64| -> Result<f64, Fail> {
        let mut at = |t: f64| -> Result<f64, Fail> {
            let (x, dx) = match (a.is_infinite(), b.is_infinite()) {
                (true, true) => (t / (1.0 - t * t), (1.0 + t * t) / (1.0 - t * t).powi(2)),
                (false, true) => (a + t / (1.0 - t), 1.0 / (1.0 - t).powi(2)),
                (true, false) => (b - t / (1.0 - t), 1.0 / (1.0 - t).powi(2)),
                _ => (t, 1.0),
            };
            Ok(f(x)? * dx)
        };
        let y = at(t)?;
        if y.is_finite() || !infinite {
            return Ok(y);
        }
        let inside = if t >= 0.5 { t - 1e-10 } else { t + 1e-10 };
        let y = at(inside)?;
        Ok(if y.is_finite() { y } else { 0.0 })
    };
    let (lo, hi) = match (a.is_infinite(), b.is_infinite()) {
        (true, true) => (-1.0, 1.0),
        (false, false) => (a, b),
        _ => (0.0, 1.0),
    };
    fn simpson(
        g: &mut impl FnMut(f64) -> Result<f64, Fail>,
        (a, fa): (f64, f64),
        (b, fb): (f64, f64),
        (m, fm): (f64, f64),
        whole: f64,
        (tol, depth): (f64, u32),
        stuck: &Cell<bool>,
    ) -> Result<f64, Fail> {
        let (lm, rm) = ((a + m) / 2.0, (m + b) / 2.0);
        let (flm, frm) = (g(lm)?, g(rm)?);
        let (left, right) = ((m - a) / 6.0 * (fa + 4.0 * flm + fm), (b - m) / 6.0 * (fm + 4.0 * frm + fb));
        let delta = left + right - whole;
        if depth == 0 || delta.abs() <= 15.0 * tol {
            stuck.set(stuck.get() || delta.abs() > 1e4 * tol);
            return Ok(left + right + delta / 15.0);
        }
        Ok(simpson(g, (a, fa), (m, fm), (lm, flm), left, (tol / 2.0, depth - 1), stuck)?
            + simpson(g, (m, fm), (b, fb), (rm, frm), right, (tol / 2.0, depth - 1), stuck)?)
    }
    // Eight panels first, so a bump the five sample points miss still shows.
    const PANELS: usize = 8;
    let xs: Vec<f64> = (0..=PANELS).map(|i| lo + (hi - lo) * i as f64 / PANELS as f64).collect();
    let fs = xs.iter().map(|&x| g(x)).collect::<Result<Vec<_>, _>>()?;
    let scale = fs.iter().fold(0.0f64, |m, y| m.max(y.abs())) * (hi - lo);
    let tol = 1e-10 * scale.max(1e-3) / PANELS as f64;
    let (mut total, stuck) = (0.0, Cell::new(false));
    for i in 0..PANELS {
        let m = (xs[i] + xs[i + 1]) / 2.0;
        let fm = g(m)?;
        let whole = (xs[i + 1] - xs[i]) / 6.0 * (fs[i] + 4.0 * fm + fs[i + 1]);
        total += simpson(&mut g, (xs[i], fs[i]), (xs[i + 1], fs[i + 1]), (m, fm), whole, (tol, 40), &stuck)?;
    }
    if total.is_finite() && !stuck.get() { Ok(total) } else { Err("the integral doesn't converge".into()) }
}

/// The x in [lo, hi] where `f` is smallest, by golden-section search.
fn golden(mut f: impl FnMut(f64) -> Result<f64, Fail>, mut lo: f64, mut hi: f64) -> Result<f64, Fail> {
    if lo >= hi {
        return Err(Fail::Arg(2, "lo must be below hi".into()));
    }
    let r = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (hi - r * (hi - lo), lo + r * (hi - lo));
    let (mut fa, mut fb) = (f(a)?, f(b)?);
    for _ in 0..200 {
        if hi - lo < 1e-12 * (lo.abs() + hi.abs()).max(1e-300) {
            break;
        }
        if fa < fb {
            (hi, b, fb) = (b, a, fa);
            a = hi - r * (hi - lo);
            fa = f(a)?;
        } else {
            (lo, a, fa) = (a, b, fb);
            b = lo + r * (hi - lo);
            fb = f(b)?;
        }
    }
    Ok((lo + hi) / 2.0)
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
        assert_eq!(
            show(
                "[integrate(|x| exp(-x ** 2), -inf, inf) ** 2, integrate(|x| exp(-x), 0, inf), integrate(|x| 1 / x ** 2, 1, inf), integrate(exp, 0, -inf), integrate(|x| abs(x), 3, -1)]"
            ),
            "[3.14159, 1, 1, -1, -5]"
        );
        assert!(try_eval("integrate(|x| 1, 0, inf)").is_err());
        assert_eq!(show("[deriv(|x| x ** 4, 1, 2), deriv(|x| x ** 3, 2, 3), deriv(sin, 0, 2)]"), "[12, 6, 0]");
        assert_eq!(show("[minimize(|x| (x - 2) ** 2 + 1, 0, 5), maximize(sin, 0, 3), root(|x| x ** 2 - 4, 0)]"), "[2, 1.5708, 2]");
        assert_eq!(show("solve_system([|x, y| x + y - 3, |x, y| x - y - 1], [0, 0])"), "[2, 1]");
        assert_eq!(show("solve_system([|x, y| x ** 2 + y ** 2 - 4, |x, y| x - y], [1, 1]).map(|v| round(v, 4))"), "[1.4142, 1.4142]");
        assert!(try_eval("solve_system([|x, y| x + y, |x, y| x + y + 1], [0, 0])").is_err());
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
