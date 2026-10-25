//! Complex numbers, `2 + 3i`: arithmetic, powers, and the main functions on the complex plane.

use crate::ast::BinOp;
use crate::modules::{Claim, Module};
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "complex",
    about: "complex numbers like `2 + 3i`, for math with square roots of negatives",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Complex numbers, which have a real part and an imaginary part, a multiple of i, the square root of -1."),
        ("writing", "\
Write the imaginary part as a number with `i` right after it and no space: `3i`, `0.5i`, and `1i` for i itself. `abs`, \
`exp`, `ln` and `sqrt` accept complex numbers too."),
    ],
    #[rustfmt::skip]
    guide: &[
        ("literals", &[("an imaginary number: i touching a number", "3i"), ("real part + imaginary part", "2 + 3i")]),
        ("operators", &[("+ - * / ** work as usual", "(1 + 2i) * (3 - 1i)")]),
        ("functions", &[("abs, exp, ln and sqrt accept complex numbers", "sqrt(-4 + 0i)")]),
    ],
    #[rustfmt::skip]
    examples: &[
        ("complex", &[
            ("Euler's identity: e to the i pi is -1", "e ** (1i * pi)"),
            ("roots of x² + 2x + 5", "[-1 + csqrt(-4) / 2, -1 - csqrt(-4) / 2]"),
            ("as a distance and an angle", "1 + 1i to polar"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    binary: Some(binary),
    targets: &[("polar", "polar")],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::sqrt;
    use crate::modules::{Fail, units};
    use crate::signature::{As, Complex, Or, Qty};
    use crate::value::{Value, fmt_float};

    /// Real and imaginary parts of a number or a complex number.
    fn z(v: Or<f64, Complex>) -> (f64, f64) {
        match v {
            Or::A(x) => (x, 0.0),
            Or::B(z) => (z.0, z.1),
        }
    }

    /// the real part
    #[builtin(examples = ["re(2 + 3i)"], see = ["im"])]
    fn re(z: Or<f64, Complex>) -> f64 {
        self::z(z).0
    }

    /// the imaginary part, as a plain number
    #[builtin(examples = ["im(2 + 3i)"], see = ["re"])]
    fn im(z: Or<f64, Complex>) -> f64 {
        self::z(z).1
    }

    /// the complex conjugate: the same number with its imaginary part negated
    #[builtin(examples = ["conj(2 + 3i)"], see = ["re", "im"])]
    fn conj(z: Or<f64, Complex>) -> Complex {
        let (re, im) = self::z(z);
        Complex(re, -im)
    }

    /// the angle from the positive real axis to the number, as an angle unit
    #[builtin(examples = ["arg(1i) to deg"], see = ["polar", "abs"])]
    fn arg(z: Or<f64, Complex>) -> As<Qty> {
        let (re, im) = self::z(z);
        Value::Qty(im.atan2(re), units::unit("rad").expect("builtin unit")).into()
    }

    /// square root that gives a complex number for negatives, so csqrt(-4) is 2i
    ///
    /// sqrt(-4) is an error instead, so that a calculation going negative by mistake doesn't quietly turn complex. Of the two square roots, gives the one with a positive real part
    #[builtin(examples = ["csqrt(-4)"], see = ["sqrt"])]
    fn csqrt(z: Or<f64, Complex>) -> Complex {
        sqrt(self::z(z)).into()
    }

    /// make a complex number from its polar form, or with one argument show z in that form
    ///
    /// A complex number seen as a point on a plane, real part across and imaginary part up, has a polar form: its distance from 0 and its angle. With one argument, the same as `z to polar`
    #[builtin(examples = ["polar(2, 90 deg)", "polar(1i)"], see = ["arg", "abs"])]
    fn polar(r: f64, theta: Or<f64, Qty>) -> Result<Complex, Fail> {
        let theta = match theta {
            Or::A(x) => x,
            Or::B(q) => {
                let (x, u) = q.get();
                if u.dim() != units::unit("rad").expect("builtin unit").dim() {
                    return Err(Fail::BadArgs);
                }
                u.to_si(x)
            }
        };
        Ok(Complex(r * theta.cos(), r * theta.sin()))
    }

    #[builtin(name = "polar")]
    fn polar_of(z: Or<f64, Complex>) -> String {
        let (re, im) = self::z(z);
        format!("polar({}, {} deg)", fmt_float(re.hypot(im)), fmt_float(im.atan2(re).to_degrees()))
    }
}

/// Builtins from other modules that take complex numbers; `call_builtin` sends them here.
type C = (f64, f64);

/// Real and imaginary parts of any plain number.
fn parts(v: &Value) -> Option<C> {
    match v {
        Value::Cplx(re, im) => Some((*re, *im)),
        v => Some((num(v)?, 0.0)),
    }
}

/// Rounding noise like `exp(1i * pi)`'s 1.2e-16 imaginary part is dropped.
pub fn mk((re, im): C) -> Value {
    let snap = |x: f64, other: f64| if x.abs() < other.abs() * 1e-15 { 0.0 } else { x };
    Value::Cplx(snap(re, im), snap(im, re))
}

fn mul((a, b): C, (c, d): C) -> C {
    (a * c - b * d, a * d + b * c)
}

fn div((a, b): C, (c, d): C) -> C {
    let n = c * c + d * d;
    ((a * c + b * d) / n, (b * c - a * d) / n)
}

pub fn exp((a, b): C) -> C {
    let r = a.exp();
    (r * b.cos(), r * b.sin())
}

pub fn ln((a, b): C) -> C {
    (a.hypot(b).ln(), b.atan2(a))
}

/// The principal root, exact on the axes: `csqrt(-4)` is `2i`, not `1.2e-16 + 2i`.
pub fn sqrt((a, b): C) -> C {
    let t = ((a.hypot(b) + a.abs()) / 2.0).sqrt();
    if t == 0.0 {
        (0.0, 0.0)
    } else if a >= 0.0 {
        (t, b / (2.0 * t))
    } else {
        (b.abs() / (2.0 * t), t.copysign(b))
    }
}

/// Integer powers by repeated squaring, so `(1i) ** 2` is exactly -1.
fn pow(z: C, (c, d): C) -> C {
    if d == 0.0 && c.fract() == 0.0 && c.abs() <= 1024.0 {
        let (mut acc, mut base, mut n) = ((1.0, 0.0), z, c.abs() as u32);
        while n > 0 {
            if n & 1 == 1 {
                acc = mul(acc, base);
            }
            base = mul(base, base);
            n >>= 1;
        }
        return if c < 0.0 { div((1.0, 0.0), acc) } else { acc };
    }
    if z == (0.0, 0.0) {
        return (0.0, 0.0);
    }
    exp(mul((c, d), ln(z)))
}

fn binary(o: BinOp, a: &Value, b: &Value) -> Claim {
    if !matches!(a, Value::Cplx(..)) && !matches!(b, Value::Cplx(..)) {
        return None;
    }
    let (x, y) = (parts(a)?, parts(b)?);
    Some(Ok(mk(match o {
        BinOp::Add => (x.0 + y.0, x.1 + y.1),
        BinOp::Sub => (x.0 - y.0, x.1 - y.1),
        BinOp::Mul => mul(x, y),
        BinOp::Div => div(x, y),
        BinOp::Pow => pow(x, y),
        _ => return None,
    })))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn complex() {
        assert_eq!(show("[2 + 3i, 2 - 3i, 3i, -1i, 1.5i, type(1i)]"), r#"[2 + 3i, 2 - 3i, 3i, -1i, 1.5i, "complex"]"#);
        assert_eq!(show("[(1 + 2i) * (3 - 1i), (1 + 2i) / (1 - 1i), 1i ** 2, (1 + 1i) ** -2]"), "[5 + 5i, -0.5 + 1.5i, -1 + 0i, -0.5i]");
        assert_eq!(show("[e ** (1i * pi), 2 ** 1i]"), "[-1 + 0i, 0.769239 + 0.638961i]");
        assert_eq!(show("[abs(3 + 4i), conj(2 + 3i), re(2 + 3i), im(2 + 3i), arg(1i) to deg]"), "[5, 2 - 3i, 2, 3, 90 deg]");
        assert_eq!(show("[sqrt(-4 + 0i), csqrt(-4), csqrt(4), exp(1i * pi / 2), ln(-1 + 0i)]"), "[2i, 2i, 2 + 0i, 1i, 3.14159i]");
        assert_eq!(show("[polar(2, 90 deg), 1 + 1i to polar]"), r#"[2i, "polar(1.41421, 45 deg)"]"#);
        assert_eq!(show("[1i == 1i, 1i != 2i, -(1 + 1i)]"), "[true, true, -1 - 1i]");
        assert_eq!(show("i = 5; 2*i"), "10");
        assert!(try_eval("sqrt(-1)").is_err());
        assert!(try_eval("1i < 2i").is_err());
    }
}
