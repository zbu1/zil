//! `new Session(options)` for JS: zil values in and out as native JS values, with tagged objects for exact or rich types.
//!
//! | zil | JS |
//! |---|---|
//! | nil, bool, str | `null`, `boolean`, `string` |
//! | bytes | `Uint8Array` |
//! | int | `number` when safe, else `bigint` |
//! | float | `number` |
//! | fraction | `{type: "frac", num, den, value}` (`num`/`den` bigint) |
//! | quantity | `{type: "quantity", value, unit}` |
//! | uncertain | `{type: "uncertain", value, error, unit}` |
//! | complex | `{type: "complex", re, im}` |
//! | mod | `{type: "mod", residue, modulus}` (bigint) |
//! | date | `{type: "date", iso, tz, epochMs}` |
//! | list, set, map | `Array`, `Set`, `Map` |
//! | table | `{type: "table", columns, rows}` |
//! | regex | `{type: "regex", source}` |
//! | function | a `ZilFunction`, for `session.call(f, [args])`; it doesn't convert back |
//!
//! Every tagged object's `toString()` is zil's display of the value. Going in, plain objects are maps too.

use crate::JsHost;
use js_sys::{Array, BigInt, Function, Object, Reflect};
use num_bigint::BigInt as Big;
use num_rational::BigRational;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use zil::{Data, Value};

#[wasm_bindgen(inline_js = "export function tag(o, text) { Object.defineProperty(o, 'toString', { value: () => text }); return o; }")]
extern "C" {
    fn tag(o: &Object, text: &str) -> Object;
}

#[wasm_bindgen(typescript_custom_section)]
const TS: &str = r#"
/** A zil value as JS sees it; see the mapping in `session.rs`. */
export type ZilValue =
  | null | boolean | number | bigint | string | Uint8Array
  | ZilValue[] | Set<ZilValue> | Map<string, ZilValue>
  | ZilFunction
  | Tagged<"frac", { num: bigint; den: bigint; value: number }>
  | Tagged<"quantity", { value: number; unit: string }>
  | Tagged<"uncertain", { value: number; error: number; unit: string }>
  | Tagged<"complex", { re: number; im: number }>
  | Tagged<"mod", { residue: bigint; modulus: bigint }>
  | Tagged<"date", { iso: string; tz: string; epochMs: number }>
  | Tagged<"table", { columns: string[]; rows: ZilValue[][] }>
  | Tagged<"regex", { source: string }>;

/** `toString()` is zil's display of the value. */
export type Tagged<T extends string, F> = { type: T; toString(): string } & F;

/** What `set` and `call` accept: every `ZilValue` but functions, plus plain objects as maps. Tagged objects need only their fields. */
export type ZilInput =
  | null | undefined | boolean | number | bigint | string | Uint8Array
  | ZilInput[] | Set<ZilInput> | Map<string, ZilInput> | { [key: string]: ZilInput }
  | { type: "frac"; num: bigint | number; den: bigint | number }
  | { type: "quantity"; value: number; unit: string }
  | { type: "uncertain"; value: number; error: number; unit?: string }
  | { type: "complex"; re: number; im: number }
  | { type: "mod"; residue: bigint | number; modulus: bigint | number }
  | { type: "date"; iso: string }
  | { type: "table"; columns: string[]; rows: ZilInput[][] }
  | { type: "regex"; source: string };

export interface SessionOptions {
  /** Each printed line; `console.log` by default. */
  onPrint?: (text: string) => void;
  /** Caps on each `eval` or `call`; `Infinity` for none. By default 5 seconds and 1 MiB of output. */
  limits?: { steps?: number; seconds?: number; depth?: number; len?: number; bits?: number; output?: number };
  /** Significant digits in a float's display, 1 to 17; 6 by default. */
  precision?: number;
}
"#;

/// A zil session: globals and user units persist across `eval`s.
#[wasm_bindgen(js_name = Session)]
pub struct JsSession(zil::Session);

#[wasm_bindgen(js_class = Session)]
impl JsSession {
    /// Options: `onPrint(text)` receives each printed line (default `console.log`), `limits` overrides any of
    /// `{steps, seconds, depth, len, bits, output}` (`Infinity` for none), and `precision` sets a float's significant
    /// digits. By default a run stops after 5 s or 1 MiB of output.
    #[wasm_bindgen(constructor)]
    pub fn new(#[wasm_bindgen(unchecked_optional_param_type = "SessionOptions")] options: Option<Object>) -> Result<JsSession, JsValue> {
        let opt = |o: &JsValue, k: &str| Reflect::get(o, &k.into()).ok().filter(|v| !v.is_undefined());
        let options: JsValue = options.map_or(JsValue::UNDEFINED, Into::into);
        let print = match opt(&options, "onPrint") {
            Some(f) => f.dyn_into::<Function>().map_err(|_| type_error("onPrint must be a function"))?,
            None => Reflect::get(&Reflect::get(&js_sys::global(), &"console".into())?, &"log".into())?.into(),
        };
        let mut s = zil::Session::new(Rc::new(JsHost(Some(print))));
        let l = s.limits();
        l.seconds = Some(5.0);
        l.output = 1 << 20;
        if let Some(limits) = opt(&options, "limits") {
            for key in ["steps", "seconds", "depth", "len", "bits", "output"] {
                let Some(v) = opt(&limits, key) else { continue };
                let n = v.as_f64().filter(|n| *n >= 0.0).ok_or_else(|| type_error(&format!("limits.{key} must be a number ≥ 0")))?;
                let int = |max| if n.is_infinite() { max } else { n as u64 };
                match key {
                    "steps" => l.steps = int(u64::MAX),
                    "seconds" => l.seconds = n.is_finite().then_some(n),
                    "depth" => l.depth = int(u64::MAX) as usize,
                    "len" => l.len = int(u64::MAX) as usize,
                    "bits" => l.bits = int(u64::MAX),
                    _ => l.output = int(u64::MAX) as usize,
                }
            }
        }
        if let Some(p) = opt(&options, "precision") {
            let p = p
                .as_f64()
                .filter(|p| (1.0..=17.0).contains(p) && p.fract() == 0.0)
                .ok_or_else(|| type_error("precision must be a whole number from 1 to 17"))?;
            s.settings().precision = p as usize;
        }
        Ok(JsSession(s))
    }

    /// Runs `src`, returning the last statement's value. Throws a `ZilError`.
    #[wasm_bindgen(unchecked_return_type = "ZilValue")]
    pub fn eval(&mut self, src: &str) -> Result<JsValue, ZilError> {
        let v = self.0.eval(src).map_err(|e| ZilError::new(e, src))?;
        Ok(self.to_js(&v))
    }

    /// A global variable or builtin, or `undefined`.
    #[wasm_bindgen(unchecked_return_type = "ZilValue | undefined")]
    pub fn get(&self, name: &str) -> JsValue {
        self.0.get(name).map_or(JsValue::UNDEFINED, |v| self.to_js(&v))
    }

    /// Calls `f` with `args`, under the same limits as `eval`. Throws a `ZilError`, or a `TypeError` for an argument zil can't hold.
    #[wasm_bindgen(unchecked_return_type = "ZilValue")]
    pub fn call(&mut self, f: &ZilFunction, #[wasm_bindgen(unchecked_optional_param_type = "ZilInput[]")] args: Option<Array>) -> Result<JsValue, JsValue> {
        let args = args.unwrap_or_default().iter().map(|a| self.0.from_data(from_js(&a)?).map_err(|e| type_error(&e))).collect::<Result<_, _>>()?;
        let v = self.0.call(&f.0, args).map_err(|e| ZilError::new(e, ""))?;
        Ok(self.to_js(&v))
    }

    /// Sets a global variable. Throws a `TypeError` for a value zil can't hold.
    pub fn set(&mut self, name: &str, #[wasm_bindgen(unchecked_param_type = "ZilInput")] value: JsValue) -> Result<(), JsValue> {
        let v = self.0.from_data(from_js(&value)?).map_err(|e| type_error(&e))?;
        self.0.set(name, v);
        Ok(())
    }
}

impl JsSession {
    fn to_js(&self, v: &Value) -> JsValue {
        self.0.enter(|| to_js(v))
    }
}

/// A zil function, to call with `session.call(f, [args])`.
#[wasm_bindgen]
pub struct ZilFunction(Value);

#[wasm_bindgen]
impl ZilFunction {
    /// `"function"`, like the tagged objects' `type`.
    #[wasm_bindgen(getter, js_name = type, unchecked_return_type = "\"function\"")]
    pub fn kind(&self) -> String {
        "function".into()
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        self.0.to_string()
    }
}

fn to_js(v: &Value) -> JsValue {
    let tagged = |kind: &str, fields: &[(&str, JsValue)]| {
        let o = Object::new();
        Reflect::set(&o, &"type".into(), &kind.into()).unwrap();
        for (k, v) in fields {
            Reflect::set(&o, &(*k).into(), v).unwrap();
        }
        tag(&o, &v.to_string()).into()
    };
    let all = |vs: &[Value]| vs.iter().map(to_js).collect::<Array>();
    match v {
        Value::Fn(_) | Value::Builtin(..) => ZilFunction(v.clone()).into(),
        Value::List(l) => all(&l.borrow()).into(),
        Value::Set(s) => js_sys::Set::new(&s.borrow().iter().map(to_js).collect::<Array>()).into(),
        Value::Map(m) => {
            let out = js_sys::Map::new();
            for (k, v) in m.borrow().iter() {
                out.set(&k.into(), &to_js(v));
            }
            out.into()
        }
        Value::Table(t) => {
            let columns: Array = t.cols.iter().map(JsValue::from).collect();
            let rows: Array = t.rows.iter().map(|r| JsValue::from(all(r))).collect();
            tagged("table", &[("columns", columns.into()), ("rows", rows.into())])
        }
        v => match v.to_data() {
            Data::Nil => JsValue::NULL,
            Data::Bool(b) => b.into(),
            Data::Int(n) if n.unsigned_abs() <= MAX_SAFE => (n as f64).into(),
            Data::Int(n) => BigInt::from(n).into(),
            Data::Big(n) => big(&n),
            Data::Frac(r) => {
                let value = r.numer().to_string().parse::<f64>().unwrap_or(f64::NAN) / r.denom().to_string().parse::<f64>().unwrap_or(f64::NAN);
                tagged("frac", &[("num", big(r.numer())), ("den", big(r.denom())), ("value", value.into())])
            }
            Data::Float(x) => x.into(),
            Data::Qty { value, unit } => tagged("quantity", &[("value", value.into()), ("unit", unit.into())]),
            Data::Unc { value, error, unit } => tagged("uncertain", &[("value", value.into()), ("error", error.into()), ("unit", unit.into())]),
            Data::Cplx { re, im } => tagged("complex", &[("re", re.into()), ("im", im.into())]),
            Data::Mod { residue, modulus } => tagged("mod", &[("residue", big(&residue)), ("modulus", big(&modulus))]),
            Data::Str(s) => s.into(),
            Data::Bytes(b) => js_sys::Uint8Array::from(&b[..]).into(),
            Data::Regex(s) => tagged("regex", &[("source", s.into())]),
            Data::Date(iso) => {
                let z: jiff::Zoned = iso.parse().unwrap();
                let tz = z.time_zone().iana_name().map_or_else(|| z.offset().to_string(), String::from);
                tagged("date", &[("iso", iso.into()), ("tz", tz.into()), ("epochMs", (z.timestamp().as_millisecond() as f64).into())])
            }
            // The containers and functions are handled above.
            Data::List(_) | Data::Set(_) | Data::Map(_) | Data::Table { .. } | Data::Fn(_) => unreachable!(),
        },
    }
}

/// `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE: u64 = (1 << 53) - 1;

fn big(n: &Big) -> JsValue {
    BigInt::new(&n.to_string().into()).unwrap().into()
}

fn type_error(msg: &str) -> JsValue {
    js_sys::TypeError::new(msg).into()
}

fn from_js(v: &JsValue) -> Result<Data, JsValue> {
    let field = |k: &str| Reflect::get(v, &k.into()).unwrap_or(JsValue::UNDEFINED);
    let num = |k: &str| field(k).as_f64().ok_or_else(|| type_error(&format!("`{k}` must be a number")));
    let text = |k: &str| field(k).as_string().ok_or_else(|| type_error(&format!("`{k}` must be a string")));
    let int = |v: &JsValue| -> Result<Big, JsValue> {
        let s = if v.is_bigint() { v.unchecked_ref::<BigInt>().to_string(10).ok().and_then(|s| s.as_string()) } else { v.as_f64().map(|n| n.to_string()) };
        s.and_then(|s| s.parse().ok()).ok_or_else(|| type_error("expected an integer"))
    };
    let all = |a: &JsValue| Array::from(a).iter().map(|d| from_js(&d)).collect::<Result<Vec<_>, _>>();
    Ok(if v.is_null() || v.is_undefined() {
        Data::Nil
    } else if let Some(b) = v.as_bool() {
        Data::Bool(b)
    } else if let Some(s) = v.as_string() {
        Data::Str(s)
    } else if let Some(n) = v.as_f64() {
        if n.fract() == 0.0 && n.abs() <= MAX_SAFE as f64 { Data::Int(n as i64) } else { Data::Float(n) }
    } else if v.is_bigint() {
        let n = int(v)?;
        i64::try_from(&n).map_or(Data::Big(n), Data::Int)
    } else if let Some(b) = v.dyn_ref::<js_sys::Uint8Array>() {
        Data::Bytes(b.to_vec())
    } else if Array::is_array(v) {
        Data::List(all(v)?)
    } else if v.is_instance_of::<js_sys::Set>() {
        Data::Set(all(&Array::from(v))?)
    } else if let Some(m) = v.dyn_ref::<js_sys::Map>() {
        let mut kvs = vec![];
        for kv in m.entries() {
            let kv = Array::from(&kv?);
            kvs.push((kv.get(0).as_string().ok_or_else(|| type_error("zil map keys must be strings"))?, from_js(&kv.get(1))?));
        }
        Data::Map(kvs)
    } else if v.is_object() && !v.is_function() {
        match field("type").as_string().as_deref() {
            Some("frac") => Data::Frac(BigRational::new(int(&field("num"))?, int(&field("den"))?)),
            Some("quantity") => Data::Qty { value: num("value")?, unit: text("unit")? },
            Some("uncertain") => Data::Unc { value: num("value")?, error: num("error")?, unit: field("unit").as_string().unwrap_or_default() },
            Some("complex") => Data::Cplx { re: num("re")?, im: num("im")? },
            Some("mod") => Data::Mod { residue: int(&field("residue"))?, modulus: int(&field("modulus"))? },
            Some("date") => Data::Date(text("iso")?),
            Some("regex") => Data::Regex(text("source")?),
            Some("function") => return Err(type_error("a zil function can't be passed back in; call it with session.call")),
            Some("table") => Data::Table {
                cols: Array::from(&field("columns"))
                    .iter()
                    .map(|c| c.as_string().ok_or_else(|| type_error("table columns must be strings")))
                    .collect::<Result<_, _>>()?,
                rows: Array::from(&field("rows")).iter().map(|r| all(&r)).collect::<Result<_, _>>()?,
            },
            _ => Data::Map(
                Object::entries(v.unchecked_ref())
                    .iter()
                    .map(|kv| Ok((Array::from(&kv).get(0).as_string().unwrap(), from_js(&Array::from(&kv).get(1))?)))
                    .collect::<Result<_, JsValue>>()?,
            ),
        }
    } else {
        return Err(type_error("zil can't hold this value"));
    })
}

/// What `eval` throws: the message, where it happened, and which limit stopped the run, if one did.
#[wasm_bindgen]
pub struct ZilError {
    error: zil::Error,
    src: String,
}

#[wasm_bindgen]
impl ZilError {
    fn new(error: zil::Error, src: &str) -> ZilError {
        ZilError { error, src: src.into() }
    }

    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.error.msg.clone()
    }

    /// 1-based.
    #[wasm_bindgen(getter)]
    pub fn line(&self) -> usize {
        self.error.line_col(&self.src).0
    }

    /// 1-based, in characters.
    #[wasm_bindgen(getter)]
    pub fn column(&self) -> usize {
        self.error.line_col(&self.src).1
    }

    /// `"steps"`, `"time"`, `"interrupt"`, `"depth"`, `"len"`, `"bits"` or `"output"`; `undefined` for an error in the code.
    #[wasm_bindgen(getter)]
    pub fn limit(&self) -> Option<String> {
        self.error.limit.map(|l| format!("{l:?}").to_lowercase())
    }

    /// The full report with the source underlined: `"plain"` (the default), `"ansi"` or `"html"`.
    pub fn render(&self, format: Option<String>) -> String {
        match format.as_deref() {
            Some("ansi") => self.error.render("<eval>", &self.src, true),
            Some("html") => zil::ansi::to_html(&self.error.render("<eval>", &self.src, true)),
            _ => self.error.render("<eval>", &self.src, false),
        }
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        format!("ZilError: {}", self.error.msg)
    }
}
