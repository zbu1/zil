//! Trigonometry: angles in radians or any angle unit.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "trig",
    tags: &["geometry"],
    about: "sine, cosine, tangent and their inverses, in radians or degrees",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Sine, cosine, tangent and their inverses.\n\n\
A plain number is read as radians, where a full turn is 2 pi. To use degrees, give an angle unit, like `30 deg`. \
The inverses return an angle, which you can convert with `to deg`."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("trig", &[
            ("angles can be in degrees", "sin(30 deg)"),
            ("radians back to degrees", "atan2(1, 1) to deg"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use crate::modules::{Fail, units};
    use crate::signature::{As, Or, Qty};
    use crate::value::Value;

    /// An angle in radians: a number, or a quantity in an angle unit.
    fn radians(x: Or<f64, Qty>) -> Result<f64, Fail> {
        match x {
            Or::A(x) => Ok(x),
            Or::B(q) => {
                let (x, u) = q.get();
                if u.dim() != units::unit("rad").expect("builtin unit").dim() {
                    return Err(Fail::BadArgs);
                }
                Ok(u.to_si(x))
            }
        }
    }

    fn angle(rad: f64) -> As<Qty> {
        Value::Qty(rad, units::unit("rad").expect("builtin unit")).into()
    }

    /// the sine of an angle, in radians or an angle unit
    #[builtin(examples = ["sin(30 deg)", "sin(pi / 2)"], see = ["cos", "tan", "asin"])]
    fn sin(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.sin())
    }

    /// the cosine of an angle, in radians or an angle unit
    #[builtin(examples = ["cos(60 deg)"], see = ["sin", "tan", "acos"])]
    fn cos(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.cos())
    }

    /// the tangent of an angle, in radians or an angle unit
    #[builtin(examples = ["tan(45 deg)"], see = ["sin", "cos", "atan"])]
    fn tan(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.tan())
    }

    /// the angle whose sine is x (arcsine)
    #[builtin(examples = ["asin(1) to deg"], see = ["sin"])]
    fn asin(x: f64) -> As<Qty> {
        angle(x.asin())
    }

    /// the angle whose cosine is x (arccosine)
    #[builtin(examples = ["acos(0) to deg"], see = ["cos"])]
    fn acos(x: f64) -> As<Qty> {
        angle(x.acos())
    }

    /// the angle whose tangent is x (arctangent)
    #[builtin(examples = ["atan(1) to deg"], see = ["tan", "atan2"])]
    fn atan(x: f64) -> As<Qty> {
        angle(x.atan())
    }

    /// the angle from the positive x axis to the point (x, y), from -180° to 180°
    ///
    /// Unlike atan(y / x), it knows which quarter the point is in. Note y comes first
    #[builtin(examples = ["atan2(1, -1) to deg"], see = ["atan", "hypot"])]
    fn atan2(y: Qty, x: Qty) -> Result<As<Qty>, Fail> {
        let ((y, u), (x, w)) = (y.get(), x.get());
        if u.dim() != w.dim() {
            return Err(Fail::BadArgs);
        }
        Ok(angle(u.to_si(y).atan2(w.to_si(x))))
    }

    #[builtin(name = "atan2")]
    fn atan2_num(y: f64, x: f64) -> As<Qty> {
        angle(y.atan2(x))
    }

    /// the hyperbolic sine, (eˣ - e⁻ˣ) / 2
    #[builtin(examples = ["sinh(1)"], see = ["cosh", "tanh"])]
    fn sinh(x: f64) -> f64 {
        x.sinh()
    }

    /// the hyperbolic cosine, (eˣ + e⁻ˣ) / 2
    #[builtin(examples = ["cosh(1)"], see = ["sinh", "tanh"])]
    fn cosh(x: f64) -> f64 {
        x.cosh()
    }

    /// the hyperbolic tangent, sinh(x) / cosh(x)
    #[builtin(examples = ["tanh(1)"], see = ["sinh", "cosh"])]
    fn tanh(x: f64) -> f64 {
        x.tanh()
    }
    /// the cosecant of an angle, 1 / sin(x)
    #[builtin(examples = ["csc(30 deg)"], see = ["sin", "asin"])]
    fn csc(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.sin().recip())
    }

    /// the secant of an angle, 1 / cos(x), not to be confused with `sec` for seconds
    #[builtin(examples = ["secant(60 deg)"], see = ["cos", "acos"])]
    fn secant(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.cos().recip())
    }

    /// the cotangent of an angle, 1 / tan(x)
    #[builtin(examples = ["cot(45 deg)"], see = ["tan", "atan"])]
    fn cot(x: Or<f64, Qty>) -> Result<f64, Fail> {
        Ok(radians(x)?.tan().recip())
    }

    /// the angle whose cosecant is x
    #[builtin(examples = ["acsc(2) to deg"], see = ["csc"])]
    fn acsc(x: f64) -> As<Qty> {
        angle(x.recip().asin())
    }

    /// the angle whose secant is x
    #[builtin(examples = ["asec(2) to deg"], see = ["secant"])]
    fn asec(x: f64) -> As<Qty> {
        angle(x.recip().acos())
    }

    /// the angle whose cotangent is x
    #[builtin(examples = ["acot(1) to deg"], see = ["cot"])]
    fn acot(x: f64) -> As<Qty> {
        angle(x.recip().atan())
    }

    /// the inverse hyperbolic sine
    #[builtin(examples = ["asinh(1)"], see = ["sinh"])]
    fn asinh(x: f64) -> f64 {
        x.asinh()
    }

    /// the inverse hyperbolic cosine, for x of 1 or more
    #[builtin(examples = ["acosh(2)"], see = ["cosh"])]
    fn acosh(x: f64) -> f64 {
        x.acosh()
    }

    /// the inverse hyperbolic tangent, for x between -1 and 1
    #[builtin(examples = ["atanh(0.5)"], see = ["tanh"])]
    fn atanh(x: f64) -> f64 {
        x.atanh()
    }

    /// the hyperbolic secant, 1 / cosh(x)
    #[builtin(examples = ["sech(1)"], see = ["cosh"])]
    fn sech(x: f64) -> f64 {
        x.cosh().recip()
    }

    /// the hyperbolic cosecant, 1 / sinh(x)
    #[builtin(examples = ["csch(1)"], see = ["sinh"])]
    fn csch(x: f64) -> f64 {
        x.sinh().recip()
    }

    /// the hyperbolic cotangent, 1 / tanh(x)
    #[builtin(examples = ["coth(1)"], see = ["tanh"])]
    fn coth(x: f64) -> f64 {
        x.tanh().recip()
    }

    /// the normalized sinc function, sin(pi x) / (pi x), which is 1 at 0
    #[builtin(examples = ["sinc(0.5)"], see = ["sin"])]
    fn sinc(x: f64) -> f64 {
        if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) }
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn trig() {
        assert_eq!(show("sin(30 deg)"), "0.5");
        assert_eq!(show("asin(1) to deg"), "90 deg");
        assert_eq!(show("atan2(1, -1) to deg"), "135 deg");
        assert_eq!(show("round(csc(30 deg), 6)"), "2");
        assert_eq!(show("round(acot(1) to deg, 6)"), "45 deg");
        assert_eq!(show("sinc(0)"), "1");
    }
}
