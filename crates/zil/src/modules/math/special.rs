//! Special functions: gamma, erf, and the step and pulse shapes used in signals.

use crate::modules::Module;
use std::f64::consts::PI;

pub const MODULE: Module = Module {
    name: "special",
    about: "gamma, error function and logistic curves, plus step, ramp and pulse shapes",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Functions that come up in statistics, probability and signal processing. `gamma` extends the factorial to any number, \
and `erf` is the area under the bell curve."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("special", &[
            ("the factorial of 4.5", "gamma(5.5)"),
            ("the share of a normal distribution within 1.96 standard deviations", "erf(1.96 / sqrt(2))"),
            ("an S-shaped curve from 0 to 1", "sigmoid(2)"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

fn lanczos(x: f64) -> f64 {
    let x = x - 1.0;
    let t = x + 7.5;
    let a = LANCZOS.iter().enumerate().skip(1).fold(LANCZOS[0], |a, (i, c)| a + c / (x + i as f64));
    (2.0 * PI).sqrt().ln() + (x + 0.5) * t.ln() - t + a.ln()
}

fn lgamma(x: f64) -> f64 {
    if x < 0.5 { (PI / (PI * x).sin().abs()).ln() - lgamma(1.0 - x) } else { lanczos(x) }
}

fn gamma(x: f64) -> f64 {
    if x <= 0.0 && x == x.floor() {
        f64::NAN
    } else if x == x.floor() && x < 171.0 {
        (1..x as u32).fold(1.0, |a, k| a * k as f64)
    } else if x < 0.5 {
        PI / ((PI * x).sin() * gamma(1.0 - x))
    } else {
        lanczos(x).exp()
    }
}

/// erfc by its continued fraction, for x of 2.5 or more.
fn erfc_tail(x: f64) -> f64 {
    let mut f = 0.0;
    for k in (1..=60).rev() {
        f = (k as f64 / 2.0) / (x + f);
    }
    (-x * x).exp() / (PI.sqrt() * (x + f))
}

fn erf(x: f64) -> f64 {
    if x.abs() >= 2.5 {
        return x.signum() * (1.0 - erfc_tail(x.abs()));
    }
    let (mut term, mut sum) = (x, x);
    for n in 1..100 {
        term *= -x * x / n as f64;
        sum += term / (2 * n + 1) as f64;
    }
    2.0 / PI.sqrt() * sum
}

fn erfc(x: f64) -> f64 {
    if x >= 2.5 { erfc_tail(x) } else { 1.0 - erf(x) }
}

#[zil_macros::builtins]
mod fns {
    use super::{erf as erf_, erfc as erfc_, gamma as gamma_, lgamma as lgamma_};

    /// the gamma function, which extends the factorial: gamma(n + 1) is n!, and it works between whole numbers too
    #[builtin(examples = ["gamma(5)", "gamma(0.5) ** 2"], see = ["lgamma", "beta", "factorial"])]
    fn gamma(x: f64) -> f64 {
        gamma_(x)
    }

    /// the natural log of the gamma function, which stays finite where gamma itself is too large
    #[builtin(examples = ["lgamma(1000)"], see = ["gamma"])]
    fn lgamma(x: f64) -> f64 {
        lgamma_(x)
    }

    /// the beta function, gamma(a) × gamma(b) / gamma(a + b)
    #[builtin(examples = ["beta(2, 3)"], see = ["gamma"])]
    fn beta(a: f64, b: f64) -> f64 {
        (lgamma_(a) + lgamma_(b) - lgamma_(a + b)).exp() * (gamma_(a) * gamma_(b) * gamma_(a + b)).signum()
    }

    /// the error function: the chance a normal variable lands within x√2 standard deviations of its mean
    #[builtin(examples = ["erf(1)", "erf(1 / sqrt(2))"], see = ["erfc"])]
    fn erf(x: f64) -> f64 {
        erf_(x)
    }

    /// the complementary error function, 1 - erf(x), accurate even where that would round to 0
    #[builtin(examples = ["erfc(1)", "erfc(5)"], see = ["erf"])]
    fn erfc(x: f64) -> f64 {
        erfc_(x)
    }

    /// the logistic curve 1 / (1 + e^-x): an S shape from 0 to 1
    #[builtin(examples = ["sigmoid(0)", "sigmoid(2)"], see = ["logit"])]
    fn sigmoid(x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }

    /// the inverse of sigmoid, ln(p / (1 - p)), for p between 0 and 1
    #[builtin(examples = ["logit(0.5)", "logit(sigmoid(2))"], see = ["sigmoid"])]
    fn logit(p: f64) -> f64 {
        (p / (1.0 - p)).ln()
    }

    /// the step function: 0 below zero, 1 above, and 1/2 at zero
    #[builtin(examples = ["heaviside(-2)", "heaviside(3)"], see = ["ramp", "rect"])]
    fn heaviside(x: f64) -> f64 {
        if x == 0.0 {
            0.5
        } else if x > 0.0 {
            1.0
        } else {
            0.0
        }
    }

    /// the ramp function: 0 below zero, and x above
    #[builtin(examples = ["ramp(-2)", "ramp(3)"], see = ["heaviside"])]
    fn ramp(x: f64) -> f64 {
        x.max(0.0)
    }

    /// the rectangle pulse: 1 for x within 1/2 of zero, 1/2 at the edges, 0 beyond
    #[builtin(examples = ["rect(0.2)", "rect(1)"], see = ["tri", "heaviside"])]
    fn rect(x: f64) -> f64 {
        let a = x.abs();
        if a < 0.5 {
            1.0
        } else if a == 0.5 {
            0.5
        } else {
            0.0
        }
    }

    /// the triangle pulse: 1 at zero, falling in straight lines to 0 at 1 and -1
    #[builtin(examples = ["tri(0.25)", "tri(2)"], see = ["rect"])]
    fn tri(x: f64) -> f64 {
        (1.0 - x.abs()).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn special() {
        assert_eq!(show("[gamma(5), gamma(0.5) ** 2 to int, round(gamma(-0.5), 5), round(lgamma(10), 6), gamma(0)]"), "[24, 3, -3.54491, 12.8018, undefined]");
        assert_eq!(show("[round(erf(1), 10), round(erf(-3), 10), round(erfc(1), 10), erfc(0), erf(0)]"), "[0.842701, -0.999978, 0.157299, 1, 0]");
        assert_eq!(show("[beta(2, 3), sigmoid(0), logit(0.5), round(logit(sigmoid(2)), 9)]"), "[0.0833333, 0.5, 0, 2]");
        assert_eq!(show("[heaviside(-1), heaviside(0), ramp(-2), ramp(3), rect(0.2), rect(1), tri(0.25), tri(2)]"), "[0, 0.5, 0, 3, 1, 0, 0.75, 0]");
    }
}
