//! Trigonometry: angles in radians or any angle unit.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc, units};
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "trig",
    about: "sine, cosine, tangent and their inverses, in radians or degrees",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Sine, cosine, tangent and their inverses.\n\n\
`sin`, `cos` and `tan` read a plain number as radians, where a full turn is 2 pi. To use degrees, give an angle unit: \
`sin(30 deg)`. `asin`, `acos`, `atan` and `atan2` return an angle, which you can convert with `to deg`."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("trig", &[
            ("angles can be in degrees", "sin(30 deg)"),
            ("radians back to degrees", "atan2(1, 1) to deg"),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("sin", "sin(x: num|quantity) -> float", "the sine of an angle, in radians or an angle unit", &["sin(30 deg)", "sin(pi / 2)"], &["cos", "tan", "asin"]),
    doc("cos", "cos(x: num|quantity) -> float", "the cosine of an angle, in radians or an angle unit", &["cos(60 deg)"], &["sin", "tan", "acos"]),
    doc("tan", "tan(x: num|quantity) -> float", "the tangent of an angle, in radians or an angle unit", &["tan(45 deg)"], &["sin", "cos", "atan"]),
    doc("asin", "asin(x: num) -> quantity", "the angle whose sine is x (arcsine)", &["asin(1) to deg"], &["sin"]),
    doc("acos", "acos(x: num) -> quantity", "the angle whose cosine is x (arccosine)", &["acos(0) to deg"], &["cos"]),
    doc("atan", "atan(x: num) -> quantity", "the angle whose tangent is x (arctangent)", &["atan(1) to deg"], &["tan", "atan2"]),
    doc("atan2", "atan2(y: num|quantity, x: num|quantity) -> quantity", "the angle from the positive x axis to the point (x, y), from -180° to 180°", &["atan2(1, -1) to deg"], &["atan", "hypot"])
        .detail("Unlike atan(y / x), it knows which quarter the point is in. Note y comes first"),
    doc("sinh", "sinh(x: num) -> float", "the hyperbolic sine, (eˣ - e⁻ˣ) / 2", &["sinh(1)"], &["cosh", "tanh"]),
    doc("cosh", "cosh(x: num) -> float", "the hyperbolic cosine, (eˣ + e⁻ˣ) / 2", &["cosh(1)"], &["sinh", "tanh"]),
    doc("tanh", "tanh(x: num) -> float", "the hyperbolic tangent, sinh(x) / cosh(x)", &["tanh(1)"], &["sinh", "cosh"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("sinh" | "cosh" | "tanh", [v]) if num(v).is_some() => {
            let x = num(v).unwrap();
            Float(match name {
                "sinh" => x.sinh(),
                "cosh" => x.cosh(),
                _ => x.tanh(),
            })
        }
        ("sin" | "cos" | "tan", [v]) => {
            let rad = match v {
                Qty(x, u) if u.dim() == units::unit("rad").unwrap().dim() => u.to_si(*x),
                _ => num(v).ok_or(Fail::BadArgs)?,
            };
            Float(match name {
                "sin" => rad.sin(),
                "cos" => rad.cos(),
                _ => rad.tan(),
            })
        }
        ("asin" | "acos" | "atan", [v]) if num(v).is_some() => {
            let x = num(v).unwrap();
            let rad = match name {
                "asin" => x.asin(),
                "acos" => x.acos(),
                _ => x.atan(),
            };
            Qty(rad, units::unit("rad").unwrap())
        }
        ("atan2", [y, x]) => {
            let (y, x) = match (y, x) {
                (Qty(y, u), Qty(x, w)) if u.dim() == w.dim() => (u.to_si(*y), w.to_si(*x)),
                _ => (num(y).ok_or(Fail::BadArgs)?, num(x).ok_or(Fail::BadArgs)?),
            };
            Qty(y.atan2(x), units::unit("rad").unwrap())
        }
        _ => return Err(Fail::BadArgs),
    })
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn trig() {
        assert_eq!(show("sin(30 deg)"), "0.5");
        assert_eq!(show("asin(1) to deg"), "90 deg");
        assert_eq!(show("atan2(1, -1) to deg"), "135 deg");
    }
}
