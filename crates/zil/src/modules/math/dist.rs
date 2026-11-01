//! Probability distributions: densities, cumulative probabilities, quantiles, random draws and t-tests.

use super::special::{beta_inc, erf_inv, erfc, gamma_inc, lgamma};
use crate::modules::{Fail, Module};
use std::f64::consts::{LN_2, PI, SQRT_2};

pub const MODULE: Module = Module {
    name: "dist",
    about: "probability distributions: how likely values are, chances of at most a value, the value for a chance, random draws and t-tests",
    #[rustfmt::skip]
    intro: &[
        ("", "\
How probable values are for the common distributions, and how to test whether two sets of measurements really differ."),
        ("writing", "\
Each distribution has the same set of functions, named after it: one ending in `_pdf` for how dense the probability is at a \
value (for counts like the binomial, `_pmf` is the chance of exactly that count), one ending in `_cdf` for the chance of a \
value at most that big, and one ending in `_inv` that goes the other way, from a chance to the value. Parameters come after \
the value, and the ones with a usual setting can be left out."),
        ("good to know", "\
The `_inv` functions are quantiles, so the value below which 95% of a normal distribution lies is the inverse at 0.95. The \
random draws take the same parameters as the distribution, and repeat after `seed`."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("distributions", &[
            ("the share of a bell curve within 1.96 deviations", "normal_cdf(1.96) - normal_cdf(-1.96)"),
            ("the 95% cutoff for a t distribution with 10 degrees of freedom", "t_inv(0.975, 10)"),
            ("heads in 10 fair coin tosses, at most 3", "binom_cdf(3, 10, 0.5)"),
            ("do these two groups differ?", "t_test([5.1, 4.9, 5.6, 5.8, 5.0], [6.1, 6.4, 5.9, 6.8, 6.2]).p"),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("normal", &["normal_pdf", "normal_cdf", "normal_inv", "rand_normal"]),
        ("t", &["t_pdf", "t_cdf", "t_inv"]),
        ("chi2", &["chi2_pdf", "chi2_cdf", "chi2_inv"]),
        ("f", &["f_pdf", "f_cdf", "f_inv"]),
        ("exp", &["exp_pdf", "exp_cdf", "exp_inv", "rand_exp"]),
        ("gamma", &["gamma_pdf", "gamma_cdf", "gamma_inv"]),
        ("beta", &["beta_pdf", "beta_cdf", "beta_inv"]),
        ("weibull", &["weibull_pdf", "weibull_cdf", "weibull_inv"]),
        ("cauchy", &["cauchy_pdf", "cauchy_cdf", "cauchy_inv"]),
        ("logistic", &["logistic_pdf", "logistic_cdf", "logistic_inv"]),
        ("pareto", &["pareto_pdf", "pareto_cdf", "pareto_inv"]),
        ("rayleigh", &["rayleigh_pdf", "rayleigh_cdf", "rayleigh_inv"]),
        ("binomial", &["binom_pmf", "binom_cdf", "binom_inv"]),
        ("poisson", &["poisson_pmf", "poisson_cdf", "poisson_inv", "rand_poisson"]),
        ("tests", &["t_test", "paired_t_test"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

fn positive(v: f64, arg: usize, name: &str) -> Result<f64, Fail> {
    if v > 0.0 { Ok(v) } else { Err(Fail::Arg(arg, format!("{name} must be positive, got {v}"))) }
}

fn prob(p: f64) -> Result<f64, Fail> {
    if (0.0..=1.0).contains(&p) { Ok(p) } else { Err(Fail::Arg(0, format!("a chance is from 0 to 1, got {p}"))) }
}

/// `a * ln(x)`, with 0 for `a = 0` even at `x = 0`.
fn xlogy(a: f64, x: f64) -> f64 {
    if a == 0.0 { 0.0 } else { a * x.ln() }
}

/// The x where the increasing function `cdf` reaches `p`, between the bounds (either may be infinite), by bisection.
fn invert(cdf: impl Fn(f64) -> f64, p: f64, lo: f64, hi: f64) -> f64 {
    if p == 0.0 {
        return lo;
    }
    if p == 1.0 {
        return hi;
    }
    let (mut a, mut b) = (lo, hi);
    if a.is_infinite() {
        a = -1.0;
        while cdf(a) > p && a > -1e300 {
            a *= 2.0;
        }
    }
    if b.is_infinite() {
        b = 1.0;
        while cdf(b) < p && b < 1e300 {
            b *= 2.0;
        }
    }
    for _ in 0..200 {
        let m = (a + b) / 2.0;
        if cdf(m) < p {
            a = m;
        } else {
            b = m;
        }
    }
    (a + b) / 2.0
}

fn binom_cdf(k: i64, n: i64, p: f64) -> f64 {
    if k < 0 {
        0.0
    } else if k >= n {
        1.0
    } else {
        beta_inc(1.0 - p, (n - k) as f64, (k + 1) as f64)
    }
}

fn poisson_cdf(k: i64, rate: f64) -> f64 {
    if k < 0 {
        0.0
    } else if rate == 0.0 {
        1.0
    } else {
        1.0 - gamma_inc(k as f64 + 1.0, rate)
    }
}

/// The smallest integer in `0..=hi` whose `cdf` reaches `q`.
fn smallest(cdf: impl Fn(i64) -> f64, q: f64, hi: i64) -> i64 {
    let (mut lo, mut hi) = (0, hi);
    while lo < hi {
        let m = lo + (hi - lo) / 2;
        if cdf(m) >= q { hi = m } else { lo = m + 1 }
    }
    lo
}

#[zil_macros::builtins]
mod fns {
    use super::*;
    use crate::signature::{List, Or};
    use crate::value::{Value, num};
    use indexmap::IndexMap;

    /// the probability density of a normal distribution at x, how likely values near x are
    ///
    /// Mean and sd default to 0 and 1, the standard bell curve
    #[builtin(examples = ["normal_pdf(0)"], see = ["normal_cdf", "normal_inv"])]
    fn normal_pdf(x: f64, mean: Option<f64>, sd: Option<f64>) -> Result<f64, Fail> {
        let mean = mean.unwrap_or(0.0);
        let sd = sd.unwrap_or(1.0);
        let sd = positive(sd, 2, "sd")?;
        Ok({
            let z = (x - mean) / sd;
            (-z * z / 2.0).exp() / (sd * (2.0 * PI).sqrt())
        })
    }

    /// the chance that a normal variable is at most x
    #[builtin(examples = ["normal_cdf(1.96)"], see = ["normal_pdf", "normal_inv"])]
    fn normal_cdf(x: f64, mean: Option<f64>, sd: Option<f64>) -> Result<f64, Fail> {
        let mean = mean.unwrap_or(0.0);
        let sd = sd.unwrap_or(1.0);
        let sd = positive(sd, 2, "sd")?;
        Ok(0.5 * erfc(-(x - mean) / (sd * SQRT_2)))
    }

    /// the value that a normal variable falls below with chance p, the reverse of normal_cdf
    #[builtin(examples = ["normal_inv(0.975)"], see = ["normal_pdf", "normal_cdf"])]
    fn normal_inv(p: f64, mean: Option<f64>, sd: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let mean = mean.unwrap_or(0.0);
        let sd = sd.unwrap_or(1.0);
        let sd = positive(sd, 2, "sd")?;
        Ok(mean + sd * SQRT_2 * erf_inv(2.0 * p - 1.0))
    }

    /// the probability density of a Student's t distribution at x, how likely values near x are
    ///
    /// Df is the degrees of freedom
    #[builtin(examples = ["t_pdf(0, 5)"], see = ["t_cdf", "t_inv"])]
    fn t_pdf(x: f64, df: f64) -> Result<f64, Fail> {
        let df = positive(df, 1, "df")?;
        Ok(if x.is_infinite() {
            0.0
        } else {
            (lgamma((df + 1.0) / 2.0) - lgamma(df / 2.0)).exp() / (df * PI).sqrt() * (1.0 + x * x / df).powf(-(df + 1.0) / 2.0)
        })
    }

    /// the chance that a Student's t variable is at most x
    #[builtin(examples = ["t_cdf(2.0, 10)"], see = ["t_pdf", "t_inv"])]
    fn t_cdf(x: f64, df: f64) -> Result<f64, Fail> {
        let df = positive(df, 1, "df")?;
        Ok({
            let ib = beta_inc(df / (df + x * x), df / 2.0, 0.5);
            if x > 0.0 { 1.0 - ib / 2.0 } else { ib / 2.0 }
        })
    }

    /// the value that a Student's t variable falls below with chance p, the reverse of t_cdf
    #[builtin(examples = ["t_inv(0.975, 10)"], see = ["t_pdf", "t_cdf"])]
    fn t_inv(p: f64, df: f64) -> Result<f64, Fail> {
        let p = prob(p)?;
        let df = positive(df, 1, "df")?;
        Ok(invert(
            |x| {
                let ib = beta_inc(df / (df + x * x), df / 2.0, 0.5);
                if x > 0.0 { 1.0 - ib / 2.0 } else { ib / 2.0 }
            },
            p,
            f64::NEG_INFINITY,
            f64::INFINITY,
        ))
    }

    /// the probability density of a chi-squared distribution at x, how likely values near x are
    ///
    /// Df is the degrees of freedom
    #[builtin(examples = ["chi2_pdf(2, 3)"], see = ["chi2_cdf", "chi2_inv"])]
    fn chi2_pdf(x: f64, df: f64) -> Result<f64, Fail> {
        let df = positive(df, 1, "df")?;
        Ok(if x < 0.0 { 0.0 } else { (xlogy(df / 2.0 - 1.0, x) - x / 2.0 - df / 2.0 * LN_2 - lgamma(df / 2.0)).exp() })
    }

    /// the chance that a chi-squared variable is at most x
    #[builtin(examples = ["chi2_cdf(3.84, 1)"], see = ["chi2_pdf", "chi2_inv"])]
    fn chi2_cdf(x: f64, df: f64) -> Result<f64, Fail> {
        let df = positive(df, 1, "df")?;
        Ok(gamma_inc(df / 2.0, x / 2.0))
    }

    /// the value that a chi-squared variable falls below with chance p, the reverse of chi2_cdf
    #[builtin(examples = ["chi2_inv(0.95, 1)"], see = ["chi2_pdf", "chi2_cdf"])]
    fn chi2_inv(p: f64, df: f64) -> Result<f64, Fail> {
        let p = prob(p)?;
        let df = positive(df, 1, "df")?;
        Ok(invert(|x| gamma_inc(df / 2.0, x / 2.0), p, 0.0, f64::INFINITY))
    }

    /// the probability density of an F distribution at x, how likely values near x are
    ///
    /// D1 and d2 are the two degrees of freedom
    #[builtin(examples = ["f_pdf(1, 5, 10)"], see = ["f_cdf", "f_inv"])]
    fn f_pdf(x: f64, d1: f64, d2: f64) -> Result<f64, Fail> {
        let d1 = positive(d1, 1, "d1")?;
        let d2 = positive(d2, 2, "d2")?;
        Ok(if x < 0.0 {
            0.0
        } else {
            (d1 / 2.0 * d1.ln() + d2 / 2.0 * d2.ln() + xlogy(d1 / 2.0 - 1.0, x) - (d1 + d2) / 2.0 * (d2 + d1 * x).ln() - lgamma(d1 / 2.0) - lgamma(d2 / 2.0)
                + lgamma((d1 + d2) / 2.0))
            .exp()
        })
    }

    /// the chance that an F variable is at most x
    #[builtin(examples = ["f_cdf(3, 5, 10)"], see = ["f_pdf", "f_inv"])]
    fn f_cdf(x: f64, d1: f64, d2: f64) -> Result<f64, Fail> {
        let d1 = positive(d1, 1, "d1")?;
        let d2 = positive(d2, 2, "d2")?;
        Ok(if x <= 0.0 {
            0.0
        } else if x.is_infinite() {
            1.0
        } else {
            beta_inc(d1 * x / (d1 * x + d2), d1 / 2.0, d2 / 2.0)
        })
    }

    /// the value that an F variable falls below with chance p, the reverse of f_cdf
    #[builtin(examples = ["f_inv(0.95, 5, 10)"], see = ["f_pdf", "f_cdf"])]
    fn f_inv(p: f64, d1: f64, d2: f64) -> Result<f64, Fail> {
        let p = prob(p)?;
        let d1 = positive(d1, 1, "d1")?;
        let d2 = positive(d2, 2, "d2")?;
        Ok(invert(
            |x| {
                if x <= 0.0 {
                    0.0
                } else if x.is_infinite() {
                    1.0
                } else {
                    beta_inc(d1 * x / (d1 * x + d2), d1 / 2.0, d2 / 2.0)
                }
            },
            p,
            0.0,
            f64::INFINITY,
        ))
    }

    /// the probability density of an exponential distribution at x, how likely values near x are
    ///
    /// Rate is how many events happen per unit of time, so the mean wait is 1 / rate
    #[builtin(examples = ["exp_pdf(1, 2)"], see = ["exp_cdf", "exp_inv"])]
    fn exp_pdf(x: f64, rate: f64) -> Result<f64, Fail> {
        let rate = positive(rate, 1, "rate")?;
        Ok(if x < 0.0 { 0.0 } else { rate * (-rate * x).exp() })
    }

    /// the chance that an exponential variable is at most x
    #[builtin(examples = ["exp_cdf(1, 2)"], see = ["exp_pdf", "exp_inv"])]
    fn exp_cdf(x: f64, rate: f64) -> Result<f64, Fail> {
        let rate = positive(rate, 1, "rate")?;
        Ok(if x <= 0.0 { 0.0 } else { -(-rate * x).exp_m1() })
    }

    /// the value that an exponential variable falls below with chance p, the reverse of exp_cdf
    #[builtin(examples = ["exp_inv(0.5, 2)"], see = ["exp_pdf", "exp_cdf"])]
    fn exp_inv(p: f64, rate: f64) -> Result<f64, Fail> {
        let p = prob(p)?;
        let rate = positive(rate, 1, "rate")?;
        Ok(-(-p).ln_1p() / rate)
    }

    /// the probability density of a gamma distribution at x, how likely values near x are
    ///
    /// Rate defaults to 1
    #[builtin(examples = ["gamma_pdf(2, 2)"], see = ["gamma_cdf", "gamma_inv"])]
    fn gamma_pdf(x: f64, shape: f64, rate: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let rate = rate.unwrap_or(1.0);
        let rate = positive(rate, 2, "rate")?;
        Ok(if x < 0.0 { 0.0 } else { (shape * rate.ln() + xlogy(shape - 1.0, x) - rate * x - lgamma(shape)).exp() })
    }

    /// the chance that a gamma variable is at most x
    #[builtin(examples = ["gamma_cdf(2, 3, 2)"], see = ["gamma_pdf", "gamma_inv"])]
    fn gamma_cdf(x: f64, shape: f64, rate: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let rate = rate.unwrap_or(1.0);
        let rate = positive(rate, 2, "rate")?;
        Ok(gamma_inc(shape, rate * x))
    }

    /// the value that a gamma variable falls below with chance p, the reverse of gamma_cdf
    #[builtin(examples = ["gamma_inv(0.5, 3)"], see = ["gamma_pdf", "gamma_cdf"])]
    fn gamma_inv(p: f64, shape: f64, rate: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let shape = positive(shape, 1, "shape")?;
        let rate = rate.unwrap_or(1.0);
        let rate = positive(rate, 2, "rate")?;
        Ok(invert(|x| gamma_inc(shape, rate * x), p, 0.0, f64::INFINITY))
    }

    /// the probability density of a beta distribution at x, how likely values near x are
    ///
    /// A and b are the two shape numbers, and x runs from 0 to 1
    #[builtin(examples = ["beta_pdf(0.5, 2, 2)"], see = ["beta_cdf", "beta_inv"])]
    fn beta_pdf(x: f64, a: f64, b: f64) -> Result<f64, Fail> {
        let a = positive(a, 1, "a")?;
        let b = positive(b, 2, "b")?;
        Ok(if !(0.0..=1.0).contains(&x) { 0.0 } else { (xlogy(a - 1.0, x) + xlogy(b - 1.0, 1.0 - x) - lgamma(a) - lgamma(b) + lgamma(a + b)).exp() })
    }

    /// the chance that a beta variable is at most x
    #[builtin(examples = ["beta_cdf(0.25, 2, 5)"], see = ["beta_pdf", "beta_inv"])]
    fn beta_cdf(x: f64, a: f64, b: f64) -> Result<f64, Fail> {
        let a = positive(a, 1, "a")?;
        let b = positive(b, 2, "b")?;
        Ok(beta_inc(x, a, b))
    }

    /// the value that a beta variable falls below with chance p, the reverse of beta_cdf
    #[builtin(examples = ["beta_inv(0.5, 2, 5)"], see = ["beta_pdf", "beta_cdf"])]
    fn beta_inv(p: f64, a: f64, b: f64) -> Result<f64, Fail> {
        let p = prob(p)?;
        let a = positive(a, 1, "a")?;
        let b = positive(b, 2, "b")?;
        Ok(invert(|x| beta_inc(x, a, b), p, 0.0, 1.0))
    }

    /// the probability density of a Weibull distribution at x, how likely values near x are
    ///
    /// Scale defaults to 1
    #[builtin(examples = ["weibull_pdf(1, 1.5)"], see = ["weibull_cdf", "weibull_inv"])]
    fn weibull_pdf(x: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(if x < 0.0 { 0.0 } else { shape / scale * (x / scale).powf(shape - 1.0) * (-(x / scale).powf(shape)).exp() })
    }

    /// the chance that a Weibull variable is at most x
    #[builtin(examples = ["weibull_cdf(1, 1.5, 2)"], see = ["weibull_pdf", "weibull_inv"])]
    fn weibull_cdf(x: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(if x <= 0.0 { 0.0 } else { -(-(x / scale).powf(shape)).exp_m1() })
    }

    /// the value that a Weibull variable falls below with chance p, the reverse of weibull_cdf
    #[builtin(examples = ["weibull_inv(0.5, 1.5, 2)"], see = ["weibull_pdf", "weibull_cdf"])]
    fn weibull_inv(p: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(scale * (-(-p).ln_1p()).powf(1.0 / shape))
    }

    /// the probability density of a Cauchy distribution at x, how likely values near x are
    ///
    /// Loc and scale default to 0 and 1
    #[builtin(examples = ["cauchy_pdf(0)"], see = ["cauchy_cdf", "cauchy_inv"])]
    fn cauchy_pdf(x: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(1.0 / (PI * scale * (1.0 + ((x - loc) / scale).powi(2))))
    }

    /// the chance that a Cauchy variable is at most x
    #[builtin(examples = ["cauchy_cdf(1)"], see = ["cauchy_pdf", "cauchy_inv"])]
    fn cauchy_cdf(x: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(0.5 + ((x - loc) / scale).atan() / PI)
    }

    /// the value that a Cauchy variable falls below with chance p, the reverse of cauchy_cdf
    #[builtin(examples = ["cauchy_inv(0.75)"], see = ["cauchy_pdf", "cauchy_cdf"])]
    fn cauchy_inv(p: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(loc + scale * (PI * (p - 0.5)).tan())
    }

    /// the probability density of a logistic distribution at x, how likely values near x are
    ///
    /// Loc and scale default to 0 and 1
    #[builtin(examples = ["logistic_pdf(0)"], see = ["logistic_cdf", "logistic_inv"])]
    fn logistic_pdf(x: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(0.25 / (scale * ((x - loc) / (2.0 * scale)).cosh().powi(2)))
    }

    /// the chance that a logistic variable is at most x
    #[builtin(examples = ["logistic_cdf(2)"], see = ["logistic_pdf", "logistic_inv"])]
    fn logistic_cdf(x: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(1.0 / (1.0 + (-(x - loc) / scale).exp()))
    }

    /// the value that a logistic variable falls below with chance p, the reverse of logistic_cdf
    #[builtin(examples = ["logistic_inv(0.9)"], see = ["logistic_pdf", "logistic_cdf"])]
    fn logistic_inv(p: f64, loc: Option<f64>, scale: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let loc = loc.unwrap_or(0.0);
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(loc + scale * (p / (1.0 - p)).ln())
    }

    /// the probability density of a Pareto distribution at x, how likely values near x are
    ///
    /// Scale is the smallest possible value and defaults to 1
    #[builtin(examples = ["pareto_pdf(2, 3)"], see = ["pareto_cdf", "pareto_inv"])]
    fn pareto_pdf(x: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(if x < scale { 0.0 } else { shape * scale.powf(shape) / x.powf(shape + 1.0) })
    }

    /// the chance that a Pareto variable is at most x
    #[builtin(examples = ["pareto_cdf(2, 3)"], see = ["pareto_pdf", "pareto_inv"])]
    fn pareto_cdf(x: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(if x < scale { 0.0 } else { 1.0 - (scale / x).powf(shape) })
    }

    /// the value that a Pareto variable falls below with chance p, the reverse of pareto_cdf
    #[builtin(examples = ["pareto_inv(0.5, 3)"], see = ["pareto_pdf", "pareto_cdf"])]
    fn pareto_inv(p: f64, shape: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let shape = positive(shape, 1, "shape")?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 2, "scale")?;
        Ok(scale / (1.0 - p).powf(1.0 / shape))
    }

    /// the probability density of a Rayleigh distribution at x, how likely values near x are
    ///
    /// Scale defaults to 1
    #[builtin(examples = ["rayleigh_pdf(1)"], see = ["rayleigh_cdf", "rayleigh_inv"])]
    fn rayleigh_pdf(x: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 1, "scale")?;
        Ok(if x < 0.0 { 0.0 } else { x / (scale * scale) * (-x * x / (2.0 * scale * scale)).exp() })
    }

    /// the chance that a Rayleigh variable is at most x
    #[builtin(examples = ["rayleigh_cdf(1, 2)"], see = ["rayleigh_pdf", "rayleigh_inv"])]
    fn rayleigh_cdf(x: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 1, "scale")?;
        Ok(if x <= 0.0 { 0.0 } else { -(-x * x / (2.0 * scale * scale)).exp_m1() })
    }

    /// the value that a Rayleigh variable falls below with chance p, the reverse of rayleigh_cdf
    #[builtin(examples = ["rayleigh_inv(0.5, 2)"], see = ["rayleigh_pdf", "rayleigh_cdf"])]
    fn rayleigh_inv(p: f64, scale: Option<f64>) -> Result<f64, Fail> {
        let p = prob(p)?;
        let scale = scale.unwrap_or(1.0);
        let scale = positive(scale, 1, "scale")?;
        Ok(scale * (-2.0 * (-p).ln_1p()).sqrt())
    }

    fn binom_args(n: i64, p: f64) -> Result<(), Fail> {
        if n < 0 {
            return Err(Fail::Arg(1, format!("n must be at least 0, got {n}")));
        }
        prob(p).map(drop).map_err(|_| Fail::Arg(2, format!("p must be from 0 to 1, got {p}")))
    }

    /// the chance of exactly k successes in n tries that each succeed with chance p
    #[builtin(examples = ["binom_pmf(5, 10, 0.5)"], see = ["binom_cdf", "binom_inv"])]
    fn binom_pmf(k: i64, n: i64, p: f64) -> Result<f64, Fail> {
        binom_args(n, p)?;
        Ok(if k < 0 || k > n {
            0.0
        } else if p == 0.0 || p == 1.0 {
            (k == if p == 0.0 { 0 } else { n }) as i64 as f64
        } else {
            let (kf, nf) = (k as f64, n as f64);
            (lgamma(nf + 1.0) - lgamma(kf + 1.0) - lgamma(nf - kf + 1.0) + kf * p.ln() + (nf - kf) * (-p).ln_1p()).exp()
        })
    }

    /// the chance of at most k successes in n tries that each succeed with chance p
    #[builtin(examples = ["binom_cdf(3, 10, 0.5)"], see = ["binom_pmf", "binom_inv"])]
    fn binom_cdf(k: i64, n: i64, p: f64) -> Result<f64, Fail> {
        binom_args(n, p)?;
        Ok(super::binom_cdf(k, n, p))
    }

    /// the fewest successes in n tries with chance p each that are at least as likely as q to be reached, the reverse of binom_cdf
    #[builtin(examples = ["binom_inv(0.5, 10, 0.5)"], see = ["binom_pmf", "binom_cdf"])]
    fn binom_inv(q: f64, n: i64, p: f64) -> Result<i64, Fail> {
        let q = prob(q)?;
        binom_args(n, p)?;
        Ok(smallest(|k| super::binom_cdf(k, n, p), q, n))
    }

    fn poisson_arg(rate: f64) -> Result<f64, Fail> {
        if rate >= 0.0 { Ok(rate) } else { Err(Fail::Arg(1, format!("rate must be at least 0, got {rate}"))) }
    }

    /// the chance of exactly k events when rate of them are expected on average
    #[builtin(examples = ["poisson_pmf(2, 3)"], see = ["poisson_cdf", "poisson_inv"])]
    fn poisson_pmf(k: i64, rate: f64) -> Result<f64, Fail> {
        let rate = poisson_arg(rate)?;
        Ok(if k < 0 {
            0.0
        } else if rate == 0.0 {
            (k == 0) as i64 as f64
        } else {
            (k as f64 * rate.ln() - rate - lgamma(k as f64 + 1.0)).exp()
        })
    }

    /// the chance of at most k events when rate of them are expected on average
    #[builtin(examples = ["poisson_cdf(2, 3)"], see = ["poisson_pmf", "poisson_inv"])]
    fn poisson_cdf(k: i64, rate: f64) -> Result<f64, Fail> {
        Ok(super::poisson_cdf(k, poisson_arg(rate)?))
    }

    /// the fewest events that are at least as likely as q to be reached when rate are expected, the reverse of poisson_cdf
    #[builtin(examples = ["poisson_inv(0.5, 3)"], see = ["poisson_pmf", "poisson_cdf"])]
    fn poisson_inv(q: f64, rate: f64) -> Result<i64, Fail> {
        let (q, rate) = (prob(q)?, poisson_arg(rate)?);
        if q == 1.0 {
            return Err(Fail::Arg(0, "no count is certain, so q must be below 1".into()));
        }
        let mut hi = rate.ceil().max(1.0) as i64;
        while super::poisson_cdf(hi, rate) < q && hi < i64::MAX / 2 {
            hi *= 2;
        }
        Ok(smallest(|k| super::poisson_cdf(k, rate), q, hi))
    }

    fn draws(n: Option<i64>, mut one: impl FnMut() -> f64) -> Result<Or<f64, Vec<Value>>, Fail> {
        match n {
            None => Ok(Or::A(one())),
            Some(n) if (0..=1_000_000).contains(&n) => Ok(Or::B((0..n).map(|_| Value::Float(one())).collect())),
            Some(n) => Err(Fail::Arg(2, format!("n must be from 0 to 1000000, got {n}"))),
        }
    }

    fn unit() -> f64 {
        crate::host::rng(|r| r.f64())
    }

    /// a random number from a normal distribution with that mean and sd, or a list of n of them
    #[builtin(examples = ["rand_normal(100, 15)", "rand_normal(0, 1, 3)"], see = ["normal_pdf", "normal_cdf", "normal_inv"])]
    fn rand_normal(mean: f64, sd: f64, n: Option<i64>) -> Result<Or<f64, Vec<Value>>, Fail> {
        let sd = positive(sd, 1, "sd")?;
        draws(n, || mean + sd * (-2.0 * (1.0 - unit()).ln()).sqrt() * (2.0 * PI * unit()).cos())
    }

    /// a random wait from an exponential distribution with that rate, or a list of n of them
    #[builtin(examples = ["rand_exp(2)", "rand_exp(0.5, 3)"], see = ["exp_pdf", "exp_cdf", "exp_inv"])]
    fn rand_exp(rate: f64, n: Option<i64>) -> Result<Or<f64, Vec<Value>>, Fail> {
        let rate = positive(rate, 0, "rate")?;
        draws(n, || -(1.0 - unit()).ln() / rate)
    }

    /// a random event count from a Poisson distribution with that rate, or a list of n of them
    #[builtin(examples = ["rand_poisson(4)", "rand_poisson(2.5, 3)"], see = ["poisson_pmf", "poisson_cdf", "poisson_inv"])]
    fn rand_poisson(rate: f64, n: Option<i64>) -> Result<Or<f64, Vec<Value>>, Fail> {
        let mut left = positive(rate, 0, "rate")?;
        if left > 1e6 {
            return Err(Fail::Arg(0, format!("rate must be at most 1000000, got {rate}")));
        }
        // A sum of Poisson counts is Poisson, so big rates are cut into pieces that Knuth's method handles.
        draws(n, || {
            let mut total = 0.0;
            left = rate;
            while left > 0.0 {
                let piece = left.min(30.0);
                left -= piece;
                let (limit, mut k, mut p) = ((-piece).exp(), 0.0, 1.0);
                loop {
                    p *= unit();
                    if p <= limit {
                        break;
                    }
                    k += 1.0;
                }
                total += k;
            }
            total
        })
    }

    fn sample(v: &List, arg: usize) -> Result<Vec<f64>, Fail> {
        let xs: Vec<f64> = v.borrow().iter().map(|v| num(v).ok_or(Fail::Arg(arg, "expected a list of numbers".into()))).collect::<Result<_, _>>()?;
        if xs.len() < 2 {
            return Err(Fail::Arg(arg, "needs at least 2 values".into()));
        }
        Ok(xs)
    }

    fn mean_var(xs: &[f64]) -> (f64, f64) {
        let m = xs.iter().sum::<f64>() / xs.len() as f64;
        (m, xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64)
    }

    fn t_result(t: f64, df: f64) -> IndexMap<String, Value> {
        let tail = beta_inc(df / (df + t * t), df / 2.0, 0.5);
        IndexMap::from([("t".to_string(), Value::Float(t)), ("df".to_string(), Value::Float(df)), ("p".to_string(), Value::Float(tail))])
    }

    /// whether two groups of measurements really differ, as a map with t, df and the two-sided p
    ///
    /// Welch's test, which doesn't assume the groups spread equally. A small p, like under 0.05, means a difference this big would be rare by chance alone
    #[builtin(examples = ["t_test([5.1, 4.9, 5.6, 5.8, 5.0], [6.1, 6.4, 5.9, 6.8, 6.2])"], see = ["paired_t_test", "t_cdf"])]
    fn t_test(xs: &List, ys: &List) -> Result<IndexMap<String, Value>, Fail> {
        let (xs, ys) = (sample(xs, 0)?, sample(ys, 1)?);
        let ((mx, vx), (my, vy)) = (mean_var(&xs), mean_var(&ys));
        let (ax, ay) = (vx / xs.len() as f64, vy / ys.len() as f64);
        if ax + ay == 0.0 {
            return Err("both groups are constant, so there is nothing to test".into());
        }
        let df = (ax + ay).powi(2) / (ax * ax / (xs.len() - 1) as f64 + ay * ay / (ys.len() - 1) as f64);
        Ok(t_result((mx - my) / (ax + ay).sqrt(), df))
    }

    /// whether paired measurements, like before and after, really differ, as a map with t, df and the two-sided p
    ///
    /// The lists must be the same length, with item i of each from the same subject
    #[builtin(examples = ["paired_t_test([200, 190, 210, 205], [195, 185, 207, 198])"], see = ["t_test", "t_cdf"])]
    fn paired_t_test(xs: &List, ys: &List) -> Result<IndexMap<String, Value>, Fail> {
        let (xs, ys) = (sample(xs, 0)?, sample(ys, 1)?);
        if xs.len() != ys.len() {
            return Err(Fail::Arg(1, format!("expected {} values to pair with, got {}", xs.len(), ys.len())));
        }
        let (m, v) = mean_var(&xs.iter().zip(&ys).map(|(x, y)| x - y).collect::<Vec<_>>());
        if v == 0.0 {
            return Err("every difference is the same, so there is nothing to test".into());
        }
        Ok(t_result(m / (v / xs.len() as f64).sqrt(), xs.len() as f64 - 1.0))
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn dist() {
        assert_eq!(
            show(
                "[normal_pdf(0), normal_cdf(1.96), normal_inv(0.975), t_cdf(2, 10), t_inv(0.975, 10), chi2_cdf(3.84, 1), chi2_inv(0.95, 1), f_inv(0.95, 5, 10)].map(|x| round(x, 4))"
            ),
            "[0.3989, 0.975, 1.96, 0.9633, 2.2281, 0.95, 3.8415, 3.3258]"
        );
        assert_eq!(
            show(
                "[exp_cdf(1, 2), exp_inv(0.5, 2), gamma_cdf(2, 3, 2), beta_cdf(0.25, 2, 5), weibull_inv(0.5, 1.5, 2), cauchy_inv(0.75), logistic_cdf(2), pareto_cdf(2, 3), rayleigh_inv(0.5, 2)].map(|x| round(x, 4))"
            ),
            "[0.8647, 0.3466, 0.7619, 0.4661, 1.5664, 1, 0.8808, 0.875, 2.3548]"
        );
        assert_eq!(
            show(
                "[binom_pmf(5, 10, 0.5), binom_cdf(3, 10, 0.5), binom_inv(0.5, 10, 0.5), poisson_pmf(2, 3), poisson_cdf(2, 3), poisson_inv(0.5, 3)].map(|x| round(x, 4))"
            ),
            "[0.2461, 0.1719, 5, 0.224, 0.4232, 3]"
        );
        assert_eq!(show("t_test([5.1, 4.9, 5.6, 5.8, 5.0], [6.1, 6.4, 5.9, 6.8, 6.2]).p < 0.01"), "true");
        assert_eq!(show("seed(1); [rand_normal(0, 1, 3).len, rand_exp(1) > 0, rand_poisson(50, 4).len]"), "[3, true, 4]");
        assert!(try_eval("normal_inv(1.5)").is_err());
        assert!(try_eval("normal_pdf(0, 0, -1)").is_err());
        assert!(try_eval("binom_pmf(1, 10, 2)").is_err());
    }
}
