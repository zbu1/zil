//! Special functions: the factorial extended to any number, the area under the bell curve, and the step and pulse shapes used in signals.

use crate::modules::Module;
use std::f64::consts::PI;

pub const MODULE: Module = Module {
    name: "special",
    about: "factorials of any number, bell curve areas and logistic curves, plus step, ramp and pulse shapes",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Functions that come up in statistics, probability and signal processing. They include the factorial extended to any number, \
and the area under the bell curve."),
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

pub(crate) fn lgamma(x: f64) -> f64 {
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

pub(crate) fn erfc(x: f64) -> f64 {
    if x >= 2.5 { erfc_tail(x) } else { 1.0 - erf(x) }
}

fn digamma(x: f64) -> f64 {
    if x <= 0.0 && x == x.floor() {
        return f64::NAN;
    }
    if x < 0.0 {
        return digamma(1.0 - x) - PI / (PI * x).tan();
    }
    let (mut x, mut r) = (x, 0.0);
    while x < 10.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + x.ln() - 0.5 / x - f * (1.0 / 12.0 - f * (1.0 / 120.0 - f * (1.0 / 252.0 - f * (1.0 / 240.0 - f * (1.0 / 132.0)))))
}

/// The Riemann zeta function by Euler-Maclaurin summation, reflected for s below 1/2.
fn zeta(s: f64) -> f64 {
    if s == 1.0 {
        return f64::INFINITY;
    }
    if s < 0.5 {
        return 2f64.powf(s) * PI.powf(s - 1.0) * (PI * s / 2.0).sin() * gamma(1.0 - s) * zeta(1.0 - s);
    }
    const B: [f64; 8] = [1.0 / 6.0, -1.0 / 30.0, 1.0 / 42.0, -1.0 / 30.0, 5.0 / 66.0, -691.0 / 2730.0, 7.0 / 6.0, -3617.0 / 510.0];
    let n: f64 = 20.0;
    let mut sum: f64 = (1..20).map(|k| (k as f64).powf(-s)).sum();
    sum += n.powf(1.0 - s) / (s - 1.0) + 0.5 * n.powf(-s);
    let (mut rising, mut fact) = (s, 2.0);
    for (k, b) in B.iter().enumerate() {
        let k = k as f64 + 1.0;
        sum += b / fact * rising * n.powf(-s - 2.0 * k + 1.0);
        rising *= (s + 2.0 * k - 1.0) * (s + 2.0 * k);
        fact *= (2.0 * k + 1.0) * (2.0 * k + 2.0);
    }
    sum
}

/// The regularized lower incomplete gamma function P(a, x).
pub(crate) fn gamma_inc(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let pre = (a * x.ln() - x - lgamma(a)).exp();
    if x < a + 1.0 {
        let (mut term, mut sum, mut n) = (1.0 / a, 1.0 / a, a);
        for _ in 0..1000 {
            n += 1.0;
            term *= x / n;
            sum += term;
            if term < sum * 1e-16 {
                break;
            }
        }
        pre * sum
    } else {
        let tiny = 1e-300;
        let (mut b, mut c) = (x + 1.0 - a, 1.0 / tiny);
        let (mut d, mut h) = (1.0 / b, 1.0 / b);
        for i in 1..1000 {
            let an = -(i as f64) * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < tiny {
                d = tiny;
            }
            c = b + an / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        1.0 - pre * h
    }
}

/// The regularized incomplete beta function I_x(a, b).
pub(crate) fn beta_inc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - beta_inc(1.0 - x, b, a);
    }
    let front = (lgamma(a + b) - lgamma(a) - lgamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp() / a;
    let tiny = 1e-300;
    let (mut c, mut d) = (1.0, 1.0 - (a + b) * x / (a + 1.0));
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..1000 {
        let m = m as f64;
        for k in 0..2 {
            let num = if k == 0 {
                m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m))
            } else {
                -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0))
            };
            d = 1.0 + num * d;
            if d.abs() < tiny {
                d = tiny;
            }
            c = 1.0 + num / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            h *= d * c;
        }
        if (d * c - 1.0).abs() < 1e-16 {
            break;
        }
    }
    front * h
}

pub(crate) fn erf_inv(y: f64) -> f64 {
    if y.abs() >= 1.0 {
        return if y.abs() == 1.0 { y * f64::INFINITY } else { f64::NAN };
    }
    let l = (1.0 - y * y).ln();
    let t = 2.0 / (PI * 0.147) + l / 2.0;
    let mut x = y.signum() * ((t * t - l / 0.147).sqrt() - t).sqrt();
    for _ in 0..4 {
        let e = erf(x) - y;
        let d = 2.0 / PI.sqrt() * (-x * x).exp();
        x -= e / (d + x * e);
    }
    x
}

#[zil_macros::builtins]
mod fns {
    use super::{
        beta_inc as beta_inc_, digamma as digamma_, erf as erf_, erf_inv as erf_inv_, erfc as erfc_, gamma as gamma_, gamma_inc as gamma_inc_,
        lgamma as lgamma_, zeta as zeta_,
    };

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

    /// the digamma function, the slope of lgamma: the derivative of ln(gamma(x))
    #[builtin(examples = ["digamma(1)", "digamma(0.5)"], see = ["gamma", "lgamma"])]
    fn digamma(x: f64) -> f64 {
        digamma_(x)
    }

    /// the Riemann zeta function, the sum of 1 / n^s over every whole number n from 1
    #[builtin(examples = ["zeta(2)", "zeta(0.5)"], see = ["gamma"])]
    fn zeta(s: f64) -> f64 {
        zeta_(s)
    }

    /// the inverse of the error function: the x where erf(x) is y, for y between -1 and 1
    #[builtin(examples = ["erf_inv(0.5)", "erf(erf_inv(0.3))"], see = ["erf", "probit"])]
    fn erf_inv(y: f64) -> f64 {
        erf_inv_(y)
    }

    /// the normal quantile: the z where a standard normal variable falls below z with chance p
    #[builtin(examples = ["probit(0.975)", "probit(0.5)"], see = ["erf_inv"])]
    fn probit(p: f64) -> f64 {
        std::f64::consts::SQRT_2 * erf_inv_(2.0 * p - 1.0)
    }

    /// the regularized lower incomplete gamma function: the share of gamma(a) accumulated up to x, from 0 to 1
    #[builtin(examples = ["gamma_inc(1, 2)", "gamma_inc(3, 3)"], see = ["gamma", "beta_inc"])]
    fn gamma_inc(a: f64, x: f64) -> f64 {
        gamma_inc_(a, x)
    }

    /// the regularized incomplete beta function: the share of beta(a, b) accumulated up to x, for x from 0 to 1
    #[builtin(examples = ["beta_inc(0.5, 2, 2)", "beta_inc(0.25, 1, 3)"], see = ["beta", "gamma_inc"])]
    fn beta_inc(x: f64, a: f64, b: f64) -> f64 {
        beta_inc_(x, a, b)
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
        assert_eq!(show("[round(digamma(1), 6), round(zeta(2), 8), round(zeta(-1), 8), round(zeta(0.5), 6)]"), "[-0.577216, 1.64493, -0.0833333, -1.46036]");
        assert_eq!(
            show("[round(erf_inv(0.5), 8), round(probit(0.975), 6), round(gamma_inc(1, 2), 8), round(beta_inc(0.5, 2, 2), 8)]"),
            "[0.476936, 1.95996, 0.864665, 0.5]"
        );
        assert_eq!(show("[heaviside(-1), heaviside(0), ramp(-2), ramp(3), rect(0.2), rect(1), tri(0.25), tri(2)]"), "[0, 0.5, 0, 3, 1, 0, 0.75, 0]");
    }
}
