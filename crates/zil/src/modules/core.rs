//! Core: printing and paging, type conversions, parsing, help, `input`.

use super::data::tables;
use super::math::formatting::commas;
use super::time;
use super::units::{money, simplify};
use super::{Call, Claim, Doc, Fail, Module, doc};
use crate::ast::{BinOp, Expr, ExprKind, Radix, UnOp};
use crate::help::help;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::value::{Value, exact, fmt_float, num};
use num_bigint::BigInt;
use num_rational::BigRational;

pub const MODULE: Module = Module {
    name: "core",
    about: "values, printing, type conversions, parsing, help",
    #[rustfmt::skip]
    examples: &[
        ("core", &[
            ("type of anything", "type(5 km)"),
            ("parse values from text", r#"parse("[1, 2, 5 km]")"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("nil", "nil"), ("bool", "true"), ("int", "0xff"), ("float", "1.5e3"), ("frac", "7/2 to frac"), ("fn", r"|x| x * 2")]),
        ("names", &[("input  (stdin as a string): input.lines.len", "")]),
        ("conversions", &[("to str int float frac bool list", r#""42" to int"#)]),
        ("pretty", &[
            ("long: thousands separators, decimals kept", "pretty(1234567.891)"),
            ("short: K M B T, 3 significant figures", r#"pretty(1234567, "short")"#),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("values", &["type", "parse", "pretty"]),
        ("convert", &["str", "int", "float", "frac", "bool", "list"]),
        ("io", &["print", "page", "edit", "help", "tip"]),
    ],
    call,
    #[rustfmt::skip]
    targets: &[("str", "str"), ("int", "int"), ("float", "float"), ("frac", "frac"), ("bool", "bool"), ("list", "list")],
    ident: Some(ident),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("print", "print(a?: any, ...)", "print values separated by spaces", &[], &["str"]).shown(&[r#"print("total:", 5 km)"#]),
    doc("page", "page(a?: any, ...)", "print like print, but long output opens a scrollable pager (arrows, wheel, / to search, q to quit) when shown on a terminal", &[], &["print", "help"]).shown(&[r#"help("math") |> page"#, r#"(1..200).map(|n| n ** 2).join("\n") |> page"#]),
    doc("edit", "edit(text?: str)", "open a full-screen editor to type or paste text (Ctrl-S saves and returns it, Esc cancels with an error); starts from text if given", &[], &["page", "paste"]).shown(&["edit().lines.len", "edit(paste()).upper"]),
    doc("type", "type(v: any)", "the type name of a value", &["type(5 km)", r#"type("hi")"#, "type([1])"], &["str", "int", "float"]),
    doc("str", "str(v: any)", "convert to a string (full float precision)", &["str(1/3)", "str(5 km)"], &["int", "float"]),
    doc("int", "int(v: num|str) / int(s: str, base: int)", "convert to an integer, parsing strings in an optional base", &["int(3.9)", r#""ff".int(16)"#, r#""0b101".int"#], &["float", "str"]).pretty(),
    doc("float", "float(v: num|quantity|str)", "convert to a float; drops a quantity's unit", &[r#"float("2.5")"#, "float(5 km)"], &["int", "str"]).pretty(),
    doc("frac", "frac(v: num)", "show as a fraction; floats become the simplest fraction within 1e-12", &["7/2 to frac", "1/3 + 1/6 to frac", "0.75.frac"], &["float"]).pretty(),
    doc("bool", "bool(v: any)", "truthiness: false only for nil and false", &["bool(0)", "nil to bool"], &["str"]),
    doc("list", "list(v: str|list|map|set|table)", "convert to a list: characters, a copy, [key, value] pairs, a set's items, or a table's rows as maps", &[r#"list("abc")"#, r#""abc" to list"#, "{a: 1, b: 2}.list"], &["chars", "parse"]),
    doc("pretty", "pretty(v: any) / pretty(v: any, style: str)", "human-readable text; style is \"long\" (default) or \"short\"; help(int), help(date), help(set), help(table) and the quantity, uncertain, list and map module pages show each type's rules", &["pretty(1234567)", r#"pretty(1234567, "short")"#], &["str", "commas", "simplify", "parts", "format"]),
    doc("parse", "parse(s: str)", "read a zil literal (number, string, list, map, quantity); never runs code", &[r#""[1, 2.5, 0xff]".parse"#, r#""5 km".parse to m"#], &["str", "nums"]),
    doc("help", "help(topic?: any)", "this help, as text; topic is a function, module (\"trig\" or \"math.trig\"), unit, or any value to list functions for its type", &[], &[]).shown(&["help(upper)", r#"help("math.trig")"#, "help(today)", r#"help("text") |> grep("case")"#]),
    doc("tip", "tip()", "a random one-liner or recipe to try, from help(\"examples\") and help(\"advanced\")", &[], &["help"]).shown(&["tip()"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("print" | "page", vs) => {
            let parts: Vec<String> = vs.iter().map(|v| if let Table(t) = v { super::data::tables::grid(t, false) } else { v.to_string() }).collect();
            let text = parts.join(" ");
            crate::state::check_output(text.len() + 1).map_err(Fail::Msg)?;
            match name {
                "page" => crate::host::get().page(&text),
                _ => crate::print(&text),
            }
            Nil
        }
        ("edit", []) => Value::str(crate::host::get().edit("").map_err(|e| format!("edit: {}", super::fs::io_reason(&e)))?),
        ("edit", [Str(t)]) => Value::str(crate::host::get().edit(t).map_err(|e| format!("edit: {}", super::fs::io_reason(&e)))?),
        // Plain text, so it can be piped to `grep` etc. A miss returns its note: not finding help isn't an error in the user's code.
        ("help", []) => Value::str(help(None, false).unwrap_or_else(|e| e)),
        ("help", [Str(s)]) => Value::str(help(Some(s), false).unwrap_or_else(|e| e)),
        ("help", [Builtin(_, f)]) => Value::str(help(Some(f), false).unwrap_or_else(|e| e)),
        ("tip", []) => Value::str(crate::help::tip(false)),
        ("help", [Fn(_)]) => return Err(Fail::Arg(0, "no help for user-defined functions".into())),
        ("help", [v]) => {
            let t = v.type_name();
            Value::str(
                crate::help::capture(false, || crate::help::type_page(t)).unwrap_or_else(|_| format!("no functions take a {t} first; help() for an overview")),
            )
        }
        ("type", [v]) => Value::str(v.type_name()),
        ("str", [v @ (Float(_) | Frac(_, false))]) => Value::str(num(v).unwrap().to_string()),
        ("str", [v]) => Value::str(v.to_string()),
        ("int", [Int(n, _)]) => Value::int(*n),
        ("int", [Big(n, _)]) => Big(n.clone(), Radix::DEC),
        ("int", [Frac(r, _)]) => exact(BigRational::from_integer(r.to_integer()), Radix::DEC, false),
        ("int", [Float(n)]) => Value::int(*n as i64),
        ("int", [Str(s)]) => parse_int(s, None).ok_or_else(|| Fail::Arg(0, format!("cannot read {s:?} as an integer")))?,
        ("int", [Str(s), Int(b, _)]) if (2..=36).contains(b) => {
            parse_int(s, Some(*b as u32)).ok_or_else(|| Fail::Arg(0, format!("cannot read {s:?} as a base-{b} integer")))?
        }
        ("int", [Str(_), Int(b, _)]) => return Err(Fail::Arg(1, format!("base {b} is out of range\nnote: bases go from 2 to 36"))),
        ("float", [v]) if num(v).is_some() => Float(num(v).unwrap()),
        ("float", [Qty(n, _)]) => Float(*n),
        ("float", [Str(s)]) => Float(s.trim().parse().map_err(|_| Fail::Arg(0, format!("cannot read {s:?} as a number")))?),
        ("frac", [v @ (Int(..) | Big(..))]) => v.clone(),
        ("frac", [Frac(r, _)]) => Frac(r.clone(), true),
        ("frac", [Float(x)]) => exact(simplest(*x).ok_or_else(|| format!("`{x}` has no fraction"))?, Radix::DEC, true),
        ("bool", [v]) => Bool(v.truthy()),
        ("list", [Str(s)]) => Value::list(s.chars().map(|c| Value::str(c.to_string())).collect()),
        ("list", [List(l)]) => Value::list(l.borrow().clone()),
        ("list", [Table(t)]) => Value::list(t.maps()),
        ("list", [Set(s)]) => Value::list(s.borrow().iter().cloned().collect()),
        ("list", [Map(m)]) => Value::list(m.borrow().iter().map(|(k, v)| Value::list(vec![Value::str(k.as_str()), v.clone()])).collect()),
        ("parse", [Str(s)]) => parse_literal(s)?,
        ("pretty", [v]) => Value::str(pretty(v, false, 0, true)),
        ("pretty", [v, Str(s)]) => match &**s {
            "long" => Value::str(pretty(v, false, 0, true)),
            "short" => Value::str(pretty(v, true, 0, true)),
            _ => return Err(Fail::Arg(1, format!("unknown style {s:?}\nnote: styles are \"long\" and \"short\""))),
        },
        _ => return Err(Fail::BadArgs),
    })
}

/// `pretty(v)`: `depth` indents a long collection's lines, `top` leaves a bare string unquoted.
fn pretty(v: &Value, short: bool, depth: usize, top: bool) -> String {
    use Value::*;
    let n = |x: f64| if short { compact(x) } else { commas(&fmt_float(x)) };
    let items = |vs: Vec<String>, open: &str, close: &str| {
        if vs.is_empty() || short {
            return format!("{open}{}{close}", vs.join(", "));
        }
        let pad = "  ".repeat(depth + 1);
        format!("{open}\n{pad}{}\n{}{close}", vs.join(&format!("\n{pad}")), "  ".repeat(depth))
    };
    let inner = |v: &Value| pretty(v, short, depth + 1, false);
    match v {
        Int(..) | Big(..) | Frac(..) | Float(_) if short => compact(num(v).unwrap()),
        // Full precision, as `str` gives it.
        Float(_) | Frac(_, false) => commas(&num(v).unwrap().to_string()),
        Int(..) | Big(..) | Frac(..) => commas(&v.to_string()),
        Qty(x, u) if time::is_duration(u) => commas(&time::parts(u.to_si(*x), if short { 2 } else { 3 })),
        Qty(x, u) => match money::show(*x, u) {
            Some(s) => s,
            None => match simplify(*x, u) {
                Qty(x, u) if u.0.is_empty() => n(x),
                Qty(x, u) => format!("{} {u}", n(x)),
                v => v.to_string(),
            },
        },
        Unc(c) => {
            let (x, e, u) = &**c;
            // Same prefix as the value, error scaled to match.
            let (k, u) = match simplify(*x, u) {
                Qty(y, v) if *x != 0.0 && !u.0.is_empty() => (y / x, v),
                _ => (1.0, u.clone()),
            };
            let s = format!("{} ± {}", n(x * k), n(e * k));
            if u.0.is_empty() { s } else { format!("{s} {u}") }
        }
        Date(z) => {
            let f = if short { "%b %-d, %Y, %-I:%M %p" } else { "%A, %B %-d, %Y at %-I:%M %p" };
            let zone = if z.time_zone().iana_name() == crate::host::tz().iana_name() { "" } else { " %Z" };
            z.strftime(&format!("{f}{zone}")).to_string()
        }
        List(l) => items(l.borrow().iter().map(inner).collect(), "[", "]"),
        Set(s) => items(s.borrow().iter().map(inner).collect(), "set(", ")"),
        Map(m) => items(m.borrow().iter().map(|(k, v)| format!("{k}: {}", inner(v))).collect(), "{", "}"),
        Table(t) if short => format!("table({})", pretty(&Value::list(t.maps()), true, depth, false)),
        Table(t) => {
            // Strings stay bare and nil blank, as in any grid; nested collections go inline to keep rows one line.
            let cell = |v: &Value| match v {
                Nil | Str(_) => tables::cell(v),
                List(_) | Map(_) | Set(_) | Table(_) => pretty(v, true, 0, false),
                v => pretty(v, false, 0, false),
            };
            tables::grid_with(t, false, &cell).replace('\n', &format!("\n{}", "  ".repeat(depth)))
        }
        Str(s) if top => s.to_string(),
        v => format!("{v:?}"),
    }
}

/// `1.23M`: K M B T with 3 significant figures, scientific past that; under 1000 as is.
fn compact(x: f64) -> String {
    let round3 = |m: f64| {
        let p = 10f64.powi((2 - m.abs().log10().floor() as i32).max(0));
        (m * p).round() / p
    };
    if x.abs() < 1000.0 || !x.is_finite() {
        return fmt_float(x);
    }
    let mut i = (x.abs().log10() / 3.0).floor() as usize;
    let mut m = round3(x / 1000f64.powi(i as i32));
    // 999,999 rounds to 1000K: that's 1M.
    if m.abs() >= 1000.0 {
        i += 1;
        m = round3(x / 1000f64.powi(i as i32));
    }
    match ["", "K", "M", "B", "T"].get(i) {
        Some(suffix) => format!("{}{suffix}", fmt_float(m)),
        None => format!("{x:.2e}").replace(".00e", "e").replace("0e", "e"),
    }
}

/// `input`: all of stdin, read once.
fn ident(it: &mut Interp, name: &str) -> Claim {
    if name != "input" {
        return None;
    }
    if it.input.is_none() {
        match crate::host::get().stdin() {
            Ok(s) => it.input = Some(s.into()),
            Err(e) => return Some(Err(super::fs::io_reason(&e))),
        }
    }
    Some(Ok(Value::Str(it.input.clone().unwrap())))
}

/// Accepts `0x`/`0b`/`0o`/`36#` prefixes and `_` separators when no base is given.
fn parse_int(s: &str, base: Option<u32>) -> Option<Value> {
    let s = s.trim().replace('_', "");
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, s),
    };
    let (base, digits) = match base {
        Some(b) => (b, s.as_str()),
        None => match s.get(..2) {
            Some("0x" | "0X") => (16, &s[2..]),
            Some("0b" | "0B") => (2, &s[2..]),
            Some("0o" | "0O") => (8, &s[2..]),
            _ => match s.split_once('#') {
                Some((b, d)) => (b.parse().ok().filter(|b| (2..=36).contains(b))?, d),
                None => (10, s.as_str()),
            },
        },
    };
    if !digits.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let n = BigInt::parse_bytes(digits.as_bytes(), base)?;
    Some(exact(BigRational::from_integer(if neg { -n } else { n }), Radix { base, width: 0 }, false))
}

/// The simplest fraction within 1e-12 of `x` (relative), by continued fractions: 0.1 + 0.2 is 3/10.
fn simplest(x: f64) -> Option<BigRational> {
    if x.fract() == 0.0 || !x.is_finite() {
        return BigRational::from_float(x);
    }
    let (mut h, mut k) = ((1i128, 0i128), (0i128, 1i128)); // (current, previous) convergent numerators / denominators
    let mut f = x;
    loop {
        let a = f.floor();
        let next = (a as i128).checked_mul(h.0).and_then(|n| n.checked_add(h.1)).zip((a as i128).checked_mul(k.0).and_then(|d| d.checked_add(k.1)));
        let Some((n, d)) = next.filter(|_| a.abs() < 1e18) else {
            break;
        };
        (h, k) = ((n, h.0), (d, k.0));
        if (n as f64 / d as f64 - x).abs() <= x.abs() * 1e-12 || f == a {
            break;
        }
        f = 1.0 / (f - a);
    }
    Some(BigRational::new(h.0.into(), k.0.into()))
}

/// A single zil literal (number, string, list, map, quantity); anything that could run code is refused.
fn parse_literal(s: &str) -> Result<Value, String> {
    fn literal(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Nil
            | ExprKind::Bool(_)
            | ExprKind::Int(..)
            | ExprKind::Big(..)
            | ExprKind::Float(_)
            | ExprKind::Dec(_)
            | ExprKind::Str(_)
            | ExprKind::Regex(_) => true,
            ExprKind::List(items) => items.iter().all(literal),
            ExprKind::Map(entries) => entries.iter().all(|(_, v)| literal(v)),
            // `set(1, 2)`, as sets print.
            ExprKind::Call(f, args) => matches!(&f.kind, ExprKind::Ident(n) if n == "set") && args.iter().all(literal),
            ExprKind::Qty(n, _) | ExprKind::Unary(UnOp::Neg, n) => literal(n),
            // `5 ft 11 in`, as `to ft in` prints it.
            ExprKind::Binary(BinOp::Add, a, b) => matches!(a.kind, ExprKind::Qty(..)) && literal(a) && literal(b),
            _ => false,
        }
    }
    let ast = crate::parser::parse(s).map_err(|e| e.msg)?;
    match &ast[..] {
        [e] if literal(e) => Interp::new().run(&ast).map_err(|e| e.msg),
        _ => Err(format!("not a literal value: {s:?}")),
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn conversions() {
        assert_eq!(show(r#""ff".int(16)"#), "0xff");
        assert_eq!(show(r#""0b101".int"#), "0b101");
        assert_eq!(show(r#""z".int(36)"#), "36#z");
        assert_eq!(show(r#""-36#z".int to dec"#), "-35");
        assert_eq!(show(r#"list("ab")"#), r#"["a", "b"]"#);
        assert_eq!(show("{a: 1}.list"), r#"[["a", 1]]"#);
    }

    #[test]
    fn parse() {
        assert_eq!(show(r#""[1, -2.5, \"a\", 0xff, 5 km, \{k: [nil, true]}]".parse"#), r#"[1, -2.5, "a", 0xff, 5 km, {k: [nil, true]}]"#);
        assert_eq!(
            show(
                r#"x = [1, "b"]
str(x).parse == x"#
            ),
            "true"
        );
        for bad in [r#""read_file(\"x\")".parse"#, r#""[x]".parse"#, r#""1 + 2".parse"#, r#""".parse"#] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn pretty() {
        assert_eq!(show("pretty(1234567.891)"), "1,234,567.891");
        assert_eq!(show("pretty(-1234)"), "-1,234");
        assert_eq!(show(r#"pretty(1234567, "short")"#), "1.23M");
        assert_eq!(show(r#"pretty(999999, "short")"#), "1M");
        assert_eq!(show(r#"pretty(2 ** 100, "short")"#), "1.27e30");
        assert_eq!(show(r#"pretty(1.5e15, "short")"#), "1.5e15");
        assert_eq!(show(r#"pretty(999.9e12, "short")"#), "1e15");
        assert_eq!(show(r#"pretty(12.5, "short")"#), "12.5");
        assert_eq!(show("pretty(1500000 B)"), "1.5 MB");
        assert_eq!(show("pretty(3725 s)"), "1 h 2 min 5 s");
        assert_eq!(show(r#"pretty(3725 s, "short")"#), "1 h 2 min");
        assert_eq!(show("pretty(5 ± 0.1 km)"), "5 ± 0.1 km");
        assert_eq!(show(r#"pretty(date("2026-12-25"))"#), "Friday, December 25, 2026 at 12:00 AM");
        assert_eq!(show(r#"pretty(date("2026-12-25 18:30"), "short")"#), "Dec 25, 2026, 6:30 PM");
        assert_eq!(show(r#"pretty([1234, "a", {b: []}])"#), "[\n  1,234\n  \"a\"\n  {\n    b: []\n  }\n]");
        assert_eq!(show(r#"pretty([1234, "a", set(2000)], "short")"#), r#"[1.23K, "a", set(2K)]"#);
        assert_eq!(show(r#"pretty("hi")"#), "hi");
        assert!(try_eval(r#"pretty(1, "tiny")"#).is_err());
    }

    #[test]
    fn type_conversions() {
        assert_eq!(show(r#""ab" to list"#), r#"["a", "b"]"#);
        assert_eq!(show(r#""0xff" to int"#), "0xff");
        assert_eq!(show(r#""2.5" to float * 2"#), "5");
        assert_eq!(show("[1, 2] to str"), "[1, 2]");
        assert_eq!(show("0 to bool"), "true");
        assert_eq!(show("nil to bool"), "false");
        assert_eq!(show("5 km to float"), "5");
        assert!(try_eval("5 to list").is_err());
        assert_eq!(show(r#""hi there" to base64"#), "aGkgdGhlcmU=");
        assert_eq!(show(r#"base64("hi there")"#), "aGkgdGhlcmU=");
    }
}
