//! Statistics over lists: sums, means, spread, extremes.

use crate::ast::BinOp;
use crate::modules::Module;
use crate::modules::units::Unit;
use crate::ops::binary as op;
use crate::value::{Value, compare, num};

pub const MODULE: Module = Module {
    name: "stats",
    about: "summarize lists of numbers: totals, averages, spread, extremes and trends, units welcome",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Summaries of lists of numbers: totals, averages, spread and trends. Every function here works on a list of numbers, \
and most accept quantities with units, like a list of lengths or prices."),
        ("good to know", "\
`variance` and `stdev` treat the list as a sample from something bigger, the usual choice, so they divide by n - 1 \
rather than n."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("stats", &[
            ("standard deviation", "[2, 4, 4, 4, 5, 5, 7, 9].stdev"),
            ("the value 90% of the data is below", "(1..=100).percentile(90)"),
            ("everything at once", "[3, 1, 4, 1, 5, 9, 2, 6].describe"),
            ("the straight line closest to the points", "fit([1, 2, 3, 4], [2.1, 3.9, 6.2, 7.8])"),
            ("running total", "[$5, $12, $3].cumsum"),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("aggregate", &["sum", "product", "avg", "geo_mean", "harmonic_mean", "rms", "min", "max"]),
        ("spread", &["median", "mode", "percentile", "variance", "stdev", "mad", "sem", "iqr", "spread", "describe"]),
        ("relations", &["corr", "spearman", "cov", "fit"]),
        ("transform", &["zscore", "normalize", "ranks", "cumsum", "deltas"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{extreme, floats, pair, percentile as at, plain, rank, sum as total, variance as var};
    use crate::ast::BinOp;
    use crate::modules::Fail;
    use crate::modules::units::Unit;
    use crate::ops::binary as op;
    use crate::signature::{As, List, Num, Or, Qty, Rest};
    use crate::value::Value;
    use indexmap::IndexMap;
    use std::collections::HashMap;

    fn qty(x: f64, u: Option<Unit>) -> As<Or<f64, Qty>> {
        u.map_or(Value::Float(x), |u| Value::qty(x, u)).into()
    }

    fn map<const N: usize>(rows: [(&str, Value); N]) -> IndexMap<String, Value> {
        rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    /// add up a list, units included
    #[builtin(examples = ["[1, 2, 3].sum", "[1 m, 50 cm].sum"], see = ["avg", "reduce"])]
    fn sum(xs: &List) -> Result<As<Or<Num, Qty>>, String> {
        total(&xs.borrow()).map(As::from)
    }

    /// the mean (average)
    ///
    /// The sum divided by how many items there are
    #[builtin(examples = ["[1, 2, 4].avg", "[2 h, 30 min].avg"], see = ["sum", "median"])]
    fn avg(xs: &List) -> Result<As<Or<Num, Qty>>, String> {
        let l = xs.borrow();
        if l.is_empty() {
            return Err("empty list".into());
        }
        op(BinOp::Div, &total(&l)?, &Value::Float(l.len() as f64)).map(As::from)
    }

    /// multiply all the items together
    #[builtin(examples = ["[2, 3, 4].product", "[2 m, 3 m].product"], see = ["sum", "factorial"])]
    fn product(xs: &List) -> Result<As<Or<Num, Qty>>, String> {
        let mut acc = Value::int(1);
        for v in xs.borrow().iter() {
            acc = op(BinOp::Mul, &acc, v)?;
        }
        Ok(acc.into())
    }

    /// the middle value once sorted, or with an even count, halfway between the middle two
    #[builtin(examples = ["[3, 1, 2].median", "[1 m, 3 m, 50 cm, 2 m].median"], see = ["avg", "percentile"])]
    fn median(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (mut xs, u) = floats(&xs.borrow())?;
        Ok(qty(at(&mut xs, 50.0), u))
    }

    /// the item that appears most often, or on a tie, the one that appears first
    #[builtin(examples = ["[1, 2, 2, 3].mode", r#""hello".chars.mode"#], see = ["median", "count"])]
    fn mode(xs: &List) -> Result<Value, String> {
        // Keyed by display form, so 1 and 1.0 count together (1 m and 100 cm don't).
        let l = xs.borrow();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for v in l.iter() {
            *counts.entry(format!("{v:?}")).or_default() += 1;
        }
        let best = counts.values().max().ok_or("empty list")?;
        Ok(l.iter().find(|v| counts[&format!("{v:?}")] == *best).cloned().unwrap_or(Value::Nil))
    }

    /// the value p% of the way through the sorted data (p from 0 to 100), so the 90th percentile is above 90% of it
    ///
    /// Between two items, it's in between them
    #[builtin(examples = ["[1, 2, 3, 4, 5].percentile(90)", "(1..=100).percentile(25)"], see = ["median"])]
    fn percentile(xs: &List, p: Num) -> Result<As<Or<f64, Qty>>, Fail> {
        let (mut xs, u) = floats(&xs.borrow())?;
        match crate::value::num(&p) {
            Some(p) if (0.0..=100.0).contains(&p) => Ok(qty(at(&mut xs, p), u)),
            _ => Err(Fail::Arg(1, format!("p must be from 0 to 100, got {:?}", *p))),
        }
    }

    fn spread_args(xs: &List, mode: Option<&str>) -> Result<(Vec<f64>, Option<Unit>, bool), Fail> {
        let pop = match mode {
            None | Some("sample") => false,
            Some("pop") => true,
            Some(m) => return Err(Fail::Arg(1, format!("mode must be \"sample\" or \"pop\", got {m:?}"))),
        };
        let (xs, u) = floats(&xs.borrow())?;
        let need = if pop { 1 } else { 2 };
        if xs.len() < need {
            return Err(format!("needs at least {need} items, got {}", xs.len()).into());
        }
        Ok((xs, u, pop))
    }

    /// how spread out the numbers are
    ///
    /// The average squared distance from the mean, for a sample (dividing by n - 1), or with "pop", for a whole population (dividing by n)
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].variance", r#"[2, 4, 4, 4, 5, 5, 7, 9].variance("pop")"#], see = ["stdev"])]
    fn variance(xs: &List, mode: Option<&str>) -> Result<As<Or<f64, Qty>>, Fail> {
        let (xs, u, pop) = spread_args(xs, mode)?;
        Ok(qty(var(&xs, pop), u.map(|u| u.pow(2))))
    }

    /// how spread out the numbers are, in the same units as them
    ///
    /// The square root of the variance, for a sample (dividing by n - 1), or with "pop", for a whole population (dividing by n)
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].stdev", "[1 m, 2 m, 3 m].stdev", r#"[2, 4, 4, 4, 5, 5, 7, 9].stdev("pop")"#], see = ["variance", "avg"])]
    fn stdev(xs: &List, mode: Option<&str>) -> Result<As<Or<f64, Qty>>, Fail> {
        let (xs, u, pop) = spread_args(xs, mode)?;
        Ok(qty(var(&xs, pop).sqrt(), u))
    }

    /// the geometric mean: the nth root of the product, the right average for growth factors
    #[builtin(examples = ["[1, 2, 4].geo_mean", "[1.1, 1.2, 0.9].geo_mean"], see = ["avg", "harmonic_mean"])]
    fn geo_mean(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = floats(&xs.borrow())?;
        if xs.iter().any(|x| *x <= 0.0) {
            return Err("needs positive numbers".into());
        }
        Ok(qty((xs.iter().map(|x| x.ln()).sum::<f64>() / xs.len() as f64).exp(), u))
    }

    /// the harmonic mean: the reciprocal of the average reciprocal, the right average for rates
    #[builtin(examples = ["[40, 60].harmonic_mean", "[1, 2, 4].harmonic_mean"], see = ["avg", "geo_mean"])]
    fn harmonic_mean(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = floats(&xs.borrow())?;
        if xs.iter().any(|x| *x <= 0.0) {
            return Err("needs positive numbers".into());
        }
        Ok(qty(xs.len() as f64 / xs.iter().map(|x| 1.0 / x).sum::<f64>(), u))
    }

    /// the root mean square: the square root of the average of the squares
    #[builtin(examples = ["[3, 4].rms", "[1, -1, 1, -1].rms"], see = ["avg", "stdev"])]
    fn rms(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = floats(&xs.borrow())?;
        Ok(qty((xs.iter().map(|x| x * x).sum::<f64>() / xs.len() as f64).sqrt(), u))
    }

    /// the median absolute deviation: the median distance from the median, a spread that outliers barely move
    #[builtin(examples = ["[1, 2, 3, 4, 100].mad"], see = ["stdev", "iqr"])]
    fn mad(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (mut xs, u) = floats(&xs.borrow())?;
        let m = at(&mut xs, 50.0);
        let mut d: Vec<f64> = xs.iter().map(|x| (x - m).abs()).collect();
        Ok(qty(at(&mut d, 50.0), u))
    }

    /// the standard error of the mean: the standard deviation divided by the square root of the count
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].sem"], see = ["stdev", "avg"])]
    fn sem(xs: &List) -> Result<As<Or<f64, Qty>>, Fail> {
        let (xs, u, pop) = spread_args(xs, None)?;
        Ok(qty((var(&xs, pop) / xs.len() as f64).sqrt(), u))
    }

    /// the interquartile range: the 75th percentile minus the 25th
    #[builtin(examples = ["[1, 2, 3, 4, 5, 6, 7, 8].iqr"], see = ["percentile", "mad"])]
    fn iqr(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (mut xs, u) = floats(&xs.borrow())?;
        Ok(qty(at(&mut xs, 75.0) - at(&mut xs, 25.0), u))
    }

    /// the largest minus the smallest (`range` makes a list of numbers)
    #[builtin(name = "spread", examples = ["[3, 9, 4].spread", "[2 m, 50 cm].spread"], see = ["iqr", "stdev"])]
    fn spread_of(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = floats(&xs.borrow())?;
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        Ok(qty(hi - lo, u))
    }

    /// the sample covariance of two lists: positive when they rise together
    #[builtin(examples = ["cov([1, 2, 3, 4], [2, 4, 5, 9])"], see = ["corr"])]
    fn cov(xs: &List, ys: &List) -> Result<f64, String> {
        let (xs, ys) = pair(xs, ys)?;
        let n = xs.len() as f64;
        let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
        Ok(xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / (n - 1.0))
    }

    /// each item's place in the sorted order, starting at 1, with ties sharing the average place
    #[builtin(examples = ["[10, 30, 20].ranks", "[1, 2, 2, 3].ranks"], see = ["spearman", "sort"])]
    fn ranks(xs: &List) -> Result<Vec<Value>, String> {
        Ok(rank(&plain(&xs.borrow())?).into_iter().map(Value::Float).collect())
    }

    /// how closely two lists rise and fall together in order, not necessarily in a straight line (rank correlation)
    #[builtin(examples = ["spearman([1, 2, 3, 4], [1, 4, 9, 16])"], see = ["corr", "ranks"])]
    fn spearman(xs: &List, ys: &List) -> Result<f64, String> {
        let (xs, ys) = pair(xs, ys)?;
        let (rx, ry) = (rank(&xs), rank(&ys));
        let n = xs.len() as f64;
        let (mx, my) = (rx.iter().sum::<f64>() / n, ry.iter().sum::<f64>() / n);
        let sxy: f64 = rx.iter().zip(&ry).map(|(x, y)| (x - mx) * (y - my)).sum();
        let (sxx, syy): (f64, f64) = (rx.iter().map(|x| (x - mx).powi(2)).sum(), ry.iter().map(|y| (y - my).powi(2)).sum());
        if sxx == 0.0 || syy == 0.0 {
            return Err("every value in a list is the same".into());
        }
        Ok(sxy / (sxx * syy).sqrt())
    }

    fn of_all(a: &Value, b: &Value, rest: Rest<&Value>) -> Vec<Value> {
        [a, b].into_iter().chain(rest.0).cloned().collect()
    }

    /// the smallest of a list, or of the arguments
    #[builtin(examples = ["min(3, 9, 4)", "[2 km, 1 mi].min"], see = ["max", "sort"])]
    fn min(xs: &List) -> Result<Value, String> {
        extreme("min", &xs.borrow())
    }

    #[builtin(name = "min")]
    fn min_of(a: &Value, b: &Value, rest: Rest<&Value>) -> Result<Value, String> {
        extreme("min", &of_all(a, b, rest))
    }

    /// the largest of a list, or of the arguments
    #[builtin(examples = ["max(3, 9, 4)", r#"["b", "a"].max"#], see = ["min", "sort"])]
    fn max(xs: &List) -> Result<Value, String> {
        extreme("max", &xs.borrow())
    }

    #[builtin(name = "max")]
    fn max_of(a: &Value, b: &Value, rest: Rest<&Value>) -> Result<Value, String> {
        extreme("max", &of_all(a, b, rest))
    }

    /// a summary in one map
    ///
    /// The count, mean, standard deviation, minimum, quartiles (the 25th, 50th and 75th percentiles) and maximum
    #[builtin(examples = ["[3, 1, 4, 1, 5, 9, 2, 6].describe"], see = ["avg", "percentile"])]
    fn describe(xs: &List) -> Result<IndexMap<String, Value>, String> {
        let n = xs.borrow().len();
        let (mut xs, u) = floats(&xs.borrow())?;
        let q = |x: f64| u.clone().map_or(Value::Float(x), |u| Value::qty(x, u));
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let sd = if xs.len() > 1 { q(var(&xs, false).sqrt()) } else { Value::Nil };
        let mut p = |n| q(at(&mut xs, n));
        Ok(map([
            ("n", Value::int(n as i64)),
            ("mean", q(mean)),
            ("stdev", sd),
            ("min", p(0.0)),
            ("p25", p(25.0)),
            ("median", p(50.0)),
            ("p75", p(75.0)),
            ("max", p(100.0)),
        ]))
    }

    /// Pearson's r, and the least-squares slope and intercept.
    fn line(xs: &List, ys: &List) -> Result<(f64, f64, f64, f64), String> {
        let (xs, ys) = (plain(&xs.borrow())?, plain(&ys.borrow())?);
        if xs.len() != ys.len() || xs.len() < 2 {
            return Err(format!("needs two lists of the same length, at least 2, got {} and {}", xs.len(), ys.len()));
        }
        let n = xs.len() as f64;
        let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
        let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
        let syy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
        if sxx == 0.0 {
            return Err("every x is the same".into());
        }
        let r = sxy / (sxx * syy).sqrt();
        let slope = sxy / sxx;
        Ok((r, slope, my - slope * mx, if syy == 0.0 { 1.0 } else { r * r }))
    }

    /// how closely two lists rise and fall together (Pearson correlation)
    ///
    /// 1 is a perfect straight-line match, -1 perfectly opposite, 0 no relationship
    #[builtin(examples = ["corr([1, 2, 3, 4], [2, 4, 5, 9])"], see = ["fit"])]
    fn corr(xs: &List, ys: &List) -> Result<f64, String> {
        Ok(line(xs, ys)?.0)
    }

    /// the straight line y = slope × x + intercept that best fits the points (least squares), as {slope, intercept, r2}
    ///
    /// r2 is from 0 to 1, how well the line explains the points
    ///
    /// Given a degree as a third argument, fits a polynomial instead and returns its coefficients highest power first, for poly_eval: fit(xs, ys, 1) gives [slope, intercept]. Degrees above about 8 lose accuracy
    #[builtin(examples = ["fit([1, 2, 3], [2, 4, 6])", "fit([1, 2, 3, 4], [2.1, 3.9, 6.2, 7.8])"], see = ["corr"])]
    fn fit(xs: &List, ys: &List) -> Result<IndexMap<String, Value>, String> {
        let (_, slope, intercept, r2) = line(xs, ys)?;
        Ok(map([("slope", Value::Float(slope)), ("intercept", Value::Float(intercept)), ("r2", Value::Float(r2))]))
    }

    #[builtin(name = "fit", examples = ["fit([0, 1, 2, 3], [1, 2, 5, 10], 2).map(round)"])]
    fn fit_poly(xs: &List, ys: &List, degree: i64) -> Result<Vec<Value>, Fail> {
        let (xs, ys) = (plain(&xs.borrow())?, plain(&ys.borrow())?);
        let d = usize::try_from(degree).ok().filter(|&d| d <= 20).ok_or_else(|| Fail::Arg(2, "degree must be from 0 to 20".into()))?;
        if xs.len() != ys.len() || xs.len() <= d {
            return Err(format!("needs two lists of the same length, with more than {d} points, got {} and {}", xs.len(), ys.len()).into());
        }
        // ponytail: normal equations in f64, which blur past degree ~8; QR would hold up better.
        let m = d + 1;
        let mut a: Vec<Vec<f64>> = (0..m)
            .map(|i| {
                (0..=m)
                    .map(|j| if j < m { xs.iter().map(|x| x.powi((i + j) as i32)).sum() } else { xs.iter().zip(&ys).map(|(x, y)| x.powi(i as i32) * y).sum() })
                    .collect()
            })
            .collect();
        for c in 0..m {
            let p = (c..m).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs())).unwrap_or(c);
            if a[p][c].abs() < 1e-12 {
                return Err("the points do not determine a polynomial of that degree".into());
            }
            a.swap(p, c);
            for r in (0..m).filter(|&r| r != c) {
                let f = a[r][c] / a[c][c];
                let pivot_row = a[c].clone();
                a[r].iter_mut().zip(pivot_row).skip(c).for_each(|(x, y)| *x -= f * y);
            }
        }
        Ok((0..m).rev().map(|i| Value::Float(a[i][m] / a[i][i])).collect())
    }

    fn rescale(xs: &List, normalize: bool) -> Result<Vec<Value>, String> {
        let xs = plain(&xs.borrow())?;
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        let (shift, scale) = match normalize {
            true => (lo, hi - lo),
            _ if xs.len() < 2 => (0.0, 0.0),
            _ => (xs.iter().sum::<f64>() / xs.len() as f64, var(&xs, false).sqrt()),
        };
        if scale == 0.0 || !scale.is_finite() {
            return Err("needs at least two different values".into());
        }
        Ok(xs.iter().map(|x| Value::Float((x - shift) / scale)).collect())
    }

    /// each item as how many standard deviations above (positive) or below (negative) the mean it is
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].zscore"], see = ["normalize", "stdev"])]
    fn zscore(xs: &List) -> Result<Vec<Value>, String> {
        rescale(xs, false)
    }

    /// rescale the list so the smallest becomes 0, the largest 1, and the rest fall in between
    #[builtin(examples = ["[10, 15, 20].normalize"], see = ["zscore", "lerp"])]
    fn normalize(xs: &List) -> Result<Vec<Value>, String> {
        rescale(xs, true)
    }

    /// running totals: each item plus everything before it
    #[builtin(examples = ["[1, 2, 3, 4].cumsum", "[1 km, 500 m].cumsum"], see = ["deltas", "sum"])]
    fn cumsum(xs: &List) -> Result<Vec<Value>, String> {
        let mut out: Vec<Value> = vec![];
        for v in xs.borrow().iter() {
            out.push(match out.last() {
                None => v.clone(),
                Some(acc) => op(BinOp::Add, acc, v)?,
            });
        }
        Ok(out)
    }

    /// the difference between each item and the one before
    #[builtin(examples = ["[1, 4, 9, 16].deltas", "[1, 4, 9, 16].deltas.deltas"], see = ["cumsum", "windows"])]
    fn deltas(xs: &List) -> Result<Vec<Value>, String> {
        xs.borrow().windows(2).map(|w| op(BinOp::Sub, &w[1], &w[0])).collect()
    }
}

fn sum(l: &[Value]) -> Result<Value, String> {
    let Some((first, rest)) = l.split_first() else {
        return Ok(Value::int(0));
    };
    let mut acc = first.clone();
    for v in rest {
        acc = op(BinOp::Add, &acc, v)?;
    }
    Ok(acc)
}

/// Numbers, or quantities of one kind in the first item's unit, as f64s.
fn floats(l: &[Value]) -> Result<(Vec<f64>, Option<Unit>), String> {
    let unit = match l.first().ok_or("empty list")? {
        Value::Qty(_, u) => Some(u.clone()),
        _ => None,
    };
    let xs = l.iter().map(|v| match (v, &unit) {
        (Value::Qty(x, w), Some(u)) if w.dim() == u.dim() => Some(u.value_from_si(w.to_si(*x))),
        (_, None) => num(v),
        _ => None,
    });
    Ok((xs.collect::<Option<_>>().ok_or("expected a list of numbers, or of quantities of one kind")?, unit))
}

/// Plain numbers only, for stats whose result would need units the list doesn't share.
fn plain(l: &[Value]) -> Result<Vec<f64>, String> {
    l.iter().map(num).collect::<Option<_>>().ok_or_else(|| "expected a list of numbers".into())
}

/// Linear interpolation between the closest ranks, like numpy's default.
fn percentile(xs: &mut [f64], p: f64) -> f64 {
    xs.sort_by(f64::total_cmp);
    let r = p / 100.0 * (xs.len() - 1) as f64;
    let (lo, hi) = (r.floor() as usize, r.ceil() as usize);
    xs[lo] + (xs[hi] - xs[lo]) * (r - lo as f64)
}

fn variance(xs: &[f64], pop: bool) -> f64 {
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() - !pop as usize) as f64
}

fn pair(xs: &crate::signature::List, ys: &crate::signature::List) -> Result<(Vec<f64>, Vec<f64>), String> {
    let (xs, ys) = (plain(&xs.borrow())?, plain(&ys.borrow())?);
    if xs.len() != ys.len() || xs.len() < 2 {
        return Err(format!("needs two lists of the same length, at least 2, got {} and {}", xs.len(), ys.len()));
    }
    Ok((xs, ys))
}

/// 1-based places in sorted order; ties share their average place.
fn rank(xs: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..xs.len()).collect();
    idx.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
    let mut out = vec![0.0; xs.len()];
    let mut i = 0;
    while i < idx.len() {
        let j = (i..idx.len()).take_while(|&j| xs[idx[j]] == xs[idx[i]]).last().unwrap();
        for &k in &idx[i..=j] {
            out[k] = (i + j) as f64 / 2.0 + 1.0;
        }
        i = j + 1;
    }
    out
}

fn extreme(name: &str, vs: &[Value]) -> Result<Value, String> {
    let mut best: Option<&Value> = None;
    for v in vs {
        best = Some(match best {
            None => v,
            Some(b) => {
                let ord = compare(v, b).ok_or_else(|| format!("cannot compare {} and {}", crate::error::short(v), crate::error::short(b)))?;
                if (name == "min" && ord.is_lt()) || (name == "max" && ord.is_gt()) { v } else { b }
            }
        });
    }
    best.cloned().ok_or_else(|| "empty list".into())
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn stats() {
        assert_eq!(show("[1, 2, 3, 4].avg"), "2.5");
        assert_eq!(show("[1 m, 50 cm].sum"), "1.5 m");
        assert_eq!(show("max(3, 9, 4)"), "9");
        assert_eq!(show("[1 km, 900 m].min"), "900 m");
        assert_eq!(show("[[3, 1, 2].median, [4, 1, 3, 2].median, [1 m, 3 m].median, [1 m, 3 m, 50 cm, 2 m].median]"), "[2, 2.5, 2 m, 1.5 m]");
        assert_eq!(show("[[1, 2, 2, 3].mode, [1, 1.0, 2, 2].mode, \"hello\".chars.mode]"), "[2, 1, \"l\"]");
        assert_eq!(show("[[1, 2, 3, 4, 5].percentile(90), [5].percentile(50), (1..=100).percentile(0)]"), "[4.6, 5, 1]");
        assert_eq!(show("[[2, 4, 4, 4, 5, 5, 7, 9].variance, [1, 2, 3, 4].stdev.round(4)]"), "[4.57143, 1.291]");
        assert_eq!(show("[[1 m, 2 m, 3 m].stdev, [1 m, 200 cm, 3 m].variance]"), "[1 m, 1 m^2]");
        assert_eq!(show("[[2, 3, 4].product, [].product, [2 m, 3 m].product, (1..=25).product]"), "[24, 1, 6 m^2, 15511210043330985984000000]");
        assert_eq!(show("[1, 2, 3, 4, 5].describe"), "{n: 5, mean: 3, stdev: 1.58114, min: 1, p25: 2, median: 3, p75: 4, max: 5}");
        assert_eq!(show("[1 m, 3 m].describe.mean"), "2 m");
        assert_eq!(show("[[2, 4, 4, 4, 5, 5, 7, 9].variance(\"pop\"), [5].stdev(\"pop\")]"), "[4, 0]");
        assert_eq!(
            show("[[1, 2, 4].geo_mean, [40, 60].harmonic_mean, [3, 4].rms.round(4), [1, 2, 3, 4, 100].mad, [1, 2, 3, 4, 5, 6, 7, 8].iqr]"),
            "[2, 48, 3.5355, 1, 3.5]"
        );
        assert_eq!(
            show("[[3, 9, 4].spread, [2 m, 50 cm].spread, cov([1, 2, 3], [2, 4, 6]), [1, 2, 2, 3].ranks, spearman([1, 2, 3, 4], [1, 4, 9, 16])]"),
            "[6, 1.5 m, 2, [1, 2.5, 2.5, 4], 1]"
        );
        assert_eq!(show("[corr([1, 2, 3], [2, 4, 6]), corr([1, 2, 3], [3, 2, 1])]"), "[1, -1]");
        assert_eq!(show("fit([1, 2, 3], [3, 5, 7])"), "{slope: 2, intercept: 1, r2: 1}");
        assert_eq!(show("[[1, 2, 3].zscore, [10, 15, 20].normalize]"), "[[-1, 0, 1], [0, 0.5, 1]]");
        assert_eq!(show("[[1, 2, 3].cumsum, [1 km, 500 m].cumsum, [1, 4, 9].deltas, [].cumsum]"), "[[1, 3, 6], [1 km, 1.5 km], [3, 5], []]");
        for bad in [
            "[].median",
            "corr([1], [2])",
            "corr([1, 1], [2, 3])",
            "[5, 5].normalize",
            "[].describe",
            "[1].stdev",
            "[1 m, 2].median",
            "[1, 2].percentile(101)",
            "[].mode",
            "[1, 2].variance(\"x\")",
            "[0, 1].geo_mean",
            "[1, 1].spearman([2, 3])",
        ] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }
}
