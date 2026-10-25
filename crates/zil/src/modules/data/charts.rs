//! Text charts for the terminal: sparklines, bar charts, histograms, function plots, heatmaps, progress bars, trees.

use crate::modules::{Fail, Module};
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "charts",
    about: "charts drawn as text: sparklines, bars, histograms, plots, heatmaps, progress bars and trees",
    #[rustfmt::skip]
    examples: &[
        ("charts", &[
            ("a trend in one line", "[3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5].sparkline"),
            ("a budget", "bars({rent: $1800, food: $600, fun: $250})"),
            ("letter frequencies", r#""mississippi".chars.count_by(|c| c) |> bars"#),
            ("dice sums", "(1..=2000).map(|_| rand(1, 6) + rand(1, 6)).histogram(11)"),
            ("a sine wave", "plot(sin, 0, 2 * pi)"),
            ("a multiplication table", "(1..=6).map(|r| (1..=10).map(|c| r * c)).heatmap"),
            ("a config at a glance", r#"tree_view({server: {host: "localhost", ports: [80, 443]}, debug: false})"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{BLOCKS, bars as draw_bars, f, floats, tree};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::{Func, List, Map, Num, Or};
    use crate::value::{Value, fmt_float, num};

    fn size(n: i64, max: i64, what: &str) -> Result<usize, Fail> {
        if (1..=max).contains(&n) { Ok(n as usize) } else { Err(Fail::Arg(1, format!("{what} must be an int from 1 to {max}"))) }
    }

    /// a chart one line tall: one bar character per number, the smallest number lowest and the largest highest
    #[builtin(examples = ["[1, 5, 2, 8, 3].sparkline", "(0..20).map(|x| sin(x / 3)).sparkline"], see = ["bars", "plot"])]
    fn sparkline(xs: &List) -> Result<String, Fail> {
        let xs = floats(&xs.borrow())?;
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        let level = |x: f64| if hi > lo { ((x - lo) / (hi - lo) * 7.0).round() as usize } else { 3 };
        Ok(xs.iter().map(|x| BLOCKS[level(*x)]).collect())
    }

    /// a horizontal bar chart
    ///
    /// A map's keys label the bars and its values set their lengths, and a list's bars are numbered. The longest bar is width characters
    #[builtin(examples = ["bars({a: 3, b: 7, c: 5})", "[2, 4, 8].bars(16)"], see = ["histogram", "sparkline"])]
    fn bars(data: Or<&Map, &List>, #[default(40)] width: i64) -> Result<String, Fail> {
        let width = size(width, 500, "width")?;
        let rows: Vec<(String, Value)> = match data {
            Or::A(m) => m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            Or::B(l) => l.borrow().iter().enumerate().map(|(i, v)| (i.to_string(), v.clone())).collect(),
        };
        draw_bars(&rows, width)
    }

    /// split the range of the numbers into bins of equal width, count how many numbers fall in each, and draw the counts as bars
    #[builtin(examples = ["[1, 2, 2, 3, 3, 3, 4, 4, 5].histogram(5)"], see = ["bars", "count_by"])]
    fn histogram(xs: &List, #[default(10)] bins: i64) -> Result<String, Fail> {
        let bins = size(bins, 200, "bins")?;
        let xs = floats(&xs.borrow())?;
        if xs.is_empty() {
            return Err("empty list".into());
        }
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        let step = if hi > lo { (hi - lo) / bins as f64 } else { 1.0 };
        let mut counts = vec![0i64; bins];
        for x in xs {
            counts[(((x - lo) / step) as usize).min(bins - 1)] += 1;
        }
        let rows: Vec<_> = counts
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("{}–{}", fmt_float(lo + step * i as f64), fmt_float(lo + step * (i + 1) as f64)), Value::int(*c)))
            .collect();
        draw_bars(&rows, 40)
    }

    /// a graph of the function f from x = a to x = b, drawn with braille dots, 60 characters wide and 16 lines tall
    #[builtin(examples = ["plot(|x| x ** 2, -1, 1)"], see = ["sparkline"])]
    fn plot(it: &mut Interp, func: Func, a: f64, b: f64, span: &Span) -> Result<String, Fail> {
        const W: usize = 60 * 2;
        const H: usize = 16 * 4;
        let mut ys = Vec::with_capacity(W);
        for i in 0..W {
            let x = a + (b - a) * i as f64 / (W - 1) as f64;
            let y = it.call(&func, vec![Value::Float(x)], span)?;
            ys.push(f(&y).ok_or_else(|| format!("f must return a number, got {}", y.type_name()))?);
        }
        let finite = ys.iter().copied().filter(|y| y.is_finite());
        let (lo, hi) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(y), hi.max(y)));
        if !lo.is_finite() {
            return Err("f has no finite values on that range".into());
        }
        let mut grid = vec![[0u8; W / 2]; H / 4];
        for (i, y) in ys.iter().enumerate().filter(|(_, y)| y.is_finite()) {
            let row = if hi > lo { ((hi - y) / (hi - lo) * (H - 1) as f64).round() as usize } else { H / 2 };
            const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
            grid[row / 4][i / 2] |= BIT[i % 2][row % 4];
        }
        let (top, bottom) = (fmt_float(hi), fmt_float(lo));
        let pad = top.len().max(bottom.len());
        let mut out: Vec<String> = grid
            .iter()
            .enumerate()
            .map(|(r, cells)| {
                let label = if r == 0 {
                    &top
                } else if r == grid.len() - 1 {
                    &bottom
                } else {
                    ""
                };
                format!("{label:>pad$} ┤{}", cells.iter().map(|b| char::from_u32(0x2800 + *b as u32).unwrap()).collect::<String>())
            })
            .collect();
        let (left, right) = (fmt_float(a), fmt_float(b));
        out.push(format!("{:pad$}  {left}{right:>w$}", "", w = W / 2 - left.chars().count()));
        Ok(out.join("\n"))
    }

    /// nested maps and lists drawn as a tree with branch lines, like the `tree` command draws folders
    #[builtin(examples = [r#"tree_view({src: {main: "rs", lib: "rs"}, docs: ["book"]})"#], see = ["keys", "from_json"])]
    fn tree_view(v: Or<&Map, &List>) -> String {
        let mut out = vec![];
        tree(
            &match v {
                Or::A(m) => Value::Map(m.clone()),
                Or::B(l) => Value::List(l.clone()),
            },
            "",
            &mut out,
        );
        out.join("\n")
    }

    /// a grid of numbers, given as a list of rows, drawn as shades from ░ for the lowest to █ for the highest
    #[builtin(examples = ["[[1, 2, 3], [4, 5, 6], [7, 8, 9]].heatmap"], see = ["bars", "sparkline"])]
    fn heatmap(rows: &List) -> Result<String, Fail> {
        let grid = rows
            .borrow()
            .iter()
            .map(|r| match r {
                Value::List(r) => r.borrow().iter().map(num).collect::<Option<Vec<f64>>>(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(Fail::Arg(0, "expected a list of lists of numbers".into()))?;
        let all = grid.iter().flatten().copied();
        let (lo, hi) = all.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
        const SHADES: [&str; 5] = ["  ", "░░", "▒▒", "▓▓", "██"];
        let shade = |x: f64| if hi > lo { SHADES[((x - lo) / (hi - lo) * 4.0).round() as usize] } else { SHADES[2] };
        Ok(grid.iter().map(|r| r.iter().map(|x| shade(*x)).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n"))
    }

    /// a progress bar filled to frac, from 0 (empty) to 1 (full), width characters wide
    #[builtin(examples = ["progress(0.42)", "progress(2 / 3, 12)"], see = ["bars"])]
    fn progress(frac: Num, #[default(30)] width: i64) -> Result<String, Fail> {
        let f = num(&frac).ok_or(Fail::Arg(0, "expected a number from 0 to 1".into()))?.clamp(0.0, 1.0);
        let w = size(width, 500, "width")?;
        let eighths = (f * w as f64 * 8.0).round() as usize;
        let bar = "█".repeat(eighths / 8) + ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"][eighths % 8];
        let pad = w - bar.chars().count();
        Ok(format!("▕{bar}{}▏ {}%", " ".repeat(pad), (f * 100.0).round()))
    }
}

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A number, or a quantity's number in its own unit.
fn f(v: &Value) -> Option<f64> {
    match v {
        Value::Qty(x, _) => Some(*x),
        v => num(v),
    }
}

/// Numbers, or quantities of one kind in the first one's unit, so 1 km and 700 m chart to scale.
fn floats(l: &[Value]) -> Result<Vec<f64>, Fail> {
    let unit = l.iter().find_map(|v| if let Value::Qty(_, u) = v { Some(u) } else { None });
    let x = |v: &Value| match (v, unit) {
        (Value::Qty(x, w), Some(u)) if w.dim() == u.dim() => Some(u.value_from_si(w.to_si(*x))),
        (Value::Qty(..), _) => None,
        _ => num(v),
    };
    l.iter().map(x).collect::<Option<_>>().ok_or_else(|| "expected numbers, or quantities of one kind".into())
}

/// `label │█████▌ value` rows, the longest bar `width` cells; eighth-blocks for the remainder.
fn bars(rows: &[(String, Value)], width: usize) -> Result<String, Fail> {
    let xs = floats(&rows.iter().map(|r| r.1.clone()).collect::<Vec<_>>())?;
    let max = xs.iter().fold(0f64, |m, x| m.max(x.abs()));
    let label_w = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let lines = rows.iter().zip(&xs).map(|((label, v), x)| {
        let eighths = if max > 0.0 { (x.abs() / max * width as f64 * 8.0).round() as usize } else { 0 };
        let bar = "█".repeat(eighths / 8) + ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"][eighths % 8];
        format!("{label:>label_w$} │{bar} {v}")
    });
    Ok(lines.collect::<Vec<_>>().join("\n"))
}

fn tree(v: &Value, prefix: &str, out: &mut Vec<String>) {
    let items: Vec<(String, Value)> = match v {
        Value::Map(m) => m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Value::List(l) => l.borrow().iter().enumerate().map(|(i, v)| (format!("[{i}]"), v.clone())).collect(),
        _ => return,
    };
    let is_list = matches!(v, Value::List(_));
    for (i, (k, v)) in items.iter().enumerate() {
        let last = i == items.len() - 1;
        let nested = match v {
            Value::Map(m) => !m.borrow().is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            _ => false,
        };
        let label = match (nested, is_list) {
            (true, _) => k.clone(),
            (false, true) => v.to_string(),
            (false, false) => format!("{k}: {v}"),
        };
        out.push(format!("{prefix}{}{label}", if last { "└── " } else { "├── " }));
        if nested {
            tree(v, &format!("{prefix}{}", if last { "    " } else { "│   " }), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn charts() {
        assert_eq!(show("[1, 2, 3, 4, 5, 6, 7, 8].sparkline"), "▁▂▃▄▅▆▇█");
        assert_eq!(show("[5, 5].sparkline"), "▄▄");
        assert_eq!(show("bars({a: 2, bb: 4}, 4)"), " a │██ 2\nbb │████ 4");
        assert_eq!(show("[1, 3].bars(8)"), "0 │██▋ 1\n1 │████████ 3");
        // Quantities chart in one unit: 1 km outdraws 500 m.
        assert_eq!(show("[1 km, 500 m].bars(4)"), "0 │████ 1 km\n1 │██ 500 m");
        assert_eq!(show("[1 h, 30 min, 2 h].sparkline"), "▃▁█");
        assert!(try_eval("[1 km, 1 h].bars").is_err());
        assert_eq!(show("[1, 1, 2].histogram(2)"), format!("1–1.5 │{} 2\n1.5–2 │{} 1", "█".repeat(40), "█".repeat(20)));
        let p = show("plot(|x| x, 0, 1)");
        assert_eq!(p.lines().count(), 17);
        assert!(p.starts_with("1 ┤") && p.lines().nth(15).unwrap().starts_with("0 ┤"));
        assert!(try_eval(r#"["a"].sparkline"#).is_err());
        assert!(try_eval(r#"plot(|x| "a", 0, 1)"#).is_err());
        assert_eq!(show(r#"tree_view({a: {b: 1, c: [2, 3]}, d: "x"})"#), "├── a\n│   ├── b: 1\n│   └── c\n│       ├── 2\n│       └── 3\n└── d: x");
        assert_eq!(show("[[0, 1], [2, 4]].heatmap"), "  ░░\n▒▒██");
        assert!(try_eval("[1, 2].heatmap").is_err());
        assert_eq!(show("progress(0.5, 4)"), "▕██  ▏ 50%");
        assert_eq!(show("progress(2, 2)"), "▕██▏ 100%");
    }
}
