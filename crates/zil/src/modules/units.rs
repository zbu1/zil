//! Units: quantities with dimensions, arithmetic and conversion between them, live currency rates, decibels.

pub mod constants;
pub mod informal;
pub mod kitchen;
pub mod money;

mod dims;
mod rates;
mod table;

pub use dims::*;
use rates::{CURRENCIES, currency, load_rates_once, rate_error};
pub use table::{Row, TABLE};

use crate::ast::{BinOp, Target, UnitSpec};
use crate::modules::{Claim, Module};
use crate::ops::mismatch;
use crate::value::{Table, Value, num};
use indexmap::IndexMap;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "units",
    about: "numbers with units like km, kg or mph: calculate with them and convert between them",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Numbers with units, like `5 km` or `60 km/h`. Units carry through arithmetic, so a distance divided by a time is a \
speed, and `to` converts to another unit."),
        ("writing", "\
Units combine with `*`, `/` and `^`, like `m/s^2`. Most take the metric prefixes: k for kilo, M for mega, m for milli, \
µ or u for micro, and so on."),
        ("good to know", "\
Adding or comparing quantities of different kinds, like meters and seconds, is an error rather than a meaningless \
number."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("units", &[
            ("speed from distance and time", "100 km / 2 h to mph"),
            ("feet and inches", "1.8 m to ft in"),
            ("feet and inches written together", "5 ft 11 in to cm"),
            ("300 miles at 65 mph", "300 mi / 65 mph to h min"),
            ("downloading 4 GB at 100 Mbps", "4 GB / 100 Mbps to min"),
            ("why a 1 TB disk shows less: GiB are bigger than GB", "1 TB to GiB"),
            ("area", "3 m * 4 m to ft^2"),
            ("a 10 km run in an hour, as minutes per mile", "1 h / 10 km to min/mi"),
            ("the force of 70 kg under gravity", "9.81 m/s^2 * 70 kg to N"),
            ("a month of a 60 W bulb", "60 W * 8 h * 30 to kWh"),
            ("a fever in Celsius", "101 F to C"),
            ("a 1080p screen at 96 dpi", "1080 px to in"),
            ("twice the power, in decibels", "db(2)"),
            ("how much more power +3 dB is", "from_db(3)"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("quantity: a number with a unit", "5 km")]),
        ("operators", &[
            ("+ and - convert to a common unit", "1 km + 300 m"),
            ("* and / combine the units", "100 km / 2 h"),
            ("** raises the unit too", "(3 m) ** 2"),
            ("< and > compare across units", "1 mi > 1 km"),
        ]),
        ("pretty", &[
            ("long, the default: best prefix, separators", "pretty(12345678 m)"),
            ("short: K, M, B and T for thousands up to trillions", r#"pretty(12345678 m, "short")"#),
            ("durations and money have their own styles, shown in help(\"time\") and help(\"money\")", ""),
        ]),
        ("conversions", &[("to a unit", "5 km to mi"), ("in works the same as to", "5 km in mi"), ("to several units splits it across them", "1.8 m to ft in"),
            ("to a combined unit", "100 km / 2 h to mph")]),
        // Shown, not run: `unit` would define them in the session running help.
        ("your own units", &[
            ("unit sprint = 2 wk defines a unit from others, then 3 sprint to d gives 42 d", ""),
            ("unit pizza on its own makes a new unit for counting things, unrelated to the rest", ""),
            ("unit slice = pizza / 8 builds on it, then 3 slice to pizza gives 0.375 pizza", ""),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("best", "simplify"), ("si", "si")],
    ident: Some(|_, name| match unit(name) {
        Ok(u) => Some(Ok(Value::Qty(1.0, u))),
        Err(e) if e.starts_with("unknown unit") => None,
        Err(e) => Some(Err(e)),
    }),
    binary: Some(binary),
    compare: Some(compare),
    convert: Some(convert),
    topic: Some(topic),
    units: TABLE,
    children: &[constants::MODULE, money::MODULE, informal::MODULE, kitchen::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Unit, all_units as table, base_terms, convert_value, simplify as best, unit};
    use crate::modules::Fail;
    use crate::signature::{As, Num, Or, Qty};
    use crate::value::{Table, Value};

    fn amplitude(kind: &str) -> Result<bool, Fail> {
        match kind {
            "amplitude" => Ok(true),
            "power" => Ok(false),
            _ => Err(Fail::Arg(1, "kind must be \"power\" or \"amplitude\"".into())),
        }
    }

    /// the quantity in the prefix of its unit that reads best, like 3.6 kW instead of 3600 W
    ///
    /// Also written `to best`
    #[builtin(examples = ["0.0012 km to best", "3600 J/s to best", "1 kg*m^2/s^2 to best", "1.5e9 B.simplify"])]
    fn simplify(q: Qty) -> As<Or<Num, Qty>> {
        let (x, u) = q.get();
        best(x, u).into()
    }

    /// the quantity in standard metric (SI) units
    ///
    /// A named one like J (joules) or N (newtons) if there is one, otherwise base units like m, kg and s. Also written `to si`
    #[builtin(examples = ["5 mi/h to si", "2 kWh to si", "72 F to si"], see = ["simplify"])]
    fn si(q: Qty) -> Result<As<Or<Num, Qty>>, String> {
        let (x, u) = q.get();
        let si = base_terms(&u.dim()).iter().try_fold(Unit::default(), |acc, (n, p)| Ok::<_, String>(acc.mul(&unit(n)?.pow(*p), 1)))?;
        // Dimensionless (`3 dozen`) comes out a plain number.
        Ok(Value::qty(convert_value(x, u, &si)?, si).into())
    }

    /// every unit zil knows, as a table
    ///
    /// Its columns are name, full name, description, other names, kind (like length), size in SI units, and source: "units", "informal", "currency" or "user" for ones you defined
    #[builtin(
        examples = ["all_units().len", r#"all_units().filter(|u| u.kind == "length").map(|u| u.name).take(5)"#, r#"all_units().filter(|u| u.source == "informal")[0]"#],
        see = ["simplify"]
    )]
    fn all_units() -> As<Table> {
        table().into()
    }

    /// a ratio of two powers in decibels, as 10 × log10(ratio), so doubling is about +3 dB
    ///
    /// For a ratio of voltages or sound pressures, pass "amplitude", which uses 20 × log10
    #[builtin(examples = ["db(2)", "db(1000)", r#"db(2, "amplitude")"#], see = ["from_db", "log"])]
    fn db(ratio: f64, #[default("power")] kind: &str) -> Result<f64, Fail> {
        if ratio <= 0.0 {
            return Err(Fail::Arg(0, format!("the ratio must be positive, got {ratio}")));
        }
        Ok(if amplitude(kind)? { 20.0 } else { 10.0 } * ratio.log10())
    }

    /// decibels back to the ratio of powers, the reverse of db
    ///
    /// With "amplitude", the ratio of voltages or sound pressures
    #[builtin(examples = ["from_db(3)", "from_db(-6, \"amplitude\")"], see = ["db"])]
    fn from_db(db: f64, #[default("power")] kind: &str) -> Result<f64, Fail> {
        Ok(10f64.powf(db / if amplitude(kind)? { 20.0 } else { 10.0 }))
    }
}

/// `x u` in the table unit whose scale is a power of ten times `u`'s (so bytes stay bytes, feet stay feet)
/// and gives the biggest value of at least 1. Temperatures, money and units with no such family stay as they are.
pub fn simplify(x: f64, u: &Unit) -> Value {
    let dim = u.dim();
    if u.offset() != 0.0 || dim[8] != 0 || x == 0.0 {
        return Value::Qty(x, u.clone());
    }
    let si = u.to_si(x).abs();
    let family = |row: &&Row| {
        let r = (row.1 / u.scale()).log10();
        row.3 == dim && row.2 == 0.0 && (r - r.round()).abs() < 1e-9
    };
    let rows: Vec<&Row> = TABLE.iter().filter(family).collect();
    let best = rows.iter().filter(|r| r.1 <= si * (1.0 + 1e-12)).max_by(|a, b| a.1.total_cmp(&b.1)).or_else(|| rows.iter().min_by(|a, b| a.1.total_cmp(&b.1)));
    match best {
        // Already in a unit of that size (`5 cd` stays candelas, not lumens).
        Some(row) if (row.1 / u.scale() - 1.0).abs() > 1e-12 || u.0.len() > 1 => {
            let t = unit(row.0.split(' ').next().unwrap()).expect("table unit");
            Value::Qty(u.to_si(x) / t.scale(), t)
        }
        None if u.0.len() > 1 => merge(x, u),
        _ => Value::Qty(x, u.clone()),
    }
}

/// With no table unit for the whole dimension, terms of one kind join the first (`s*min/tile` is
/// `s^2/tile`) and the lead term takes its best prefix (`10 m*km/tile` is `1 ha/tile`).
fn merge(x: f64, u: &Unit) -> Value {
    let mut terms: Vec<(Rc<UnitDef>, i8)> = vec![];
    for (d, p) in &u.0 {
        match terms.iter_mut().find(|(t, _)| t.dim == d.dim && d.dim != NONE) {
            Some(t) => t.1 += p,
            None => terms.push((d.clone(), *p)),
        }
    }
    terms.retain(|(_, p)| *p != 0);
    let merged = Unit(terms);
    let x = x * u.scale() / merged.scale();
    let Some((lead, rest)) = merged.0.split_first() else { return Value::qty(x, merged) };
    match simplify(x, &Unit(vec![lead.clone()])) {
        Value::Qty(y, t) => Value::qty(y, t.mul(&Unit(rest.to_vec()), 1)),
        _ => Value::qty(x, merged),
    }
}

fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::*;
    // `3 dozen + 1`: a dimensionless quantity adds to a plain number as its value.
    let plain = |v: &Value| match v {
        Qty(x, u) if u.dim() == NONE => Some(x * u.scale()),
        _ => num(v),
    };
    Some(Ok(match (op, a, b) {
        (BinOp::Add | BinOp::Sub, Qty(_, u), n) | (BinOp::Add | BinOp::Sub, n, Qty(_, u)) if u.dim() == NONE && num(n).is_some() => {
            let (x, y) = (plain(a).unwrap(), plain(b).unwrap());
            Float(if op == BinOp::Add { x + y } else { x - y })
        }
        (BinOp::Add | BinOp::Sub, Qty(x, u), Qty(y, w)) => {
            if u.dim() != w.dim() {
                return Some(Err(mismatch(op, a, b)));
            }
            let y = match convert_value(*y, w, u) {
                Ok(y) => y,
                Err(e) => return Some(Err(e)),
            };
            Value::qty(if op == BinOp::Add { x + y } else { x - y }, u.clone())
        }
        (BinOp::Mul | BinOp::Div, Qty(..), _) | (BinOp::Mul | BinOp::Div, _, Qty(..)) => {
            let split = |v: &Value| match v {
                Qty(x, u) => Some((*x, u.clone())),
                _ => Some((num(v)?, Unit::default())),
            };
            let (Some((x, u)), Some((y, w))) = (split(a), split(b)) else {
                return Some(Err(mismatch(op, a, b)));
            };
            let u = u.mul(&w, if op == BinOp::Mul { 1 } else { -1 });
            // USD/EUR is a plain number, but only at today's rate.
            if !u.0.is_empty() && u.dim() == NONE && u.scale().is_nan() {
                return Some(Err(rate_error()));
            }
            let (v, u) = named(if op == BinOp::Mul { x * y } else { x / y }, u);
            Value::qty(v, u)
        }
        (BinOp::Pow, Qty(x, u), Int(n, _)) if (-9..=9).contains(n) => Value::qty(x.powi(*n as i32), u.pow(*n as i8)),
        _ => return None,
    }))
}

/// A product of metric units as its named SI unit: `mA*kΩ` is `V`, `kg*m/s^2` is `N`. Anything with
/// non-metric terms (`W*h`), money, data or angle keeps the units as typed.
fn named(x: f64, u: Unit) -> (f64, Unit) {
    // ponytail: "metric" = power-of-ten factor to SI; fine for TABLE, revisit if a unit like 1e3 B joins it.
    let metric = |d: &UnitDef| d.offset == 0.0 && (d.scale().log10() - d.scale().log10().round()).abs() < 1e-9;
    let dim = u.dim();
    if matches!(u.0.as_slice(), [] | [(_, 1)]) || dim[7..].iter().any(|x| *x != 0) || !u.0.iter().all(|(d, _)| metric(d)) {
        return (x, u);
    }
    match TABLE.iter().find(|row| row.1 == 1.0 && row.2 == 0.0 && row.3 == dim) {
        Some(row) => (x * u.scale(), unit(row.0.split(' ').next().unwrap()).unwrap()),
        None => (x, u),
    }
}

fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Qty(x, u), Value::Qty(y, w)) if u.dim() == w.dim() => x.partial_cmp(&convert_value(*y, w, u).ok()?),
        _ => None,
    }
}

/// `to km`, and `to ft in` as a mixed-unit string.
fn convert(v: &Value, t: &Target) -> Claim {
    let r = match (v, t) {
        (Value::Qty(x, u), Target::Units(specs)) => {
            specs.iter().map(target_of).collect::<Result<Vec<_>, _>>().and_then(|us| match us.iter().find(|t| t.dim() != u.dim()) {
                Some(t) => Err(format!("cannot convert {} to {}", describe(u, None), describe(t, None))),
                None => Ok(Value::str(split(u.to_si(*x), &us))),
            })
        }
        (Value::Qty(x, u), Target::Unit(spec)) => target_of(spec).and_then(|t| to_unit(*x, u, t)),
        // `36 to dozen`, `0.002 to ppm`: a plain number converts to a dimensionless unit.
        (v, Target::Unit(spec)) if num(v).is_some() && target_of(spec).is_ok_and(|t| t.dim() == NONE) => {
            to_unit(num(v).unwrap(), &Unit::default(), target_of(spec).unwrap())
        }
        (v, Target::Unit(_) | Target::Units(_)) if num(v).is_some() => {
            if let Err(e) = match t {
                Target::Unit(spec) => target_of(spec).map(drop),
                Target::Units(specs) => specs.iter().try_for_each(|s| target_of(s).map(drop)),
                _ => Ok(()),
            } {
                return Some(Err(e));
            }
            let to = match t {
                Target::Unit(spec) => spec_name(spec),
                Target::Units(specs) => specs.iter().map(spec_name).collect::<Vec<_>>().join(" "),
                _ => unreachable!(),
            };
            Err(format!("cannot convert a plain number to `{to}`\nhelp: attach a unit first, like `{v} km to {to}`"))
        }
        _ => return None,
    };
    Some(r)
}

/// A conversion target, where a physical constant also counts as a unit: `to c`, `to m_e`.
pub fn target_of(spec: &UnitSpec) -> Result<Unit, String> {
    let mut u = Unit::default();
    for (name, p) in spec {
        u = u.mul(&unit(name).or_else(|e| constant(name).ok_or(e))?.pow(*p), 1);
    }
    Ok(u)
}

/// A constant from `units.constants` as a unit of its own size, named after it.
fn constant(name: &str) -> Option<Unit> {
    let (_, x, ts) = constants::MODULE.quantities.iter().find(|q| q.0 == name)?;
    let u = terms(ts).ok()?;
    Some(Unit(vec![(Rc::new(UnitDef { name: name.into(), scale: Cell::new(x * u.scale()), offset: 0.0, dim: u.dim() }), 1)]))
}

/// `length (km)`, or just `` `km*kg` `` for a kind with no name.
pub fn describe(u: &Unit, written: Option<&str>) -> String {
    let name = written.map_or_else(|| u.to_string(), str::to_string);
    match DIMS.iter().find(|d| d.1 == u.dim()) {
        Some((kind, _)) => format!("{kind} ({name})"),
        None => format!("`{name}`"),
    }
}

/// A unit spec as written: `km/h`, `m^2`.
pub fn spec_name(spec: &UnitSpec) -> String {
    product(spec.iter().map(|(n, p)| (n.as_str(), *p)))
}

/// `a*b/c^2` from (name, power) terms.
fn product<'a>(terms: impl Iterator<Item = (&'a str, i8)>) -> String {
    let (num, den): (Vec<_>, Vec<_>) = terms.partition(|t| t.1 > 0);
    let term = |(n, p): &(&str, i8)| if p.abs() == 1 { n.to_string() } else { format!("{n}^{}", p.abs()) };
    let (num, den): (Vec<_>, Vec<_>) = (num.iter().map(term).collect(), den.iter().map(term).collect());
    match (num.is_empty(), den.is_empty()) {
        (_, true) => num.join("*"),
        (true, false) => format!("1/{}", den.join("/")),
        _ => format!("{}/{}", num.join("*"), den.join("/")),
    }
}

pub fn unit_of(spec: &UnitSpec) -> Result<Unit, String> {
    let mut u = Unit::default();
    for (name, p) in spec {
        u = u.mul(&unit(name)?.pow(*p), 1);
    }
    Ok(u)
}

/// `help("units")`, `help("length")`, `help("km")`, `help("currency")`.
fn topic(topic: &str) -> bool {
    use crate::ansi::Role;
    use crate::help::{code, eval, names, out, pad, paint, show};
    if topic == "units" {
        let w = DIMS.iter().map(|d| d.0.len()).max().unwrap_or(0);
        out!("  {}{}  3-letter codes (USD EUR GBP ...), live rates fetched when converting", paint(Role::Dim, "currency"), pad("currency", w));
        out!("{}", paint(Role::Dim, r#"help("length") or help("km") for details"#));
    } else if topic == "currency" {
        out!("{}: 3-letter codes like USD, EUR, GBP, JPY. Rates by Exchange Rate API (exchangerate-api.com),", paint(Role::Name, "currency"));
        out!("are fetched only when two currencies meet, and cached for a day.");
        out!("  {}", code("100 USD to EUR"));
    } else if let Some(kind) = DIMS.iter().map(|d| d.0).find(|k| *k == topic) {
        let units = units_of(kind);
        out!("{}: {}", paint(Role::Heading, &format!("{kind} units")), names(&units, " "));
        show(vec![format!("1 {} to {}", units[1], units[0]), format!("1 {} to {}", units[0], units[units.len() - 1])]);
        out!("{}", paint(Role::Dim, r#"help("km") etc. for one unit"#));
    } else if let Some((names, scale, offset, dim, about)) = rows().find(|row| row.0.split(' ').any(|n| n == topic)) {
        let mut names = names.split(' ');
        let name = names.next().unwrap();
        let (full, desc) = about.split_once(": ").expect("unit description is \"full name: description\"");
        let kind = DIMS.iter().find(|d| d.1 == *dim).map(|d| d.0);
        out!("{}{}", paint(Role::Name, name), if full == name { String::new() } else { format!(": {full}") });
        out!("  {desc}");
        let row = |label: &str, text: String| out!("  {}  {text}", paint(Role::Dim, &format!("{label:<6}")));
        // An informal-only kind (gee) has no name and no real units to compare with.
        let Some(kind) = kind else {
            row("equals", code(&format!("1 {name} = {}", eval(&format!("1 {name} to {}", base_name(dim))))));
            return true;
        };
        row("kind", kind.to_string());
        let aliases: Vec<_> = names.collect();
        if !aliases.is_empty() {
            row("also", aliases.join(", "));
        }
        // The SI or named base unit of the kind; temperatures show their zero, since 1 C is not 1 K apart from 0 C.
        let base = base_name(dim);
        if base.is_empty() {
            row("equals", code(&format!("1 {name} = {}", crate::value::fmt_float(*scale))));
        } else if base != name {
            let x = if *offset != 0.0 { 0 } else { 1 };
            row("equals", code(&format!("{x} {name} = {}", eval(&format!("{x} {name} to {base}")))));
        }
        // Examples convert to the closest-sized everyday unit, not the base unit `equals` already shows.
        let closeness = |r: &&(&str, f64)| (r.1 / scale).log10().abs();
        let others: Vec<_> = TABLE.iter().filter(|r| r.3 == *dim).map(|r| (r.0.split(' ').next().unwrap(), r.1)).filter(|r| r.0 != name).collect();
        let other = others.iter().filter(|r| r.0 != base).min_by(|a, b| closeness(a).total_cmp(&closeness(b))).or(others.first()).unwrap().0;
        out!("");
        show(vec![format!("1 {name} to {other}"), format!("1 {other} to {name}")]);
        out!("{}", paint(Role::Dim, &format!("all {kind} units: help(\"{kind}\")")));
    } else if let Some((_, desc)) = user_defs(|u| u.borrow().iter().find(|(n, _)| n == topic).cloned()) {
        match desc.as_str() {
            "" => out!("{}: a base unit you defined with `unit {topic}`", paint(Role::Name, topic)),
            _ => out!("{}: a unit you defined, 1 {topic} = {desc}", paint(Role::Name, topic)),
        }
    } else {
        return false;
    }
    true
}

/// `rows` grouped by kind, in `DIMS` order, leaving out kinds with none.
pub fn kinds(rows: &'static [Row]) -> Vec<(&'static str, Vec<&'static Row>)> {
    DIMS.iter().map(|(kind, dim)| (*kind, rows.iter().filter(|r| r.3 == *dim).collect::<Vec<_>>())).filter(|k| !k.1.is_empty()).collect()
}

/// `all_units()`: every unit as a table of {name, full, desc, aliases, kind, si, source}. Never fetches rates, so
/// a currency's `si` is nil until they're loaded.
fn all_units() -> Value {
    let entry = |name: &str, full: &str, desc: &str, aliases: Vec<&str>, scale: f64, dim: &Dim, source: &str| {
        let kind = match DIMS.iter().find(|d| d.1 == *dim) {
            Some(d) => d.0.to_string(),
            None if *dim == MONEY => "money".into(),
            None if *dim == NONE => "number".into(),
            None => base_name(dim),
        };
        let si = if scale.is_nan() {
            Value::Nil
        } else {
            let base = base_terms(dim).iter().fold(Unit::default(), |u, (n, p)| u.mul(&unit(n).expect("base unit").pow(*p), 1));
            Value::qty(scale, base)
        };
        Value::map(IndexMap::from([
            ("name".into(), Value::str(name)),
            ("full".into(), Value::str(full)),
            ("desc".into(), Value::str(desc)),
            ("aliases".into(), Value::list(aliases.into_iter().map(Value::str).collect())),
            ("kind".into(), Value::str(kind)),
            ("si".into(), si),
            ("source".into(), Value::str(source)),
        ]))
    };
    let mut maps = Vec::new();
    for (source, table) in [("units", TABLE), ("informal", informal::TABLE)] {
        for (names, scale, _, dim, about) in table {
            let mut names = names.split(' ');
            let name = names.next().unwrap();
            let (full, desc) = about.split_once(": ").expect("unit description is \"full name: description\"");
            maps.push(entry(name, full, desc, names.collect(), *scale, dim, source));
        }
    }
    let user = user_defs(|u| u.borrow().clone());
    // A currency is registered under its code and its lowercase alias; user units can be money too.
    let mut codes: Vec<_> = registry(|r| {
        r.borrow().iter().filter(|(k, u)| u.dim == MONEY && **k == u.name && !user.iter().any(|(n, _)| n == *k)).map(|(_, u)| u.clone()).collect()
    });
    codes.sort_by(|a, b| a.name.cmp(&b.name));
    for def in codes {
        maps.push(entry(&def.name, &def.name, "currency", vec![&def.name.to_lowercase()], def.scale.get(), &def.dim, "currency"));
    }
    for (name, desc) in &user {
        let def = lookup(name).expect("user unit is registered");
        maps.push(entry(name, name, desc, vec![], def.scale.get(), &def.dim, "user"));
    }
    Value::table(Table::from_maps(&maps).expect("all maps, few cells"))
}

/// The unit a kind is defined in, as (name, power) terms: its named SI unit (`J`, `lx`), else SI and user bases.
fn base_terms(dim: &Dim) -> Vec<(String, i8)> {
    if let Some(row) = TABLE.iter().find(|r| r.1 == 1.0 && r.2 == 0.0 && r.3 == *dim) {
        return vec![(row.0.split(' ').next().unwrap().to_string(), 1)];
    }
    const BASES: [&str; 11] = ["m", "kg", "s", "A", "K", "mol", "cd", "bit", "EUR", "rad", "Sv"];
    // User bases take slots in the order they were made.
    let user = user_defs(|u| u.borrow().iter().filter(|(_, d)| d.is_empty()).map(|(n, _)| n.clone()).collect::<Vec<_>>());
    BASES.iter().map(|n| n.to_string()).chain(user).zip(dim).filter(|(_, p)| **p != 0).map(|(n, p)| (n, *p)).collect()
}

/// The unit a kind is defined in, written out: `J`, `m/s`, `bit/s`, `pizza`.
fn base_name(dim: &Dim) -> String {
    let terms = base_terms(dim);
    // `to 1/m` doesn't parse, `to m^-1` does.
    if terms.iter().all(|t| t.1 < 0) {
        return terms.iter().map(|(n, p)| format!("{n}^{p}")).collect::<Vec<_>>().join("*");
    }
    product(terms.iter().map(|(n, p)| (n.as_str(), *p)))
}

fn units_of(kind: &str) -> Vec<&'static str> {
    let dim = DIMS.iter().find(|d| d.0 == kind).unwrap().1;
    TABLE.iter().filter(|row| row.3 == dim).map(|row| row.0.split(' ').next().unwrap()).collect()
}

#[derive(Debug)]
pub struct UnitDef {
    pub name: String,
    /// NaN for a currency whose rate isn't loaded yet; read it through `scale()`.
    scale: Cell<f64>,
    pub offset: f64,
    pub dim: Dim,
}

/// A product of units with integer powers, e.g. km/h = [(km, 1), (h, -1)].
#[derive(Debug, Clone, Default)]
pub struct Unit(pub Vec<(Rc<UnitDef>, i8)>);

impl UnitDef {
    /// Factor to SI; the first currency rate asked for loads them all.
    pub fn scale(&self) -> f64 {
        if self.scale.get().is_nan() {
            load_rates_once();
        }
        self.scale.get()
    }
}

impl Unit {
    pub fn dim(&self) -> Dim {
        let mut dim = NONE;
        for (u, p) in &self.0 {
            for (acc, x) in dim.iter_mut().zip(u.dim) {
                *acc += x * p;
            }
        }
        dim
    }

    pub fn scale(&self) -> f64 {
        self.0.iter().map(|(u, p)| u.scale().powi(*p as i32)).product()
    }

    /// Offsets only apply to a lone unit like `C`, not inside `C/s`.
    pub fn offset(&self) -> f64 {
        match self.0.as_slice() {
            [(u, 1)] => u.offset,
            _ => 0.0,
        }
    }

    pub fn to_si(&self, v: f64) -> f64 {
        (v + self.offset()) * self.scale()
    }

    pub fn value_from_si(&self, si: f64) -> f64 {
        si / self.scale() - self.offset()
    }

    /// `self * other^sign`, merging repeated units (m * m = m^2).
    pub fn mul(&self, other: &Unit, sign: i8) -> Unit {
        let mut terms = self.0.clone();
        for (u, p) in &other.0 {
            match terms.iter_mut().find(|(t, _)| t.name == u.name) {
                Some(t) => t.1 += p * sign,
                None => terms.push((u.clone(), p * sign)),
            }
        }
        terms.retain(|(_, p)| *p != 0);
        Unit(terms)
    }

    pub fn pow(&self, n: i8) -> Unit {
        Unit(self.0.iter().map(|(u, p)| (u.clone(), p * n)).collect())
    }

    /// Single term with power 1 and the given name (e.g. "mo" for calendar math).
    pub fn is(&self, name: &str) -> bool {
        matches!(self.0.as_slice(), [(u, 1)] if u.name == name)
    }
}

/// `x u` as a quantity in `t`, if the dimensions match.
pub fn to_unit(x: f64, u: &Unit, t: Unit) -> Result<Value, String> {
    if u.dim() != t.dim() {
        return Err(format!("cannot convert {} to {}", describe(u, None), describe(&t, None)));
    }
    Ok(Value::Qty(convert_value(x, u, &t)?, t))
}

/// `x` in `from` as a value in `to` (same dimension). Through `from / to`, so currencies that cancel
/// (`USD/h` to `USD/workyr`) need no exchange rate.
pub fn convert_value(x: f64, from: &Unit, to: &Unit) -> Result<f64, String> {
    if from.offset() != 0.0 || to.offset() != 0.0 {
        return Ok(to.value_from_si(from.to_si(x)));
    }
    let f = from.mul(to, -1).scale();
    if f.is_nan() { Err(rate_error()) } else { Ok(x * f) }
}

/// `si` as whole amounts of each unit but the last, which gets the remainder: "5 ft 6 in", "79 d 9 h 36 min".
pub fn split(si: f64, units: &[Unit]) -> String {
    let (last, big) = units.split_last().expect("at least one unit");
    let round = |x: f64| (x * 1e6).round() / 1e6;
    let mut rest = round(last.value_from_si(si.abs()));
    let mut parts = Vec::new();
    for u in big {
        let f = u.scale() / last.scale();
        let n = (rest / f + 1e-9).floor();
        rest = round(rest - n * f);
        if n != 0.0 {
            parts.push(format!("{n} {u}"));
        }
    }
    if rest != 0.0 || parts.is_empty() {
        parts.push(format!("{} {last}", crate::value::fmt_float(rest)));
    }
    let sign = if si < 0.0 { "-" } else { "" };
    format!("{sign}{}", parts.join(" "))
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", product(self.0.iter().map(|(u, p)| (u.name.as_str(), *p))))
    }
}

/// The running session's units, built-in and user-made, by every name.
fn registry<T>(f: impl FnOnce(&RefCell<HashMap<String, Rc<UnitDef>>>) -> T) -> T {
    crate::state::with(|s| f(&s.units))
}

/// The running session's units made with `unit`: name and what it was defined as, for `help`.
fn user_defs<T>(f: impl FnOnce(&RefCell<Vec<(String, String)>>) -> T) -> T {
    crate::state::with(|s| f(&s.user_units))
}

/// `unit pizza` (no value) makes a new base unit; `unit slice = pizza / 8` or `unit dozen = 12` one
/// in terms of others. Built-in units can't be redefined; user ones can.
pub fn define(name: &str, value: Option<&Value>) -> Result<(), String> {
    let user = user_defs(|u| u.borrow().iter().position(|(n, _)| n == name));
    if user.is_none() && lookup(name).is_ok() {
        return Err(format!("`{name}` is already a unit"));
    }
    let (scale, dim, desc) = match value {
        // Redefining a base keeps its slot, so values made with it still convert.
        None if user.is_some_and(|i| user_defs(|u| u.borrow()[i].1.is_empty())) => return Ok(()),
        None => {
            let used = user_defs(|u| u.borrow().iter().filter(|(_, d)| d.is_empty()).count());
            // ponytail: fixed slots in Dim; widen Dim if 6 user bases ever feels tight.
            if FIRST_USER_BASE + used >= NONE.len() {
                return Err(format!("too many base units: at most {}", NONE.len() - FIRST_USER_BASE));
            }
            (1.0, base(FIRST_USER_BASE + used, 1, NONE), String::new())
        }
        Some(v @ Value::Qty(x, u)) => (u.to_si(*x), u.dim(), v.to_string()),
        Some(v) => match num(v) {
            Some(x) => (x, NONE, v.to_string()),
            None => return Err(format!("expected a quantity or number, got {}", v.type_name())),
        },
    };
    if scale.is_nan() {
        return Err(rate_error());
    }
    let def = Rc::new(UnitDef { name: name.into(), scale: Cell::new(scale), offset: 0.0, dim });
    registry(|r| r.borrow_mut().insert(name.into(), def));
    user_defs(|u| {
        let mut u = u.borrow_mut();
        u.retain(|(n, _)| n != name);
        u.push((name.into(), desc));
    });
    Ok(())
}

/// Units made with `unit`, oldest first: name, what it was defined as, factor to SI and dimension, for saving the session.
pub fn saved_units() -> Vec<(String, String, f64, Dim)> {
    let defs = user_defs(|u| u.borrow().clone());
    defs.into_iter()
        .filter_map(|(name, desc)| {
            let d = registry(|r| r.borrow().get(&name).cloned())?;
            Some((name, desc, d.scale.get(), d.dim))
        })
        .collect()
}

/// Makes a unit `saved_units` gave again.
pub fn restore(name: &str, desc: &str, scale: f64, dim: Dim) {
    registry(|r| r.borrow_mut().insert(name.into(), Rc::new(UnitDef { name: name.into(), scale: Cell::new(scale), offset: 0.0, dim })));
    user_defs(|u| u.borrow_mut().push((name.into(), desc.into())));
}

/// Names of units made with `unit`, for completion.
pub fn user_units() -> Vec<String> {
    user_defs(|u| u.borrow().iter().map(|(n, _)| n.clone()).collect())
}

/// Every unit row: the real ones, then those children add.
pub fn rows() -> impl Iterator<Item = &'static Row> {
    TABLE.iter().chain(informal::TABLE)
}

pub fn builtin_units() -> HashMap<String, Rc<UnitDef>> {
    let mut map = HashMap::new();
    for (names, scale, offset, dim, _) in rows() {
        let primary = names.split(' ').next().unwrap();
        let def = Rc::new(UnitDef { name: primary.into(), scale: Cell::new(*scale), offset: *offset, dim: *dim });
        for n in names.split(' ') {
            map.insert(n.to_string(), def.clone());
        }
    }
    // `banana_for_scale`.
    for (names, ..) in informal::TABLE {
        let primary = names.split(' ').next().unwrap();
        map.insert(format!("{primary}_for_scale"), map[primary].clone());
    }
    for code in CURRENCIES.split(' ') {
        let def = currency(code, if code == "EUR" { 1.0 } else { f64::NAN });
        map.insert(code.to_lowercase(), def.clone());
        map.insert(code.into(), def);
    }
    map
}

pub fn lookup(name: &str) -> Result<Rc<UnitDef>, String> {
    if let Some(u) = registry(|r| r.borrow().get(name).cloned()) {
        return Ok(u);
    }
    // Any other code the rate source has (VND, ARS, ...): load the rates to find out.
    let looks_like_currency = name.len() == 3 && name.bytes().all(|b| b.is_ascii_uppercase());
    if looks_like_currency && !crate::state::with(|s| s.rates_loaded.get()) {
        load_rates_once();
        return lookup(name);
    }
    // Rates failed to load, so a real code like VND isn't registered: say why, not just "unknown".
    if looks_like_currency && crate::state::with(|s| !s.rates_error.borrow().is_empty()) {
        return Err(format!("unknown unit `{name}` ({})", rate_error()));
    }
    let known: Vec<String> = registry(|r| r.borrow().keys().cloned().collect());
    let targets = crate::modules::modules().flat_map(|m| m.targets.iter().map(|t| t.0));
    Err(format!("unknown unit `{name}`{}", crate::error::did_you_mean(name, known.iter().map(String::as_str).chain(targets))))
}

/// Every unit name in this session.
pub fn names() -> Vec<String> {
    registry(|r| r.borrow().keys().cloned().collect())
}

/// Dimension of `spec` from already-loaded units only, so REPL completion never fetches rates.
pub fn known_dim(spec: &UnitSpec) -> Option<Dim> {
    registry(|r| {
        let r = r.borrow();
        let mut dim = NONE;
        for (name, p) in spec {
            for (acc, x) in dim.iter_mut().zip(r.get(name)?.dim) {
                *acc += x * p;
            }
        }
        Some(dim)
    })
}

/// Display names of every loaded unit of dimension `dim`: its COMMON ones first, then the rest sorted.
pub fn names_with_dim(dim: Dim) -> Vec<String> {
    let mut v: Vec<String> = registry(|r| r.borrow().values().filter(|u| u.dim == dim).map(|u| u.name.clone()).collect());
    v.sort();
    v.dedup();
    let kind = DIMS.iter().find(|d| d.1 == dim).map(|d| d.0);
    let common: Vec<String> = COMMON.iter().find(|c| Some(c.0) == kind).map_or(&[][..], |c| c.1).iter().map(|s| s.to_string()).collect();
    v.retain(|n| !common.contains(n));
    common.into_iter().chain(v).collect()
}

/// A unit from space-separated `name^power` terms: `m s^-1`.
pub fn terms(s: &str) -> Result<Unit, String> {
    let mut u = Unit::default();
    for t in s.split(' ') {
        let (name, p) = t.split_once('^').unwrap_or((t, "1"));
        u = u.mul(&unit(name)?.pow(p.parse().map_err(|_| format!("bad power in `{t}`"))?), 1);
    }
    Ok(u)
}

/// A unit from its display form (`km/h`, `kg*m/s^2`, `1/s`, or `c` from `to c`); empty is no unit.
pub fn parse(s: &str) -> Result<Unit, String> {
    let mut u = Unit::default();
    for (i, part) in s.split('/').enumerate() {
        for t in part.split('*').filter(|t| !t.is_empty() && *t != "1") {
            let (name, p) = t.split_once('^').unwrap_or((t, "1"));
            let p: i8 = p.parse().map_err(|_| format!("bad power in `{t}`"))?;
            u = u.mul(&unit(name).or_else(|e| constant(name).ok_or(e))?.pow(p), if i == 0 { 1 } else { -1 });
        }
    }
    Ok(u)
}

pub fn unit(name: &str) -> Result<Unit, String> {
    Ok(Unit(vec![(lookup(name)?, 1)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interp::tests::{show, try_eval};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn unit_names_beat_builtins_in_arithmetic() {
        assert_eq!(show(r#"[100 km / day, 60 s / min, cd * 3, min(3, 4)]"#), "[100 km/d, 1, 3 cd, 3]");
        assert_eq!(show("day = 2; 6 / day"), "3");
    }

    #[test]
    fn units_lists_every_source() {
        for source in ["units", "informal", "currency", "user"] {
            assert_eq!(show(&format!(r#"unit widget; all_units().filter(|u| u.source == "{source}").len > 0"#)), "true", "{source}");
        }
        assert_eq!(show(r#"all_units().filter(|u| u.name == "mi")[0].si"#), "1609.34 m");
        assert_eq!(show(r#"all_units().filter(|u| u.name == "USD")[0].kind"#), "money");
        assert_eq!(show(r#"unit widget; all_units().filter(|u| u.name == "widget")[0].kind"#), "widget");
    }

    #[test]
    fn decibels_and_pixels() {
        assert_eq!(show(r#"[db(2).round(2), db(100), db(10, "amplitude"), from_db(20), from_db(-20, "amplitude")]"#), "[3.01, 20, 20, 100, 0.1]");
        assert_eq!(show("[96 px to in, 1 in to px]"), "[1 in, 96 px]");
        assert!(try_eval("db(0)").is_err());
        assert!(try_eval(r#"db(2, "loud")"#).is_err());
    }

    #[test]
    fn convert_linear_and_offset() {
        let (km, mi) = (unit("km").unwrap(), unit("mi").unwrap());
        assert!(close(mi.value_from_si(km.to_si(5.0)), 3.1068559611866697));
        let (f, c) = (unit("F").unwrap(), unit("C").unwrap());
        assert!(close(c.value_from_si(f.to_si(212.0)), 100.0));
    }

    #[test]
    fn compound_units() {
        let kmh = unit("km").unwrap().mul(&unit("h").unwrap(), -1);
        assert_eq!(kmh.dim(), unit("kph").unwrap().dim());
        assert_eq!(kmh.to_string(), "km/h");
        let m2 = unit("m").unwrap().mul(&unit("m").unwrap(), 1);
        assert_eq!(m2.to_string(), "m^2");
        assert!(unit("m").unwrap().mul(&unit("m").unwrap(), -1).0.is_empty());
    }

    #[test]
    fn end_to_end() {
        assert_eq!(show("5 ft 11 in to cm"), "180.34 cm");
        assert_eq!(show("[100 cd * 2 sr to lm, 3 mm / 1 m]"), "[200 lm, 0.003]");
        assert_eq!(show("1 h 30 min 15 s to s"), "5415 s");
        assert_eq!(show("2 * 1 h 30 min"), "3 h");
        assert_eq!(show(r#"parse(1.8 m to ft in) to m"#), "1.8 m");
        assert_eq!(show("5 km to mi"), "3.10686 mi");
        assert_eq!(show("72 F to C"), "22.2222 C");
        assert_eq!(show("-40 C to F"), "-40 F");
        assert_eq!(show("3 km / 20 min to kph"), "9 kph");
        assert_eq!(
            show("[5 mi/h to si, 2 kWh to si, 32 F to si, 1 g/cm^3 to si, 3 Mbps to si]"),
            "[2.2352 m/s, 7200000 J, 273.15 K, 1000 kg/m^3, 3000000 bps]"
        );
        assert_eq!(
            show("[1 pc to ly, 30 mpg to mpg_uk, 1 rev to deg, 12 point to pica, 100 Å to nm, 1 thm to kWh]"),
            "[3.26156 ly, 36.0285 mpg_uk, 360 deg, 1 pica, 10 nm, 29.3071 kWh]"
        );
        assert_eq!(show("1.5 GB to MiB"), "1430.51 MiB");
        assert_eq!(show("2 m + 30 cm"), "2.3 m");
        assert_eq!(show("60 km/h to m/s"), "16.6667 m/s");
        assert_eq!(show("2 m * 3 m"), "6 m^2");
        assert_eq!(show("1 kWh / 1 J"), "3600000");
        assert_eq!(show("d = 5\nd * km to mi"), "3.10686 mi");
        assert_eq!(show("m = 3\n5 m to ft"), "16.4042 ft");
        assert_eq!(show("1 m > 50 cm"), "true");
        assert_eq!(show("2 A * 5 ohm"), "10 V");
        assert_eq!(show("2 mA * 5 kohm"), "10 V");
        assert_eq!(show("12 V / 4 Ω"), "3 A");
        assert_eq!(show("9.81 m/s^2 * 70 kg"), "686.7 N");
        assert_eq!(show("2 N * 3 m"), "6 J");
        assert_eq!(show("1 / 2 s"), "0.5 Hz");
        assert_eq!(show("2 A * 3 s"), "6 coulomb");
        assert_eq!(show("60 W * 8 h"), "480 W*h");
        assert_eq!(show("25 USD/h * 40 h"), "$1,000.00");
        assert_eq!(show("2 kV * 3"), "6 kV");
        assert_eq!(show("2 N * 3 m to N*m"), "6 N*m");
        assert_eq!(show("1 uF to nF"), "1000 nF");
        assert_eq!(show("2000 mAh to coulomb"), "7200 coulomb");
        assert_eq!(show("3000 rpm to Hz"), "50 Hz");
        assert_eq!(
            show(
                "unit sprint = 2 wk
3 sprint to d"
            ),
            "42 d"
        );
        assert_eq!(
            show(
                "unit pizza
unit slice = pizza / 8
2 pizza to slice"
            ),
            "16 slice"
        );
        assert_eq!(
            show(
                "unit six_pack = 6
2 six_pack to six_pack"
            ),
            "2 six_pack"
        );
        assert!(try_eval("unit km = 3").is_err());
        assert!(
            try_eval(
                "unit widget
1 pizza + 1 widget"
            )
            .is_err()
        );
        assert_eq!(
            show(
                "unit = 5
unit * 2"
            ),
            "10"
        );
        assert_eq!(show("0.0012 km to best"), "1.2 m");
        assert_eq!(show("3600 J/s to best"), "3.6 kW");
        assert_eq!(show("1 kg*m^2/s^2 to best"), "1 J");
        assert_eq!(show("1.5e9 B.simplify"), "1.5 GB");
        assert_eq!(show("5000 ft to best"), "5000 ft");
        assert_eq!(show("20 C to best"), "20 C");
        assert_eq!(show("5 cd to best"), "5 cd");
        assert_eq!(show("0.00002 m to best"), "20 µm");
        assert_eq!(show("unit tile\n60 s*min/tile to best"), "3600 s^2/tile");
        assert_eq!(show("unit tile\n10 m*km/tile to best"), "1 ha/tile");
        assert_eq!(show("[36 to dozen, 0.0025 to ppm, 2 gross to dozen, 3 dozen to si, 5 ‰ to ppm]"), "[3 dozen, 2500 ppm, 24 dozen, 36, 5000 ppm]");
        assert_eq!(show("[3 dozen + 1, 1 - 2 dozen, 2 dozen * 3, 50 ppm * 2 kg]"), "[37, -23, 72, 0.0001 kg]");
        assert_eq!(show("[1 tun to hogshead, 1 chain to perch, 3 barleycorn to in, 1 scruple to gr]"), "[4 hogshead, 4 rod, 1 in, 20 gr]");
        assert_eq!(show("[500 km/ms to c, 1 kg * 9.81 m/s^2 to g0*kg, 1e-30 kg to m_e]"), "[1.66782 c, 1.00034 g0*kg, 1.09777 m_e]");
        assert!(try_eval("5 km to c").is_err());
        assert!(try_eval("5 to best").is_err());
        assert!(try_eval("5 km + 1 kg").is_err());
        assert!(try_eval("5 km to kg").is_err());
    }
}
