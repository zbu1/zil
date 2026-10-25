//! Statistics over lists: sums, means, spread, extremes.

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::units::Unit;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::ops::binary as op;
use crate::value::{Value, compare, num};
use std::collections::HashMap;

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
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("aggregate", &["sum", "product", "avg", "min", "max"]),
        ("spread", &["median", "mode", "percentile", "variance", "stdev", "describe"]),
        ("relations", &["corr", "fit"]),
        ("transform", &["zscore", "normalize", "cumsum", "deltas"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("sum", "sum(xs: list) -> num|quantity", "add up a list, units included", &["[1, 2, 3].sum", "[1 m, 50 cm].sum"], &["avg", "reduce"]),
    doc("avg", "avg(xs: list) -> num|quantity", "the mean (average)", &["[1, 2, 4].avg", "[2 h, 30 min].avg"], &["sum", "median"])
        .detail("The sum divided by how many items there are"),
    doc("product", "product(xs: list) -> num|quantity", "multiply all the items together", &["[2, 3, 4].product", "[2 m, 3 m].product"], &["sum", "factorial"]),
    doc("median", "median(xs: list) -> float|quantity", "the middle value once sorted, or with an even count, halfway between the middle two", &["[3, 1, 2].median", "[1 m, 3 m, 50 cm, 2 m].median"], &["avg", "percentile"]),
    doc("mode", "mode(xs: list) -> any", "the item that appears most often, or on a tie, the one that appears first", &["[1, 2, 2, 3].mode", r#""hello".chars.mode"#], &["median", "count"]),
    doc("percentile", "percentile(xs: list, p: num) -> float|quantity", "the value p% of the way through the sorted data (p from 0 to 100), so the 90th percentile is above 90% of it", &["[1, 2, 3, 4, 5].percentile(90)", "(1..=100).percentile(25)"], &["median"])
        .detail("Between two items, it's in between them"),
    doc("variance", "variance(xs: list) -> float|quantity", "how spread out the numbers are", &["[2, 4, 4, 4, 5, 5, 7, 9].variance"], &["stdev"])
        .detail("The average squared distance from the mean, for a sample (dividing by n - 1)"),
    doc("stdev", "stdev(xs: list) -> float|quantity", "how spread out the numbers are, in the same units as them", &["[2, 4, 4, 4, 5, 5, 7, 9].stdev", "[1 m, 2 m, 3 m].stdev"], &["variance", "avg"])
        .detail("The square root of the variance, for a sample (dividing by n - 1)"),
    doc("min", "min(xs: list) -> any / min(a: any, b: any, ...) -> any", "the smallest of a list, or of the arguments", &["min(3, 9, 4)", "[2 km, 1 mi].min"], &["max", "sort"]),
    doc("max", "max(xs: list) -> any / max(a: any, b: any, ...) -> any", "the largest of a list, or of the arguments", &["max(3, 9, 4)", r#"["b", "a"].max"#], &["min", "sort"]),
    doc("describe", "describe(xs: list) -> map", "a summary in one map", &["[3, 1, 4, 1, 5, 9, 2, 6].describe"], &["avg", "percentile"])
        .detail("The count, mean, standard deviation, minimum, quartiles (the 25th, 50th and 75th percentiles) and maximum"),
    doc("corr", "corr(xs: list, ys: list) -> float", "how closely two lists rise and fall together (Pearson correlation)", &["corr([1, 2, 3, 4], [2, 4, 5, 9])"], &["fit"])
        .detail("1 is a perfect straight-line match, -1 perfectly opposite, 0 no relationship"),
    doc("fit", "fit(xs: list, ys: list) -> map", "the straight line y = slope × x + intercept that best fits the points (least squares), as {slope, intercept, r2}", &["fit([1, 2, 3], [2, 4, 6])", "fit([1, 2, 3, 4], [2.1, 3.9, 6.2, 7.8])"], &["corr"])
        .detail("r2 is from 0 to 1, how well the line explains the points"),
    doc("zscore", "zscore(xs: list) -> list", "each item as how many standard deviations above (positive) or below (negative) the mean it is", &["[2, 4, 4, 4, 5, 5, 7, 9].zscore"], &["normalize", "stdev"]),
    doc("normalize", "normalize(xs: list) -> list", "rescale the list so the smallest becomes 0, the largest 1, and the rest fall in between", &["[10, 15, 20].normalize"], &["zscore", "lerp"]),
    doc("cumsum", "cumsum(xs: list) -> list", "running totals: each item plus everything before it", &["[1, 2, 3, 4].cumsum", "[1 km, 500 m].cumsum"], &["deltas", "sum"]),
    doc("deltas", "deltas(xs: list) -> list", "the difference between each item and the one before", &["[1, 4, 9, 16].deltas", "[1, 4, 9, 16].deltas.deltas"], &["cumsum", "windows"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("sum", [List(l)]) => sum(&l.borrow())?,
        ("avg", [List(l)]) => {
            let l = l.borrow();
            if l.is_empty() {
                return Err("empty list".into());
            }
            op(BinOp::Div, &sum(&l)?, &Float(l.len() as f64))?
        }
        ("product", [List(l)]) => {
            let mut acc = Value::int(1);
            for v in l.borrow().iter() {
                acc = op(BinOp::Mul, &acc, v)?;
            }
            acc
        }
        ("median" | "percentile" | "variance" | "stdev", [List(l), rest @ ..]) => {
            let (mut xs, u) = floats(&l.borrow())?;
            let qty = |x: f64, u: Option<Unit>| u.map_or(Float(x), |u| Value::qty(x, u));
            match (name, rest) {
                ("median", []) => qty(percentile(&mut xs, 50.0), u),
                ("percentile", [p]) if num(p).is_some_and(|p| (0.0..=100.0).contains(&p)) => qty(percentile(&mut xs, num(p).unwrap()), u),
                ("percentile", [p]) => return Err(Fail::Arg(1, format!("p must be from 0 to 100, got {p:?}"))),
                ("variance" | "stdev", []) if xs.len() < 2 => {
                    return Err(format!("needs at least 2 items, got {}", xs.len()).into());
                }
                ("variance", []) => qty(variance(&xs), u.map(|u| u.pow(2))),
                ("stdev", []) => qty(variance(&xs).sqrt(), u),
                _ => return Err(Fail::BadArgs),
            }
        }
        ("mode", [List(l)]) => {
            // Keyed by display form, so 1 and 1.0 count together (1 m and 100 cm don't).
            let l = l.borrow();
            let mut counts: HashMap<String, usize> = HashMap::new();
            for v in l.iter() {
                *counts.entry(format!("{v:?}")).or_default() += 1;
            }
            let best = counts.values().max().ok_or("empty list")?;
            l.iter().find(|v| counts[&format!("{v:?}")] == *best).unwrap().clone()
        }
        ("min" | "max", [List(l)]) => extreme(name, &l.borrow())?,
        ("min" | "max", vs) if vs.len() >= 2 => extreme(name, vs)?,
        ("describe", [List(l)]) => {
            let (mut xs, u) = floats(&l.borrow())?;
            let q = |x: f64| u.clone().map_or(Float(x), |u| Value::qty(x, u));
            let mean = xs.iter().sum::<f64>() / xs.len() as f64;
            let sd = if xs.len() > 1 { q(variance(&xs).sqrt()) } else { Nil };
            let mut p = |n| q(percentile(&mut xs, n));
            let rows = [
                ("n", Value::int(l.borrow().len() as i64)),
                ("mean", q(mean)),
                ("stdev", sd),
                ("min", p(0.0)),
                ("p25", p(25.0)),
                ("median", p(50.0)),
                ("p75", p(75.0)),
                ("max", p(100.0)),
            ];
            Value::map(rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        }
        ("corr" | "fit", [List(a), List(b)]) => {
            let (xs, ys) = (plain(&a.borrow())?, plain(&b.borrow())?);
            if xs.len() != ys.len() || xs.len() < 2 {
                return Err(format!("needs two lists of the same length, at least 2, got {} and {}", xs.len(), ys.len()).into());
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
            if name == "corr" {
                Float(r)
            } else {
                let slope = sxy / sxx;
                let rows = [("slope", Float(slope)), ("intercept", Float(my - slope * mx)), ("r2", Float(if syy == 0.0 { 1.0 } else { r * r }))];
                Value::map(rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
            }
        }
        ("zscore" | "normalize", [List(l)]) => {
            let xs = plain(&l.borrow())?;
            let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
            let (shift, scale) = match name {
                "normalize" => (lo, hi - lo),
                _ if xs.len() < 2 => (0.0, 0.0),
                _ => (xs.iter().sum::<f64>() / xs.len() as f64, variance(&xs).sqrt()),
            };
            if scale == 0.0 || !scale.is_finite() {
                return Err("needs at least two different values".into());
            }
            Value::list(xs.iter().map(|x| Float((x - shift) / scale)).collect())
        }
        ("cumsum", [List(l)]) => {
            let mut out: Vec<Value> = vec![];
            for v in l.borrow().iter() {
                out.push(match out.last() {
                    None => v.clone(),
                    Some(acc) => op(BinOp::Add, acc, v)?,
                });
            }
            Value::list(out)
        }
        ("deltas", [List(l)]) => Value::list(l.borrow().windows(2).map(|w| op(BinOp::Sub, &w[1], &w[0])).collect::<Result<_, _>>()?),
        _ => return Err(Fail::BadArgs),
    })
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
