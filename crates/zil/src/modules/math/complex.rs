//! Complex numbers, `2 + 3i`: arithmetic, powers, and the main functions on the complex plane.

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc, units};
use crate::value::{Value, fmt_float, num};

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
    fns: FNS,
    call,
    binary: Some(binary),
    targets: &[("polar", "polar")],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("re", "re(z: num|complex) -> float", "the real part", &["re(2 + 3i)"], &["im"]),
    doc("im", "im(z: num|complex) -> float", "the imaginary part, as a plain number", &["im(2 + 3i)"], &["re"]),
    doc("conj", "conj(z: num|complex) -> complex", "the complex conjugate: the same number with its imaginary part negated", &["conj(2 + 3i)"], &["re", "im"]),
    doc("arg", "arg(z: num|complex) -> quantity", "the angle from the positive real axis to the number, as an angle unit", &["arg(1i) to deg"], &["polar", "abs"]),
    doc("csqrt", "csqrt(z: num|complex) -> complex", "square root that gives a complex number for negatives, so csqrt(-4) is 2i", &["csqrt(-4)"], &["sqrt"])
        .detail("sqrt(-4) is an error instead, so that a calculation going negative by mistake doesn't quietly turn complex. Of the two square roots, gives the one with a positive real part"),
    doc("polar", "polar(r: num, theta: num|quantity) -> complex / polar(z: num|complex) -> str", "make a complex number from its polar form, or with one argument show z in that form", &["polar(2, 90 deg)", "polar(1i)"], &["arg", "abs"])
        .detail("A complex number seen as a point on a plane, real part across and imaginary part up, has a polar form: its distance from 0 and its angle. With one argument, the same as `z to polar`"),
];

/// Builtins from other modules that take complex numbers; `call_builtin` sends them here.
pub const LIFTED: &[&str] = &["abs", "exp", "ln", "sqrt"];

type C = (f64, f64);

/// Real and imaginary parts of any plain number.
fn parts(v: &Value) -> Option<C> {
    match v {
        Value::Cplx(re, im) => Some((*re, *im)),
        v => Some((num(v)?, 0.0)),
    }
}

/// Rounding noise like `exp(1i * pi)`'s 1.2e-16 imaginary part is dropped.
fn mk((re, im): C) -> Value {
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

fn exp((a, b): C) -> C {
    let r = a.exp();
    (r * b.cos(), r * b.sin())
}

fn ln((a, b): C) -> C {
    (a.hypot(b).ln(), b.atan2(a))
}

/// The principal root, exact on the axes: `csqrt(-4)` is `2i`, not `1.2e-16 + 2i`.
fn sqrt((a, b): C) -> C {
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

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    let rad = || units::unit("rad").unwrap();
    let z = |i: usize| args.get(i).and_then(parts).ok_or(Fail::BadArgs);
    Ok(match (name, args.len()) {
        ("re", 1) => Value::Float(z(0)?.0),
        ("im", 1) => Value::Float(z(0)?.1),
        ("conj", 1) => Value::Cplx(z(0)?.0, -z(0)?.1),
        ("arg", 1) => Value::Qty(z(0)?.1.atan2(z(0)?.0), rad()),
        ("abs", 1) => Value::Float(z(0)?.0.hypot(z(0)?.1)),
        ("exp", 1) => mk(exp(z(0)?)),
        ("ln", 1) => mk(ln(z(0)?)),
        ("sqrt" | "csqrt", 1) => mk(sqrt(z(0)?)),
        ("polar", 1) => {
            let (re, im) = z(0)?;
            Value::str(format!("polar({}, {} deg)", fmt_float(re.hypot(im)), fmt_float(im.atan2(re).to_degrees())))
        }
        ("polar", 2) => {
            let r = num(&args[0]).ok_or(Fail::BadArgs)?;
            let theta = match &args[1] {
                Value::Qty(x, u) if u.dim() == rad().dim() => u.to_si(*x),
                v => num(v).ok_or(Fail::BadArgs)?,
            };
            mk((r * theta.cos(), r * theta.sin()))
        }
        _ => return Err(Fail::BadArgs),
    })
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
