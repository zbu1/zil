//! Polynomials as coefficient lists: evaluating, multiplying, dividing, differentiating and finding roots.

use crate::ast::BinOp;
use crate::modules::{Fail, Module};
use crate::ops::binary;
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "poly",
    tags: &["algebra"],
    about: "polynomials written as coefficient lists: evaluate, multiply, divide, differentiate and find roots",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Polynomials, like 2x² - 3x + 5, written as a list of coefficients from the highest power down: `[2, -3, 5]`."),
        ("good to know", "\
Whole-number and fraction coefficients give exact answers, except for finding roots, which is always numeric."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("poly", &[
            ("2x² - 3x + 5 at x = 4", "poly_eval([2, -3, 5], 4)"),
            ("the roots of x² - 3x + 2", "poly_roots([1, -3, 2])"),
            ("(x + 1)(x - 1)", "poly_mul([1, 1], [1, -1])"),
            ("divide x² - 1 by x - 1, as quotient and remainder", "poly_div([1, 0, -1], [1, -1])"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

fn op(o: BinOp, a: &Value, b: &Value) -> Result<Value, Fail> {
    Ok(binary(o, a, b)?)
}

fn is_zero(v: &Value) -> bool {
    num(v) == Some(0.0)
}

/// Drops leading zero coefficients, keeping at least one.
fn trim(mut p: Vec<Value>) -> Vec<Value> {
    let lead = p.iter().take_while(|v| is_zero(v)).count().min(p.len().saturating_sub(1));
    p.drain(..lead);
    p
}

/// Roots by Durand-Kerner iteration.
/// ponytail: 500 fixed rounds, so a root repeated many times converges slowly and comes out accurate only to a few digits.
fn roots(c: &[f64]) -> Vec<(f64, f64)> {
    let lead = c.iter().position(|&x| x != 0.0).unwrap_or(c.len());
    let c = &c[lead..];
    let n = c.len().saturating_sub(1);
    let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let div = |a: (f64, f64), b: (f64, f64)| {
        let d = b.0 * b.0 + b.1 * b.1;
        ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
    };
    let eval = |z: (f64, f64)| {
        c.iter().fold((0.0, 0.0), |acc, &k| {
            let m = mul(acc, z);
            (m.0 + k / c[0], m.1)
        })
    };
    let radius = 1.0 + c.iter().skip(1).map(|k| (k / c[0]).abs()).fold(0.0, f64::max);
    let mut z: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = 0.4 + 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            (radius * a.cos() * 0.5, radius * a.sin() * 0.5)
        })
        .collect();
    for _ in 0..500 {
        let mut moved: f64 = 0.0;
        for i in 0..n {
            let denom = (0..n).filter(|&j| j != i).fold((1.0, 0.0), |acc, j| mul(acc, (z[i].0 - z[j].0, z[i].1 - z[j].1)));
            let step = div(eval(z[i]), denom);
            z[i] = (z[i].0 - step.0, z[i].1 - step.1);
            moved = moved.max(step.0.abs() + step.1.abs());
        }
        if moved < 1e-15 * radius {
            break;
        }
    }
    z.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    z
}

#[zil_macros::builtins]
mod fns {
    use super::{is_zero, op, roots, trim};
    use crate::ast::BinOp::{Add, Div, Mul, Sub};
    use crate::modules::Fail;
    use crate::signature::List;
    use crate::value::{Value, num};

    fn coeffs(p: &List, i: usize) -> Result<Vec<Value>, Fail> {
        let p = p.borrow().clone();
        if p.is_empty() {
            return Err(Fail::Arg(i, "expected a list of coefficients".into()));
        }
        Ok(p)
    }

    /// the polynomial's value at x, from its coefficients highest power first
    #[builtin(examples = ["poly_eval([2, -3, 5], 4)"], see = ["poly_roots", "poly_deriv"])]
    fn poly_eval(p: &List, x: &Value) -> Result<Value, Fail> {
        let p = coeffs(p, 0)?;
        let mut acc = p[0].clone();
        for c in &p[1..] {
            acc = op(Add, &op(Mul, &acc, x)?, c)?;
        }
        Ok(acc)
    }

    /// the product of two polynomials
    #[builtin(examples = ["poly_mul([1, 1], [1, -1])"], see = ["poly_div"])]
    fn poly_mul(a: &List, b: &List) -> Result<Vec<Value>, Fail> {
        let (a, b) = (coeffs(a, 0)?, coeffs(b, 1)?);
        let mut out = vec![Value::int(0); a.len() + b.len() - 1];
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                out[i + j] = op(Add, &out[i + j], &op(Mul, x, y)?)?;
            }
        }
        Ok(out)
    }

    /// divide polynomial a by b, giving the list [quotient, remainder]
    #[builtin(examples = ["poly_div([1, 0, -1], [1, -1])", "poly_div([1, 2, 3], [1, 1])"], see = ["poly_mul"])]
    fn poly_div(a: &List, b: &List) -> Result<Vec<Value>, Fail> {
        let (a, b) = (trim(coeffs(a, 0)?), trim(coeffs(b, 1)?));
        if is_zero(&b[0]) {
            return Err(Fail::Arg(1, "can not divide by the zero polynomial".into()));
        }
        if a.len() < b.len() {
            return Ok(vec![Value::list(vec![Value::int(0)]), Value::list(a)]);
        }
        let (mut rem, mut quot) = (a, vec![]);
        for i in 0..=rem.len() - b.len() {
            let f = op(Div, &rem[i], &b[0])?;
            for (j, c) in b.iter().enumerate() {
                rem[i + j] = op(Sub, &rem[i + j], &op(Mul, &f, c)?)?;
            }
            quot.push(f);
        }
        let rem = rem.split_off(quot.len());
        Ok(vec![Value::list(quot), Value::list(trim(if rem.is_empty() { vec![Value::int(0)] } else { rem }))])
    }

    /// the derivative of a polynomial, as a coefficient list
    #[builtin(examples = ["poly_deriv([3, 2, 1])"], see = ["poly_eval"])]
    fn poly_deriv(p: &List) -> Result<Vec<Value>, Fail> {
        let p = coeffs(p, 0)?;
        let n = p.len() - 1;
        if n == 0 {
            return Ok(vec![Value::int(0)]);
        }
        p[..n].iter().enumerate().map(|(i, c)| op(Mul, c, &Value::int((n - i) as i64))).collect()
    }

    /// every root of the polynomial, real or complex, found numerically
    ///
    /// Real roots come back as plain numbers
    #[builtin(examples = ["poly_roots([1, -3, 2])", "poly_roots([1, 0, 1])"], see = ["poly_eval", "root"])]
    fn poly_roots(p: &List) -> Result<Vec<Value>, Fail> {
        let c = coeffs(p, 0)?.iter().map(|v| num(v).ok_or_else(|| Fail::Arg(0, "coefficients must be numbers".into()))).collect::<Result<Vec<_>, _>>()?;
        if c.len() > 101 {
            return Err(Fail::Arg(0, "degree is limited to 100".into()));
        }
        Ok(roots(&c)
            .into_iter()
            .map(|(re, im)| if im.abs() <= 1e-9 * re.abs().max(1.0) { Value::Float(re) } else { crate::modules::math::complex::mk((re, im)) })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn poly() {
        assert_eq!(show("[poly_eval([2, -3, 5], 4), poly_mul([1, 1], [1, -1]), poly_deriv([3, 2, 1]), poly_deriv([5])]"), "[25, [1, 0, -1], [6, 2], [0]]");
        assert_eq!(show("[poly_div([1, 0, -1], [1, -1]), poly_div([1, 2, 3], [1, 1]), poly_div([1], [1, 1])]"), "[[[1, 1], [0]], [[1, 1], [2]], [[0], [1]]]");
        assert_eq!(show("poly_roots([1, -3, 2]).map(|r| round(r, 6))"), "[1, 2]");
        assert_eq!(show("poly_roots([1, 0, 1]).map(|r| round(im(r), 6))"), "[-1, 1]");
        assert!(try_eval("poly_div([1], [0])").is_err());
    }
}
