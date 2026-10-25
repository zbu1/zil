//! Typed signatures like `round(x: num|quantity, digits: int = 0)`: the Rust types `#[zil_macros::builtins]` reads them
//! from, parsing them, and saying which argument a failed call got wrong.

use crate::Error;
use crate::error::{args_word, short};
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail};
use crate::value::Value;
use indexmap::IndexMap;
use regex::Regex;
use std::cell::RefCell;

/// A Rust type's name in a sig.
pub trait Named {
    const TYPE: &'static str;
}

/// A builtin param type: the value, if it's of this type.
pub trait Arg<'a>: Sized {
    fn from(v: &'a Value) -> Option<Self>;
}

/// A builtin return type.
pub trait Ret {
    fn ret(self) -> Call;
}

/// Either type, shown `A|B`; tried in that order.
pub enum Or<A, B> {
    A(A),
    B(B),
}

pub type List = RefCell<Vec<Value>>;
pub type Map = RefCell<IndexMap<String, Value>>;

macro_rules! named {
    ($($t:ty => $name:literal),* $(,)?) => { $(impl Named for $t { const TYPE: &'static str = $name; })* };
}

named! {
    &str => "str", String => "str", i64 => "int", bool => "bool", &Regex => "regex", &List => "list", Vec<Value> => "list",
    &Map => "map", &Value => "any", Value => "any", () => "nil",
}

impl<'a> Arg<'a> for &'a str {
    fn from(v: &'a Value) -> Option<Self> {
        if let Value::Str(s) = v { Some(s) } else { None }
    }
}

impl<'a> Arg<'a> for i64 {
    fn from(v: &'a Value) -> Option<Self> {
        if let Value::Int(n, _) = v { Some(*n) } else { None }
    }
}

impl<'a> Arg<'a> for &'a Regex {
    fn from(v: &'a Value) -> Option<Self> {
        if let Value::Regex(r) = v { Some(r) } else { None }
    }
}

impl<'a> Arg<'a> for &'a List {
    fn from(v: &'a Value) -> Option<Self> {
        if let Value::List(l) = v { Some(l) } else { None }
    }
}

impl<'a> Arg<'a> for &'a Map {
    fn from(v: &'a Value) -> Option<Self> {
        if let Value::Map(m) = v { Some(m) } else { None }
    }
}

impl<'a> Arg<'a> for &'a Value {
    fn from(v: &'a Value) -> Option<Self> {
        Some(v)
    }
}

impl<'a, A: Arg<'a>, B: Arg<'a>> Arg<'a> for Or<A, B> {
    fn from(v: &'a Value) -> Option<Self> {
        A::from(v).map(Or::A).or_else(|| B::from(v).map(Or::B))
    }
}

/// An `x?: T` argument: missing or nil is `None`, anything else must be a `T`.
pub fn opt<'a, T: Arg<'a>>(v: Option<&'a Value>) -> Option<Option<T>> {
    match v {
        None | Some(Value::Nil) => Some(None),
        Some(v) => T::from(v).map(Some),
    }
}

/// An `x: T = d` argument.
pub fn or<'a, T: Arg<'a>>(v: Option<&'a Value>, d: T) -> Option<T> {
    v.map_or(Some(d), T::from)
}

impl Ret for Value {
    fn ret(self) -> Call {
        Ok(self)
    }
}

impl Ret for String {
    fn ret(self) -> Call {
        Ok(Value::str(self))
    }
}

impl Ret for i64 {
    fn ret(self) -> Call {
        Ok(Value::int(self))
    }
}

impl Ret for bool {
    fn ret(self) -> Call {
        Ok(Value::Bool(self))
    }
}

impl Ret for Vec<Value> {
    fn ret(self) -> Call {
        Ok(Value::list(self))
    }
}

impl Ret for () {
    fn ret(self) -> Call {
        Ok(Value::Nil)
    }
}

impl<T: Ret> Ret for Option<T> {
    fn ret(self) -> Call {
        self.map_or(Ok(Value::Nil), T::ret)
    }
}

impl<A: Ret, B: Ret> Ret for Or<A, B> {
    fn ret(self) -> Call {
        match self {
            Or::A(a) => a.ret(),
            Or::B(b) => b.ret(),
        }
    }
}

impl<T: Ret, E: Into<Fail>> Ret for Result<T, E> {
    fn ret(self) -> Call {
        self.map_err(Into::into)?.ret()
    }
}

/// The length of `parts` joined, for building a sig at compile time.
pub const fn concat_len(parts: &[&str]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i < parts.len() {
        n += parts[i].len();
        i += 1;
    }
    n
}

pub const fn concat<const N: usize>(parts: &[&str]) -> [u8; N] {
    let mut out = [0; N];
    let (mut at, mut i) = (0, 0);
    while i < parts.len() {
        let b = parts[i].as_bytes();
        let mut j = 0;
        while j < b.len() {
            out[at] = b[j];
            at += 1;
            j += 1;
        }
        i += 1;
    }
    out
}

/// Type names a signature may use: what `type()` returns, plus `num` (int, frac or float) and `any`.
#[rustfmt::skip]
pub const TYPES: &[&str] =
    &["nil", "bool", "int", "frac", "float", "quantity", "uncertain", "complex", "mod", "str", "bytes", "regex", "date", "list", "map", "set", "table", "fn", "num", "any"];

/// One form of a typed signature, like `round(x: num|quantity, digits?: int) -> num|quantity`; a trailing `...` repeats the last parameter.
pub struct Form {
    pub text: &'static str,
    /// Each parameter's types, and whether it's optional.
    pub params: Vec<(Vec<&'static str>, bool)>,
    rest: bool,
}

impl Form {
    fn min(&self) -> usize {
        self.params.iter().filter(|p| !p.1).count()
    }

    fn max(&self) -> Option<usize> {
        (!self.rest).then_some(self.params.len())
    }

    fn types(&self, i: usize) -> &[&'static str] {
        self.params.get(i).or(self.params.last().filter(|_| self.rest)).map_or(&[], |p| &p.0)
    }
}

/// The forms of a `Doc.sig`, separated by ` / `.
pub fn forms(sig: &'static str) -> Result<Vec<Form>, String> {
    sig.split(" / ")
        .map(|text| {
            let (call, ret) = text.rsplit_once(" -> ").ok_or_else(|| format!("`{text}` has no return type"))?;
            if let Some(t) = ret.split('|').find(|t| !TYPES.contains(t)) {
                return Err(format!("unknown return type `{t}` in `{text}`"));
            }
            let inner = call.split_once('(').and_then(|(_, r)| r.strip_suffix(')')).ok_or_else(|| format!("`{text}` is not `name(params)`"))?;
            let mut form = Form { text, params: vec![], rest: false };
            for p in params(inner) {
                if form.rest {
                    return Err(format!("`...` must come last in `{text}`"));
                }
                if p == "..." {
                    form.rest = true;
                    continue;
                }
                let (p, default) = p.split_once(" = ").map_or((p, None), |(p, d)| (p, Some(d)));
                let (name, ty) = p.split_once(": ").ok_or_else(|| format!("`{p}` in `{text}` has no type"))?;
                let mut types: Vec<_> = ty.split('|').collect();
                if let Some(t) = types.iter().find(|t| !TYPES.contains(t)) {
                    return Err(format!("unknown type `{t}` in `{text}`"));
                }
                // `x?: T` is short for `x: T|nil = nil`.
                let short = name.ends_with('?');
                if short && default.is_some() {
                    return Err(format!("`{name}` in `{text}` has a default, so write `{}: {ty}` without `?`", name.trim_end_matches('?')));
                }
                if short && types.contains(&"nil") {
                    return Err(format!("`{name}` in `{text}` already allows nil, so drop `|nil`"));
                }
                if short {
                    types.push("nil");
                }
                form.params.push((types, short || default.is_some()));
            }
            Ok(form)
        })
        .collect()
}

/// A form's params, split on the `, ` outside string defaults like `sep: str = ", "`.
fn params(inner: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted, mut escaped) = (vec![], 0, false, false);
    for (i, c) in inner.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ',' if !quoted && inner[i..].starts_with(", ") => {
                out.push(&inner[start..i]);
                start = i + 2;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out.retain(|p| !p.is_empty());
    out
}

/// Whether `v` is of signature type `ty`.
pub fn is_type(v: &Value, ty: &str) -> bool {
    match ty {
        "any" => true,
        "num" => matches!(v, Value::Int(..) | Value::Big(..) | Value::Frac(..) | Value::Float(_)),
        _ => v.type_name() == ty,
    }
}

/// The first argument no form accepts, or a wrong count; `help:` lists every form.
pub fn bad_args(f: &Doc, args: &[Value], span: &Span, at: &dyn Fn(usize) -> Span) -> Error {
    // ponytail: untyped sigs (mid-migration) fall back to listing the argument types.
    let Ok(forms) = forms(f.sig) else {
        let types: Vec<_> = args.iter().map(Value::type_name).collect();
        return Error::new(format!("{}: unsupported arguments ({})", f.name, types.join(", ")), span.clone()).help(f.sig);
    };
    let fits: Vec<_> = forms.iter().filter(|g| g.min() <= args.len() && g.max().is_none_or(|m| args.len() <= m)).collect();
    let mut e = if fits.is_empty() {
        let lo = forms.iter().map(Form::min).min().unwrap_or(0);
        let hi = forms.iter().map(Form::max).try_fold(0, |a, m| Some(a.max(m?)));
        let takes = match hi {
            Some(hi) if hi == lo => args_word(lo),
            Some(hi) => format!("{lo}-{hi} arguments"),
            None => format!("at least {}", args_word(lo)),
        };
        let first_extra = hi.filter(|h| *h < args.len()).map_or(span.clone(), at);
        let mut e = Error::new(format!("{} takes {takes}, got {}", f.name, args.len()), first_extra);
        for i in hi.unwrap_or(usize::MAX)..args.len() {
            e = e.label(at(i), "extra argument");
        }
        e
    } else {
        // The form that accepts the most leading arguments decides which one is wrong.
        let first_bad = |g: &Form| (0..args.len()).find(|&i| !g.types(i).iter().any(|t| is_type(&args[i], t)));
        match fits.iter().map(|g| first_bad(g)).max_by_key(|b| b.unwrap_or(usize::MAX)).flatten() {
            Some(i) => {
                let mut want: Vec<&str> = fits.iter().filter(|g| first_bad(g).is_some_and(|b| b >= i)).flat_map(|g| g.types(i).iter().copied()).collect();
                want.dedup();
                let got = args[i].type_name();
                Error::new(format!("{}: expected {}, got {got}", f.name, want.join(" or ").replace('|', " or ")), at(i))
                    .label(at(i), format!("this is {}", short(&args[i])))
            }
            None => {
                let types: Vec<_> = args.iter().map(Value::type_name).collect();
                Error::new(format!("{}: unsupported arguments ({})", f.name, types.join(", ")), span.clone())
            }
        }
    };
    for g in &forms {
        e = e.help(g.text);
    }
    e
}

#[cfg(test)]
mod tests {
    use super::forms;

    #[test]
    fn defaults() {
        let f = forms(r#"join(xs: list, sep: str = ", ", n?: int) -> str"#).unwrap();
        assert_eq!(f[0].params, vec![(vec!["list"], false), (vec!["str"], true), (vec!["int", "nil"], true)]);
        assert!(forms("f(x?: int|nil) -> nil").err().unwrap().contains("drop `|nil`"));
        assert!(forms("f(x?: int = 3) -> nil").err().unwrap().contains("write `x: int`"));
    }
}
