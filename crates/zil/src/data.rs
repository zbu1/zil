//! `Data`: a value as plain host-neutral data, for JS and JSON to map from.

use crate::Value;
use crate::modules::units;
use crate::value::{Table, exact};
use num_bigint::BigInt;
use num_rational::BigRational;

/// A zil value without the `Rc`/closure internals. Display hints (base, fraction display) are dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Nil,
    Bool(bool),
    Int(i64),
    Big(BigInt),
    Frac(BigRational),
    Float(f64),
    /// `unit` is its display form: `km/h`, `kg*m/s^2`, `1/s`.
    Qty {
        value: f64,
        unit: String,
    },
    /// `unit` is empty for a plain number.
    Unc {
        value: f64,
        error: f64,
        unit: String,
    },
    Cplx {
        re: f64,
        im: f64,
    },
    /// `residue (mod modulus)`.
    Mod {
        residue: BigInt,
        modulus: BigInt,
    },
    Str(String),
    Bytes(Vec<u8>),
    /// The pattern's source.
    Regex(String),
    /// RFC 9557: `2026-10-06T14:30:00-05:00[America/Chicago]`.
    Date(String),
    List(Vec<Data>),
    Map(Vec<(String, Data)>),
    Set(Vec<Data>),
    Table {
        cols: Vec<String>,
        rows: Vec<Vec<Data>>,
    },
    /// A function's display; it doesn't convert back.
    Fn(String),
}

impl Value {
    pub fn to_data(&self) -> Data {
        let all = |vs: &[Value]| vs.iter().map(Value::to_data).collect();
        match self {
            Value::Nil => Data::Nil,
            Value::Bool(b) => Data::Bool(*b),
            Value::Int(n, _) => Data::Int(*n),
            Value::Big(n, _) => Data::Big((**n).clone()),
            Value::Frac(r, _) => Data::Frac((**r).clone()),
            Value::Float(x) => Data::Float(*x),
            Value::Qty(x, u) => Data::Qty { value: *x, unit: u.to_string() },
            Value::Unc(c) => Data::Unc { value: c.0, error: c.1, unit: c.2.to_string() },
            Value::Cplx(re, im) => Data::Cplx { re: *re, im: *im },
            Value::Mod(x) => Data::Mod { residue: x.0.clone(), modulus: x.1.clone() },
            Value::Str(s) => Data::Str(s.to_string()),
            Value::Bytes(b) => Data::Bytes(b.to_vec()),
            Value::Regex(r) => Data::Regex(r.as_str().into()),
            Value::Date(z) => Data::Date(z.to_string()),
            Value::List(l) => Data::List(all(&l.borrow())),
            Value::Map(m) => Data::Map(m.borrow().iter().map(|(k, v)| (k.clone(), v.to_data())).collect()),
            Value::Set(s) => Data::Set(s.borrow().iter().map(Value::to_data).collect()),
            Value::Table(t) => Data::Table { cols: t.cols.clone(), rows: t.rows.iter().map(|r| all(r)).collect() },
            Value::Fn(_) | Value::Builtin(..) => Data::Fn(self.to_string()),
        }
    }

    /// The value `d` describes; units resolve in the running session, so call it through `Session::from_data`.
    pub(crate) fn from_data(d: Data) -> Result<Value, String> {
        let all = |ds: Vec<Data>| ds.into_iter().map(Value::from_data).collect::<Result<Vec<_>, _>>();
        Ok(match d {
            Data::Nil => Value::Nil,
            Data::Bool(b) => Value::Bool(b),
            Data::Int(n) => Value::int(n),
            Data::Big(n) => exact(BigRational::from_integer(n), crate::ast::Radix::DEC, false),
            Data::Frac(r) => exact(r, crate::ast::Radix::DEC, false),
            Data::Float(x) => Value::Float(x),
            Data::Qty { value, unit } => Value::qty(value, units::parse(&unit)?),
            Data::Unc { error, .. } if error.is_nan() || error < 0.0 => return Err(format!("uncertainty must be ≥ 0, got {error}")),
            Data::Unc { value, error, unit } => Value::unc(value, error, units::parse(&unit)?),
            Data::Cplx { re, im } => Value::Cplx(re, im),
            Data::Mod { modulus, .. } if modulus <= BigInt::ZERO => return Err(format!("the modulus must be positive, got {modulus}")),
            Data::Mod { residue, modulus } => Value::modular(residue, modulus),
            Data::Str(s) => Value::str(s),
            Data::Bytes(b) => Value::bytes(b),
            Data::Regex(s) => Value::Regex(crate::state::regex(&s).map_err(|e| e.to_string())?.into()),
            Data::Date(s) => Value::date(s.parse().map_err(|e| format!("bad date `{s}`: {e}"))?),
            Data::List(ds) => Value::list(all(ds)?),
            Data::Map(kvs) => Value::map(kvs.into_iter().map(|(k, d)| Ok((k, Value::from_data(d)?))).collect::<Result<_, String>>()?),
            Data::Set(ds) => {
                let vs = all(ds)?;
                if let Some(v) = vs.iter().find(|v| !v.hashable()) {
                    return Err(format!("a set can't hold a {}", v.type_name()));
                }
                Value::set(vs.into_iter().collect())
            }
            Data::Table { cols, rows } => {
                if let Some(r) = rows.iter().find(|r| r.len() != cols.len()) {
                    return Err(format!("a table row has {} cells for {} columns", r.len(), cols.len()));
                }
                Value::table(Table { rows: rows.into_iter().map(all).collect::<Result<_, _>>()?, cols })
            }
            Data::Fn(s) => return Err(format!("{s} can't be made from data")),
        })
    }
}
