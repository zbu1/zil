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
        ("aggregate", &["sum", "product", "avg", "min", "max"]),
        ("spread", &["median", "mode", "percentile", "variance", "stdev", "describe"]),
        ("relations", &["corr", "fit"]),
        ("transform", &["zscore", "normalize", "cumsum", "deltas"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{extreme, floats, percentile as at, plain, sum as total, variance as var};
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
        Ok(l.iter().find(|v| counts[&format!("{v:?}")] == *best).unwrap().clone())
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

    fn spread(xs: &List) -> Result<(Vec<f64>, Option<Unit>), String> {
        let (xs, u) = floats(&xs.borrow())?;
        if xs.len() < 2 {
            return Err(format!("needs at least 2 items, got {}", xs.len()));
        }
        Ok((xs, u))
    }

    /// how spread out the numbers are
    ///
    /// The average squared distance from the mean, for a sample (dividing by n - 1)
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].variance"], see = ["stdev"])]
    fn variance(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = spread(xs)?;
        Ok(qty(var(&xs), u.map(|u| u.pow(2))))
    }

    /// how spread out the numbers are, in the same units as them
    ///
    /// The square root of the variance, for a sample (dividing by n - 1)
    #[builtin(examples = ["[2, 4, 4, 4, 5, 5, 7, 9].stdev", "[1 m, 2 m, 3 m].stdev"], see = ["variance", "avg"])]
    fn stdev(xs: &List) -> Result<As<Or<f64, Qty>>, String> {
        let (xs, u) = spread(xs)?;
        Ok(qty(var(&xs).sqrt(), u))
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
        let sd = if xs.len() > 1 { q(var(&xs).sqrt()) } else { Value::Nil };
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
    #[builtin(examples = ["fit([1, 2, 3], [2, 4, 6])", "fit([1, 2, 3, 4], [2.1, 3.9, 6.2, 7.8])"], see = ["corr"])]
    fn fit(xs: &List, ys: &List) -> Result<IndexMap<String, Value>, String> {
        let (_, slope, intercept, r2) = line(xs, ys)?;
        Ok(map([("slope", Value::Float(slope)), ("intercept", Value::Float(intercept)), ("r2", Value::Float(r2))]))
    }

    fn rescale(xs: &List, normalize: bool) -> Result<Vec<Value>, String> {
        let xs = plain(&xs.borrow())?;
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        let (shift, scale) = match normalize {
            true => (lo, hi - lo),
            _ if xs.len() < 2 => (0.0, 0.0),
            _ => (xs.iter().sum::<f64>() / xs.len() as f64, var(&xs).sqrt()),
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

fn variance(xs: &[f64]) -> f64 {
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() - 1) as f64
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
        ] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }
}
