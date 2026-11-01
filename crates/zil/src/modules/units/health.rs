//! Health: body mass index.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "health",
    about: "body measurements like body mass index",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Quick health calculations from body measurements. They are rules of thumb, not medical advice."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("health", &[
            ("body mass index in metric", "bmi(70 kg, 1.75 m)"),
            ("and in pounds and inches", "bmi(154 lb, 5 ft 9 in)"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use crate::modules::Fail;
    use crate::modules::units::expect_in;
    use crate::signature::Qty;

    /// body mass index, mass divided by the square of height, in kg/m²
    ///
    /// Under 18.5 is usually called underweight, 25 and over overweight, and 30 and over obese. It ignores build, so it suits populations better than individuals
    #[builtin(examples = ["bmi(70 kg, 1.75 m)", "bmi(154 lb, 5 ft 9 in)"])]
    fn bmi(mass: Qty, height: Qty) -> Result<f64, Fail> {
        let (m, mu) = mass.get();
        let (h, hu) = height.get();
        let kg = expect_in(m, mu, "kg").map_err(|e| Fail::Arg(0, e))?;
        let metres = expect_in(h, hu, "m").map_err(|e| Fail::Arg(1, e))?;
        if metres <= 0.0 {
            return Err(Fail::Arg(1, "height must be positive".into()));
        }
        Ok(kg / (metres * metres))
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn health() {
        assert_eq!(show("bmi(70 kg, 1.75 m).round(1)"), "22.9");
        assert_eq!(show("bmi(154 lb, 5 ft 9 in).round(1)"), "22.7");
        assert!(try_eval("bmi(70 m, 1.75 m)").is_err());
    }
}
