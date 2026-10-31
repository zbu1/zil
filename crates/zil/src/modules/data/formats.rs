//! Data as text: JSON and CSV in and out.

use crate::modules::Module;
use crate::value::{Table, Value};
use indexmap::IndexMap;

pub const MODULE: Module = Module {
    name: "formats",
    about: "read and write JSON and CSV text",
    #[rustfmt::skip]
    examples: &[
        ("formats", &[
            ("read JSON and add up a list in it", r#""\{\"a\": [1, 2]}".from_json.a.sum"#),
            ("read CSV and add up a column", "\"name,n\\nx,1\\ny,2\".from_csv.n.sum"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{csv, from_csv as read_csv, from_json as json_value, line, to_json as json};
    use crate::modules::Fail;
    use crate::signature::{As, List, Or};
    use crate::value::{Table, Value};
    use std::rc::Rc;

    /// read JSON text into zil values: objects become maps, arrays lists, and null nil
    #[builtin(examples = [r#""\{\"a\": [1, 2.5, null]}".from_json"#], see = ["to_json", "parse"])]
    fn from_json(s: &str) -> Result<Value, Fail> {
        Ok(json_value(serde_json::from_str(s).map_err(|e| Fail::Arg(0, format!("invalid JSON: {e}")))?))
    }

    /// v as JSON text
    ///
    /// Values JSON has no type for, like quantities, fractions and dates, become strings
    #[builtin(examples = [r#"{a: [1, nil], b: 5 km}.to_json"#], see = ["from_json"])]
    fn to_json(v: &Value) -> String {
        json(v.to_data()).to_string()
    }

    /// read CSV (comma-separated) text
    ///
    /// The first row is taken as column names and the result is a table. Pass false if there's no such row, to get a list of rows. Numbers become numbers and empty cells nil
    #[builtin(examples = ["\"name,n\\nx,1\".from_csv", "\"1,2\\n3,4\".from_csv(false)"], see = ["to_csv", "table"])]
    fn from_csv(s: &str, #[default(true)] header: bool) -> Result<As<Or<Table, Vec<Value>>>, String> {
        read_csv(s, header).map(As::from)
    }

    /// CSV text from a table, a list of maps, or a list of lists
    ///
    /// Cells with commas or quotes are quoted
    #[builtin(examples = [r#"[{a: 1, b: "x,y"}].to_csv"#, "[[1, 2], [3, 4]].to_csv"], see = ["from_csv"])]
    fn to_csv(rows: Or<&Rc<Table>, &List>) -> Result<String, Fail> {
        let l = match rows {
            Or::A(t) => return Ok(csv(&t.cols, &t.rows)),
            Or::B(l) => l.borrow(),
        };
        match Table::from_maps(&l) {
            Ok(t) => Ok(csv(&t.cols, &t.rows)),
            Err(e) if l.iter().all(|r| matches!(r, Value::Map(_))) => Err(Fail::Arg(0, e)),
            Err(_) if l.iter().all(|r| matches!(r, Value::List(_))) => {
                Ok(l.iter().map(|r| if let Value::List(r) = r { line(&r.borrow()) } else { unreachable!() }).collect())
            }
            Err(_) => Err(Fail::Arg(0, "expected a list of maps or a list of lists".into())),
        }
    }
}

pub fn from_json(j: serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Bool(b),
        J::Number(n) => n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::int),
        J::String(s) => Value::str(s),
        J::Array(a) => Value::list(a.into_iter().map(from_json).collect()),
        J::Object(o) => Value::map(o.into_iter().map(|(k, v)| (k, from_json(v))).collect::<IndexMap<_, _>>()),
    }
}

/// JSON for `d`; what JSON can't hold becomes a string. Non-finite floats become null.
pub(crate) fn to_json(d: crate::Data) -> serde_json::Value {
    use crate::Data as D;
    use crate::value::fmt_float as f;
    use serde_json::Value as J;
    let all = |ds: Vec<D>| J::Array(ds.into_iter().map(to_json).collect());
    let unit = |u: String| if u.is_empty() { u } else { format!(" {u}") };
    match d {
        D::Nil => J::Null,
        D::Bool(b) => J::Bool(b),
        D::Int(n) => J::from(n),
        D::Float(x) => serde_json::Number::from_f64(x).map_or(J::Null, J::Number),
        D::Str(s) | D::Regex(s) | D::Date(s) | D::Fn(s) => J::String(s),
        D::List(ds) | D::Set(ds) => all(ds),
        D::Map(kvs) => J::Object(kvs.into_iter().map(|(k, d)| (k, to_json(d))).collect()),
        D::Table { cols, rows } => J::Array(rows.into_iter().map(|r| J::Object(cols.iter().cloned().zip(r.into_iter().map(to_json)).collect())).collect()),
        D::Big(n) => J::String(n.to_string()),
        D::Frac(r) => J::String(r.to_string()),
        D::Qty { value, unit: u } => J::String(format!("{}{}", f(value), unit(u))),
        D::Unc { value, error, unit: u } => J::String(format!("{} ± {}{}", f(value), f(error), unit(u))),
        D::Cplx { re, im } => J::String(Value::Cplx(re, im).to_string()),
        D::Mod { residue, modulus } => J::String(format!("{residue} (mod {modulus})")),
        D::Bytes(b) => J::String(format!("b\"{}\"", crate::value::escape_bytes(&b))),
    }
}

fn from_csv(s: &str, header: bool) -> Result<Value, String> {
    let mut rows = parse_csv(s)?.into_iter().map(|r| r.into_iter().map(|f| cell(&f)).collect::<Vec<_>>());
    if !header {
        return Ok(Value::list(rows.map(Value::list).collect()));
    }
    let cols: Vec<String> = rows.next().unwrap_or_default().into_iter().map(|v| if let Value::Nil = v { String::new() } else { v.to_string() }).collect();
    let rows = rows.map(|mut r| {
        r.resize(cols.len(), Value::Nil);
        r
    });
    Ok(Value::table(Table { rows: rows.collect(), cols }))
}

/// A CSV field as a value: numbers become numbers, empty is nil.
fn cell(f: &str) -> Value {
    if f.is_empty() {
        Value::Nil
    } else if let Ok(n) = f.parse::<i64>() {
        Value::int(n)
    } else if let Ok(x) = f.parse::<f64>().map_err(drop).and_then(|x| if f.bytes().any(|b| b.is_ascii_digit()) { Ok(x) } else { Err(()) }) {
        Value::Float(x)
    } else {
        Value::str(f)
    }
}

/// RFC 4180 rows: `"`-quoted fields may hold commas, newlines and `""` for a quote.
fn parse_csv(s: &str) -> Result<Vec<Vec<String>>, String> {
    let (mut rows, mut row, mut field, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut cs = s.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '"' if quoted && cs.next_if_eq(&'"').is_some() => field.push('"'),
            '"' => quoted = !quoted,
            c if quoted => field.push(c),
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if quoted {
        return Err("unclosed quote".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

/// CSV with a header row; how a table displays as a string.
pub fn csv(cols: &[String], rows: &[Vec<Value>]) -> String {
    let header: Vec<Value> = cols.iter().map(|c| Value::str(c.as_str())).collect();
    std::iter::once(&header).chain(rows).map(|r| line(r)).collect()
}

fn line(r: &[Value]) -> String {
    let field = |v: &Value| {
        let s = if let Value::Nil = v { String::new() } else { v.to_string() };
        if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
    };
    r.iter().map(field).collect::<Vec<_>>().join(",") + "\n"
}
