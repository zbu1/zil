//! Tables: rows under named columns, like nushell's. `ls`, `glob` and `from_csv` make them.

use crate::ansi::{RESET, Role};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::{self, Table, Value};

pub const MODULE: Module = Module {
    name: "tables",
    about: "rows of data under named columns, like a spreadsheet: filter rows, pick columns, sort",
    #[rustfmt::skip]
    intro: &[
        ("", "\
A table is a grid of rows under named columns, like a spreadsheet. Make one from a list of maps, where each map is a \
row and its keys are the column names."),
        ("good to know", "\
`ls`, `glob` and `from_csv` return tables. Functions for lists work on tables too, and see each row as a map."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("tables", &[
            ("a column as a list", "[{n: 1}, {n: 5}].table.n"),
            ("list functions see each row as a map", "[{n: 1}, {n: 5}].table.map(|r| r.n * 2)"),
            ("sort, then take one column", r#"[{f: "a", kb: 3}, {f: "b", kb: 9}].table.sort_by_desc("kb").f"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("table", "[{a: 1, b: 2}] to table")]),
        ("pretty", &[
            ("long, the default: a grid", r#"pretty([{a: 1234, b: "x"}].table)"#),
            ("short: all on one line, as a list of maps", r#"pretty([{a: 1234, b: "x"}].table, "short")"#),
        ]),
        ("access", &[("t.col: a column, as a list", "[{a: 1}, {a: 2}].table.a"), ("t[i]: a row, as a map", "[{a: 1}, {a: 2}].table[1]")]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("build", &["table"]),
        ("rows", &["where", "sort_by", "sort_by_desc"]),
        ("columns", &["select", "reject"]),
    ],
    call,
    targets: &[("table", "table")],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("table", "table(rows: list|table) -> table", "a table from a list of maps, one map per row", &["[{a: 1}, {a: 2, b: 3}].table", "[{a: 1}] to table"], &["select", "list"]).pretty()
        .detail("Every key that appears becomes a column, and rows without it get nil there"),
    doc("where", "where(t: table, f: fn) -> table", "keep only the rows that pass the test f, which gets each row as a map", &["[{n: 1}, {n: 5}].table.where(|r| r.n > 2)"], &["filter", "sort_by"]),
    doc("select", "select(t: table, col: str, ...) -> table", "keep only the named columns, in the order given", &[r#"[{a: 1, b: 2, c: 3}].table.select("c", "a")"#], &["reject"]),
    doc("reject", "reject(t: table, col: str, ...) -> table", "remove the named columns", &[r#"[{a: 1, b: 2, c: 3}].table.reject("b")"#], &["select"]),
    doc("sort_by", "sort_by(t: table, col: str) -> table", "the rows sorted by the values in a column, smallest first", &[r#"[{n: 3}, {n: 1}].table.sort_by("n")"#], &["sort_by_desc", "sort"]),
    doc("sort_by_desc", "sort_by_desc(t: table, col: str) -> table", "the rows sorted by the values in a column, largest first", &[r#"[{n: 1}, {n: 3}].table.sort_by_desc("n")"#], &["sort_by"]),
];

/// List fns that also take a table, seeing its rows as maps.
pub const LIFTED: &[&str] = &[
    "len",
    "first",
    "last",
    "take",
    "drop",
    "reverse",
    "sort",
    "sort_desc",
    "unique",
    "filter",
    "map",
    "any",
    "all",
    "reduce",
    "contains",
    "find",
    "count",
    "enumerate",
    "group_by",
    "count_by",
    "chunks",
    "windows",
    "step",
];

/// Lifted fns whose result, while still all maps, is a table again.
const KEEP: &[&str] = &["take", "drop", "reverse", "sort", "sort_desc", "unique", "filter", "map", "step"];

/// Calls `f` with the table in `args[0]` as a list of maps.
pub fn lift<E>(name: &str, args: &[Value], f: impl FnOnce(&[Value]) -> Result<Value, E>) -> Result<Value, E> {
    let Value::Table(t) = &args[0] else { unreachable!("caller checks for a table") };
    let mut a = args.to_vec();
    a[0] = Value::list(t.maps());
    let r = f(&a)?;
    Ok(match &r {
        // An empty result keeps the columns, so it still prints as the table it came from.
        Value::List(l) if KEEP.contains(&name) && l.borrow().is_empty() => Value::table(Table { cols: t.cols.clone(), rows: vec![] }),
        Value::List(l) if KEEP.contains(&name) => Table::from_maps(&l.borrow()).map_or(r.clone(), Value::table),
        _ => r,
    })
}

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("table", [t @ Table(_)]) => t.clone(),
        ("table", [List(l)]) => Value::table(value::Table::from_maps(&l.borrow()).map_err(|e| Fail::Arg(0, e))?),
        ("where", [Table(t), f]) => {
            let mut rows = Vec::new();
            for (i, r) in t.rows.iter().enumerate() {
                if it.call(f, vec![t.row(i)], span)?.truthy() {
                    rows.push(r.clone());
                }
            }
            Value::table(value::Table { cols: t.cols.clone(), rows })
        }
        ("select" | "reject", [Table(t), cols @ ..]) => {
            let mut picked = Vec::new();
            for (i, c) in cols.iter().enumerate() {
                let Str(c) = c else { return Err(Fail::BadArgs) };
                picked.push(column(t, c, i + 1)?);
            }
            let keep: Vec<usize> = if name == "select" { picked } else { (0..t.cols.len()).filter(|c| !picked.contains(c)).collect() };
            Value::table(value::Table {
                cols: keep.iter().map(|&c| t.cols[c].clone()).collect(),
                rows: t.rows.iter().map(|r| keep.iter().map(|&c| r[c].clone()).collect()).collect(),
            })
        }
        ("sort_by" | "sort_by_desc", [Table(t), Str(c)]) => {
            let c = column(t, c, 1)?;
            let mut keyed: Vec<_> = t.rows.iter().enumerate().map(|(i, r)| (r[c].clone(), Value::int(i as i64))).collect();
            let Value::List(order) = super::sort_keyed(&mut keyed, name == "sort_by_desc")? else { unreachable!() };
            let rows = order.borrow().iter().map(|i| if let Int(i, _) = i { t.rows[*i as usize].clone() } else { unreachable!() }).collect();
            Value::table(value::Table { cols: t.cols.clone(), rows })
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// The index of column `name`, blaming argument `arg` if there's none.
fn column(t: &Table, name: &str, arg: usize) -> Result<usize, Fail> {
    t.cols.iter().position(|c| c == name).ok_or_else(|| {
        let hint = crate::error::did_you_mean(name, t.cols.iter().map(String::as_str));
        Fail::Arg(arg, format!("no column `{name}`{hint}"))
    })
}

/// A cell as the grid shows it: strings bare, nil blank, local times to the minute.
pub fn cell(v: &Value) -> String {
    match v {
        Value::Nil => String::new(),
        Value::Date(z) if z.time_zone().iana_name() == crate::host::tz().iana_name() && z.time() != jiff::civil::Time::midnight() => {
            z.strftime("%Y-%m-%d %H:%M").to_string()
        }
        v => v.to_string(),
    }
}

/// The table as a grid for people: a rule under the header and around the rows, no vertical lines,
/// numbers right-aligned, cells colored by type when `color`.
// ponytail: widths count chars, so wide (CJK, emoji) cells misalign; use unicode-width if that matters.
pub fn grid(t: &Table, color: bool) -> String {
    grid_with(t, color, &cell)
}

/// `grid`, with each cell shown by `cell`.
pub fn grid_with(t: &Table, color: bool, cell: &dyn Fn(&Value) -> String) -> String {
    let paint = |r: Role, s: &str| if color && r != Role::Plain { format!("{}{s}{RESET}", r.code()) } else { s.to_string() };
    let n = t.cols.len();
    let numeric: Vec<bool> = (0..n)
        .map(|c| {
            let mut vs = t.rows.iter().map(|r| &r[c]).filter(|v| !matches!(v, Value::Nil)).peekable();
            vs.peek().is_some() && vs.all(|v| crate::ansi::tint(v) == Role::Number)
        })
        .collect();
    // One long cell (a description, a list) would otherwise pad its whole column: text past MAX ends in `…`.
    const MAX: usize = 32;
    let fit = |c: usize, s: String| if numeric[c] || s.chars().count() <= MAX { s } else { s.chars().take(MAX - 1).chain(['…']).collect() };
    let cells: Vec<Vec<String>> = t.rows.iter().map(|r| r.iter().enumerate().map(|(c, v)| fit(c, cell(v))).collect()).collect();
    let width = |c: usize| cells.iter().map(|r| r[c].chars().count()).chain([t.cols[c].chars().count()]).max().unwrap_or(0);
    let widths: Vec<usize> = (0..n).map(width).collect();
    let iw = t.rows.len().saturating_sub(1).to_string().len();
    let line = |idx: &str, row: &[(String, Role)]| {
        let mut s = format!(" {}", paint(Role::Dim, &format!("{idx:>iw$}")));
        for (c, (text, tint)) in row.iter().enumerate() {
            let pad = " ".repeat(widths[c] - text.chars().count());
            let last = c + 1 == n;
            s += "  ";
            s += &match numeric[c] {
                true => format!("{pad}{}", paint(*tint, text)),
                false if last => paint(*tint, text),
                false => format!("{}{pad}", paint(*tint, text)),
            };
        }
        s
    };
    let total = 1 + iw + widths.iter().map(|w| w + 2).sum::<usize>();
    let rule = paint(Role::Dim, &"─".repeat(total));
    let mut out = vec![line("#", &t.cols.iter().map(|c| (c.clone(), Role::TableHeader)).collect::<Vec<_>>()), rule.clone()];
    for (i, (r, cs)) in t.rows.iter().zip(cells).enumerate() {
        out.push(line(&i.to_string(), &cs.into_iter().zip(r.iter().map(crate::ansi::tint)).collect::<Vec<_>>()));
    }
    let plural = |k: usize, w: &str| format!("{k} {w}{}", if k == 1 { "" } else { "s" });
    out.push(rule);
    out.push(paint(Role::Dim, &format!(" {} · {}", plural(t.rows.len(), "row"), plural(n, "column"))));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn tables() {
        let t = r#"t = [{name: "b", n: 3}, {name: "a", n: 1}, {name: "c"}].table; "#;
        let run = |s: &str| show(&format!("{t}{s}"));
        assert_eq!(run("type(t)"), "table");
        assert_eq!(run("[t.n, t[1], t.len, t.first.name]"), r#"[[3, 1, nil], {name: "a", n: 1}, 3, "b"]"#);
        assert_eq!(run(r#"t.where(|r| r.n).sort_by("n").name"#), r#"["a", "b"]"#);
        assert_eq!(run(r#"[type(t.take(1)), type(t.map(|r| r.n)), t.select("n").n, t.reject("n").name.len]"#), r#"["table", "list", [3, 1, nil], 3]"#);
        assert_eq!(run(r#"t.filter(|r| false).select("n") to list"#), "[]");
        assert_eq!(run("str(t)"), "name,n\nb,3\na,1\nc,\n");
        assert_eq!(run(r#"x = 0; for r in t { if r.n { x = x + r.n } }; x"#), "4");
        assert!(try_eval(&format!(r#"{t}t.select("nme")"#)).unwrap_err().msg.contains("no column `nme`"));
        assert!(try_eval("[1].table").is_err());
    }

    #[test]
    fn grid() {
        let crate::value::Value::Table(t) = crate::interp::tests::eval(r#"[{name: "a", n: 10}, {name: "bb", n: 2}].table"#) else { panic!() };
        assert_eq!(super::grid(&t, false), " #  name   n\n────────────\n 0  a     10\n 1  bb     2\n────────────\n 2 rows · 2 columns");
        let crate::value::Value::Table(t) = crate::interp::tests::eval(r#"[{s: "x".repeat(40)}].table"#) else { panic!() };
        assert!(super::grid(&t, false).contains(&format!("{}…\n", "x".repeat(31))));
    }
}
