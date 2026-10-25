//! Runtime values: display, equality, ordering, number formatting.

use crate::ast::Radix;
use crate::interp::Closure;
use crate::modules::{self, Module, units::Unit};
use indexmap::{IndexMap, IndexSet};
use jiff::Zoned;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};
use regex::Regex;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::rc::Rc;

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    /// Integer and how it displays (`255 to hex` remembers base 16).
    Int(i64, Radix),
    /// An integer too big for i64; arithmetic overflows into it and shrinks back out.
    Big(Rc<BigInt>, Radix),
    /// Exact non-integer from int division (`7 / 2`); `true` displays `7/2` instead of `3.5`.
    Frac(Rc<BigRational>, bool),
    Float(f64),
    Qty(f64, Unit),
    /// `5 ± 0.1 m`: value, uncertainty (≥ 0) and unit, which is empty for a plain number.
    /// Behind an `Rc` so `Value` stays small: every eval frame holds several.
    Unc(Rc<(f64, f64, Unit)>),
    /// `2 + 3i`: real and imaginary parts.
    Cplx(f64, f64),
    /// `5 mod 7`: residue and modulus, with `0 <= residue < modulus`.
    Mod(Rc<(BigInt, BigInt)>),
    Str(Rc<str>),
    /// Raw bytes: `b"\x7fELF"`, `read_bytes(path)`.
    Bytes(Rc<[u8]>),
    Regex(Rc<Regex>),
    Date(Rc<Zoned>),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<IndexMap<String, Value>>>),
    /// Insertion-ordered; holds only `hashable` values.
    Set(Rc<RefCell<IndexSet<Value>>>),
    /// Rows under named columns; immutable, so ops build new tables.
    Table(Rc<Table>),
    Fn(Rc<Closure>),
    Builtin(&'static Module, &'static str),
}

pub struct Table {
    pub cols: Vec<String>,
    /// Each row has one cell per column; missing cells are nil.
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    /// A table from maps, columns in order of first appearance; `None` if any item isn't a map.
    pub fn from_maps(items: &[Value]) -> Result<Table, String> {
        let mut cols: IndexSet<String> = IndexSet::new();
        for v in items {
            let Value::Map(m) = v else { return Err("every item must be a map".into()) };
            cols.extend(m.borrow().keys().cloned());
        }
        crate::state::check_len(items.len().saturating_mul(cols.len()), "cells")?;
        let rows = items
            .iter()
            .map(|v| {
                let Value::Map(m) = v else { unreachable!() };
                let m = m.borrow();
                cols.iter().map(|c| m.get(c).cloned().unwrap_or(Value::Nil)).collect()
            })
            .collect();
        Ok(Table { cols: cols.into_iter().collect(), rows })
    }

    pub fn row(&self, i: usize) -> Value {
        Value::map(self.cols.iter().cloned().zip(self.rows[i].iter().cloned()).collect())
    }

    /// The rows as a list of maps.
    pub fn maps(&self) -> Vec<Value> {
        (0..self.rows.len()).map(|i| self.row(i)).collect()
    }

    pub fn column(&self, name: &str) -> Option<Value> {
        let c = self.cols.iter().position(|k| k == name)?;
        Some(Value::list(self.rows.iter().map(|r| r[c].clone()).collect()))
    }
}

impl Value {
    pub fn int(n: i64) -> Value {
        Value::Int(n, Radix::DEC)
    }

    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(v)))
    }

    pub fn map(m: IndexMap<String, Value>) -> Value {
        Value::Map(Rc::new(RefCell::new(m)))
    }

    pub fn table(t: Table) -> Value {
        Value::Table(Rc::new(t))
    }

    pub fn set(s: IndexSet<Value>) -> Value {
        Value::Set(Rc::new(RefCell::new(s)))
    }

    /// What a set can hold: values whose equality is plain and whose hash agrees with it.
    pub fn hashable(&self) -> bool {
        matches!(self, Value::Nil | Value::Bool(_) | Value::Int(..) | Value::Big(..) | Value::Str(_) | Value::Bytes(_))
    }

    pub fn str(s: impl Into<Rc<str>>) -> Value {
        Value::Str(s.into())
    }

    pub fn bytes(b: impl Into<Rc<[u8]>>) -> Value {
        Value::Bytes(b.into())
    }

    pub fn unc(x: f64, e: f64, u: Unit) -> Value {
        Value::Unc(Rc::new((x, e, u)))
    }

    /// `r mod m` for a positive `m`, reduced into `0..m`.
    pub fn modular(r: BigInt, m: BigInt) -> Value {
        use num_integer::Integer;
        Value::Mod(Rc::new((r.mod_floor(&m), m)))
    }

    pub fn date(z: Zoned) -> Value {
        Value::Date(Rc::new(z))
    }

    /// Quantity, collapsing to a plain number when the units cancel (km/m, kWh/J).
    pub fn qty(v: f64, u: Unit) -> Value {
        if u.0.is_empty() {
            Value::Float(v)
        } else if u.dim() == crate::modules::units::NONE {
            Value::Float(v * u.scale())
        } else {
            Value::Qty(v, u)
        }
    }

    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(..) | Value::Big(..) => "int",
            Value::Frac(..) => "frac",
            Value::Float(_) => "float",
            Value::Qty(..) => "quantity",
            Value::Unc(..) => "uncertain",
            Value::Cplx(..) => "complex",
            Value::Mod(_) => "mod",
            Value::Str(_) => "str",
            Value::Bytes(_) => "bytes",
            Value::Regex(_) => "regex",
            Value::Date(_) => "date",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Table(_) => "table",
            Value::Fn(_) | Value::Builtin(..) => "fn",
        }
    }

    fn write(&self, f: &mut fmt::Formatter, top: bool) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n, base) => write!(f, "{}", fmt_int(*n, *base)),
            Value::Big(n, base) => write!(f, "{}", fmt_big(n, base.base)),
            Value::Frac(r, true) => write!(f, "{r}"),
            Value::Frac(r, false) => write!(f, "{}", fmt_float(r.to_f64().unwrap_or(f64::NAN))),
            Value::Float(n) => write!(f, "{}", fmt_float(*n)),
            Value::Qty(n, u) => match modules::units::money::show(*n, u) {
                Some(s) => write!(f, "{s}"),
                None => write!(f, "{} {u}", fmt_float(*n)),
            },
            Value::Unc(c) if c.2.0.is_empty() => write!(f, "{} ± {}", fmt_float(c.0), fmt_float(c.1)),
            Value::Unc(c) => write!(f, "{} ± {} {}", fmt_float(c.0), fmt_float(c.1), c.2),
            Value::Cplx(re, im) if *re == 0.0 => write!(f, "{}i", fmt_float(*im)),
            Value::Cplx(re, im) if im.is_sign_negative() => write!(f, "{} - {}i", fmt_float(*re), fmt_float(-im)),
            Value::Cplx(re, im) => write!(f, "{} + {}i", fmt_float(*re), fmt_float(*im)),
            Value::Mod(x) => write!(f, "{} (mod {})", x.0, x.1),
            Value::Str(s) if top => write!(f, "{s}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Bytes(b) if top && b.len() > 32 => write!(f, "{}", xxd(b)),
            Value::Bytes(b) => write!(f, "b\"{}\"", escape_bytes(b)),
            Value::Regex(r) => write!(f, "r\"{}\"", r.as_str()),
            // Local midnight is just a date.
            Value::Date(z) if z.time() == jiff::civil::Time::midnight() && z.time_zone().iana_name() == crate::host::tz().iana_name() => {
                write!(f, "{}", z.date())
            }
            Value::Date(z) => write!(f, "{}", z.strftime("%Y-%m-%d %H:%M:%S %:z")),
            Value::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    v.write(f, false)?;
                }
                write!(f, "]")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: ")?;
                    v.write(f, false)?;
                }
                write!(f, "}}")
            }
            Value::Set(s) => {
                write!(f, "set(")?;
                for (i, v) in s.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    v.write(f, false)?;
                }
                write!(f, ")")
            }
            // Top level is CSV, so `str` and `write_file` round-trip with `from_csv`; nested reads back as code.
            Value::Table(t) if top => write!(f, "{}", modules::fs::csv(&t.cols, &t.rows)),
            Value::Table(t) => write!(f, "table({:?})", Value::list(t.maps())),
            Value::Fn(_) => write!(f, "<fn>"),
            Value::Builtin(_, name) => write!(f, "<builtin {name}>"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write(f, true)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write(f, false)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a, _), Int(b, _)) => a == b,
            (Int(..) | Big(..) | Frac(..), Int(..) | Big(..) | Frac(..)) => ratio(self) == ratio(other),
            (Int(..) | Big(..) | Frac(..) | Float(_), Int(..) | Big(..) | Frac(..) | Float(_)) => num(self) == num(other),
            (Qty(..), Qty(..)) => compare(self, other) == Some(Ordering::Equal),
            (Unc(a), Unc(b)) => Value::qty(a.0, a.2.clone()) == Value::qty(b.0, b.2.clone()) && Value::qty(a.1, a.2.clone()) == Value::qty(b.1, b.2.clone()),
            (Cplx(a, b), Cplx(c, d)) => a == c && b == d,
            (Mod(a), Mod(b)) => a == b,
            // Congruence: `(2 mod 7) ** 3 == 1`.
            (Mod(x), Int(..) | Big(..)) | (Int(..) | Big(..), Mod(x)) => {
                let n = if let Mod(_) = self { other } else { self };
                matches!(ratio(n), Some(r) if Value::modular(r.to_integer(), x.1.clone()) == Mod(x.clone()))
            }
            (Str(a), Str(b)) => a == b,
            (Bytes(a), Bytes(b)) => a == b,
            (Regex(a), Regex(b)) => a.as_str() == b.as_str(),
            (Date(a), Date(b)) => a.timestamp() == b.timestamp(),
            (List(a), List(b)) => *a.borrow() == *b.borrow(),
            (Map(a), Map(b)) => *a.borrow() == *b.borrow(),
            (Set(a), Set(b)) => *a.borrow() == *b.borrow(),
            (Table(a), Table(b)) => a.cols == b.cols && a.rows == b.rows,
            (Fn(a), Fn(b)) => Rc::ptr_eq(a, b),
            (Builtin(_, a), Builtin(_, b)) => a == b,
            _ => false,
        }
    }
}

// ponytail: `Eq` is only true for `hashable` values (NaN != NaN), which is all a set ever holds.
impl Eq for Value {}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        // Big ints never equal an i64 (they shrink back), so hashing each as itself agrees with `==`.
        match self {
            Value::Bool(b) => b.hash(h),
            Value::Int(n, _) => n.hash(h),
            Value::Big(n, _) => n.hash(h),
            Value::Str(s) => s.hash(h),
            Value::Bytes(b) => b.hash(h),
            v => v.type_name().hash(h),
        }
    }
}

/// The inside of a `b"..."` literal: printable ASCII as is, everything else escaped.
pub fn escape_bytes(b: &[u8]) -> String {
    let mut s = String::new();
    for &c in b {
        match c {
            b'"' | b'\\' | b'{' | b'}' => s.extend(['\\', c as char]),
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            b'\r' => s.push_str("\\r"),
            0 => s.push_str("\\0"),
            b' '..=b'~' => s.push(c as char),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s
}

/// Long bytes display as a hex dump of the first 256; `hexdump(b)` shows them all.
fn xxd(b: &[u8]) -> String {
    let dump = modules::text::encoding::hexdump(&b[..b.len().min(256)]);
    if b.len() > 256 { format!("{dump}\n... {} bytes in all", b.len()) } else { dump }
}

pub fn fmt_int(n: i64, r: Radix) -> String {
    let fixed = r.width > 0 && fits(n, r.width);
    let (sign, m) = match n {
        _ if fixed => ("", n as u64 & mask(r.width)),
        ..0 => ("-", n.unsigned_abs()),
        _ => ("", n as u64),
    };
    let mut s = digits(m, r.base);
    if fixed && r.base != 10 {
        let w = digits(mask(r.width), r.base).len();
        s = format!("{s:0>w$}");
    }
    match r.base {
        10 => format!("{sign}{s}"),
        16 => format!("{sign}0x{s}"),
        2 => format!("{sign}0b{s}"),
        8 => format!("{sign}0o{s}"),
        b => format!("{sign}{b}#{s}"),
    }
}

/// Big ints ignore a bit width: two's complement only applies up to 64 bits.
fn fmt_big(n: &BigInt, base: u32) -> String {
    let sign = if n.is_negative() { "-" } else { "" };
    let s = n.abs().to_str_radix(base);
    match base {
        10 => format!("{sign}{s}"),
        16 => format!("{sign}0x{s}"),
        2 => format!("{sign}0b{s}"),
        8 => format!("{sign}0o{s}"),
        b => format!("{sign}{b}#{s}"),
    }
}

fn digits(mut m: u64, base: u32) -> String {
    let mut d = Vec::new();
    loop {
        d.push(std::char::from_digit((m % base as u64) as u32, base).unwrap());
        m /= base as u64;
        if m == 0 {
            break;
        }
    }
    d.iter().rev().collect()
}

/// Whether `n` is representable in `w` bits, signed or unsigned.
pub fn fits(n: i64, w: u32) -> bool {
    let n = n as i128;
    -(1i128 << (w - 1)) <= n && n < (1i128 << w)
}

pub fn mask(w: u32) -> u64 {
    u64::MAX >> (64 - w)
}

/// 6 significant digits, trailing zeros trimmed; scientific for very large/small.
pub fn fmt_float(x: f64) -> String {
    if x.is_nan() {
        return "undefined".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞" } else { "-∞" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let trim = |s: String| {
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    let exp = x.abs().log10().floor() as i32;
    if !(-6..15).contains(&exp) {
        let s = format!("{x:.5e}");
        let (mantissa, e) = s.split_once('e').unwrap();
        return format!("{}e{e}", trim(mantissa.to_string()));
    }
    trim(format!("{:.*}", (5 - exp).max(0) as usize, x))
}

pub fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n, _) => Some(*n as f64),
        Value::Big(n, _) => n.to_f64(),
        Value::Frac(r, _) => r.to_f64(),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

/// Ints and fractions as an exact ratio.
pub fn ratio(v: &Value) -> Option<BigRational> {
    match v {
        Value::Int(n, _) => Some(BigRational::from_integer((*n).into())),
        Value::Big(n, _) => Some(BigRational::from_integer((**n).clone())),
        Value::Frac(r, _) => Some((**r).clone()),
        _ => None,
    }
}

/// An exact result as the smallest value that holds it: int, big int, or fraction.
pub fn exact(r: BigRational, base: Radix, as_frac: bool) -> Value {
    if !r.is_integer() {
        return Value::Frac(Rc::new(r), as_frac);
    }
    let n = r.to_integer();
    match n.to_i64() {
        Some(i) => Value::Int(i, base),
        None => Value::Big(Rc::new(n), base),
    }
}

pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Int(a, _), Value::Int(b, _)) => Some(a.cmp(b)),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        (Value::Bytes(a), Value::Bytes(b)) => Some(a.cmp(b)),
        (Value::Int(..) | Value::Big(..) | Value::Frac(..), Value::Int(..) | Value::Big(..) | Value::Frac(..)) => Some(ratio(a)?.cmp(&ratio(b)?)),
        _ => modules::compare(a, b).or_else(|| num(a)?.partial_cmp(&num(b)?)),
    }
}
