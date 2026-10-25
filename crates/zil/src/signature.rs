//! Typed signatures like `round(x: num|quantity, digits: int = 0)`: the Rust types `#[zil_macros::builtins]` reads them
//! from, parsing them, and saying which argument a failed call got wrong.

use crate::Error;
use crate::error::{args_word, short};
use crate::lexer::Span;
use crate::modules::units::Unit;
use crate::modules::{Call, Fail};
use crate::value::{Table, Value};
use indexmap::{IndexMap, IndexSet};
use jiff::Zoned;
use num_bigint::BigInt;
use regex::Regex;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

/// A builtin param type: its name in a sig, and the value if it's of this type.
pub trait Arg<'a>: Sized {
    /// Unused for `Or`, whose name the macro spells out from its parts.
    const TYPE: &'static str;
    fn of(v: &'a Value) -> Option<Self>;
}

/// A builtin return type.
pub trait Ret {
    /// Unused for `Or`, `Option` and `Result`, whose names the macro spells out from their parts.
    const TYPE: &'static str;
    fn ret(self) -> Call;
}

/// Either type, shown `A|B`; tried in that order.
pub enum Or<A, B> {
    A(A),
    B(B),
}

macro_rules! or_n {
    ($($name:ident<$($t:ident),*>;)*) => {
        $(
            /// One of several types, shown `A|B|...`; tried in that order.
            pub enum $name<$($t),*> {
                $($t($t)),*
            }

            impl<'a, $($t: Arg<'a>),*> Arg<'a> for $name<$($t),*> {
                const TYPE: &'static str = "";
                fn of(v: &'a Value) -> Option<Self> {
                    None$(.or_else(|| $t::of(v).map($name::$t)))*
                }
            }

            impl<$($t: Ret),*> Ret for $name<$($t),*> {
                const TYPE: &'static str = "";
                fn ret(self) -> Call {
                    match self {
                        $($name::$t(x) => x.ret()),*
                    }
                }
            }
        )*
    };
}

or_n! {
    Or3<A, B, C>;
    Or4<A, B, C, D>;
    Or5<A, B, C, D, E>;
}

/// A command's flags (`rm -rf` passes `{r: true, f: true}` last), as a param that takes no argument. Shown `-rf`, from
/// `#[builtin(flags = "rf")]`.
pub struct Flags(pub String);

impl Flags {
    pub fn has(&self, c: char) -> bool {
        self.0.contains(c)
    }
}

/// The arguments left over, as a last param `*xs: T`.
pub struct Rest<T>(pub Vec<T>);

/// A number of any kind, kept exact.
pub struct Num(pub Value);

pub struct Qty(pub Value);

/// An int of any size, kept as a value so it keeps the base it shows in (`0xff`).
pub struct Int(pub Value);

/// Something callable: a closure or a builtin.
pub struct Func(pub Value);

pub struct Complex(pub f64, pub f64);

/// A returned value that's always a `T` (`As<Or<Num, Qty>>` shows as `num|quantity`), for results built as a `Value`.
pub struct As<T>(pub Value, PhantomData<T>);

impl<T> From<Value> for As<T> {
    fn from(v: Value) -> As<T> {
        As(v, PhantomData)
    }
}

macro_rules! deref_value {
    ($($t:ty),*) => {
        $(impl Deref for $t {
            type Target = Value;
            fn deref(&self) -> &Value {
                &self.0
            }
        })*
    };
}

deref_value!(Num, Qty, Int, Func);

impl Qty {
    pub fn get(&self) -> (f64, &Unit) {
        let Value::Qty(x, u) = &self.0 else { unreachable!("a Qty holds a quantity") };
        (*x, u)
    }
}

impl From<(f64, f64)> for Complex {
    fn from((re, im): (f64, f64)) -> Complex {
        Complex(re, im)
    }
}

/// Only a float, where `f64` takes any number.
pub struct Float(pub f64);

impl<A: Deref<Target = Value>, B: Deref<Target = Value>> Or<A, B> {
    /// The value either side holds.
    pub fn value(&self) -> &Value {
        match self {
            Or::A(a) => a,
            Or::B(b) => b,
        }
    }
}

pub type List = Rc<RefCell<Vec<Value>>>;
pub type Map = Rc<RefCell<IndexMap<String, Value>>>;
pub type Set = Rc<RefCell<IndexSet<Value>>>;

macro_rules! arg {
    ($($t:ty => $name:literal, |$v:ident| $e:expr;)*) => {
        $(impl<'a> Arg<'a> for $t {
            const TYPE: &'static str = $name;
            fn of($v: &'a Value) -> Option<Self> {
                $e
            }
        })*
    };
}

arg! {
    &'a str => "str", |v| if let Value::Str(s) = v { Some(s) } else { None };
    i64 => "int", |v| if let Value::Int(n, _) = v { Some(*n) } else { None };
    BigInt => "int", |v| match v {
        Value::Int(n, _) => Some((*n).into()),
        Value::Big(n, _) => Some((**n).clone()),
        _ => None,
    };
    &'a (BigInt, BigInt) => "mod", |v| if let Value::Mod(m) = v { Some(m) } else { None };
    &'a (f64, f64, Unit) => "uncertain", |v| if let Value::Unc(u) = v { Some(u) } else { None };
    bool => "bool", |v| if let Value::Bool(b) = v { Some(*b) } else { None };
    f64 => "num", |v| crate::value::num(v);
    Float => "float", |v| if let Value::Float(x) = v { Some(Float(*x)) } else { None };
    Num => "num", |v| matches!(v, Value::Int(..) | Value::Big(..) | Value::Frac(..) | Value::Float(_)).then(|| Num(v.clone()));
    Qty => "quantity", |v| matches!(v, Value::Qty(..)).then(|| Qty(v.clone()));
    Int => "int", |v| matches!(v, Value::Int(..) | Value::Big(..)).then(|| Int(v.clone()));
    Complex => "complex", |v| if let Value::Cplx(re, im) = v { Some(Complex(*re, *im)) } else { None };
    &'a [u8] => "bytes", |v| if let Value::Bytes(b) = v { Some(b) } else { None };
    &'a Regex => "regex", |v| if let Value::Regex(r) = v { Some(r) } else { None };
    &'a Zoned => "date", |v| if let Value::Date(d) = v { Some(d) } else { None };
    &'a List => "list", |v| if let Value::List(l) = v { Some(l) } else { None };
    &'a Map => "map", |v| if let Value::Map(m) = v { Some(m) } else { None };
    &'a Set => "set", |v| if let Value::Set(s) = v { Some(s) } else { None };
    &'a Rc<Table> => "table", |v| if let Value::Table(t) = v { Some(t) } else { None };
    Func => "fn", |v| matches!(v, Value::Fn(_) | Value::Builtin(..)).then(|| Func(v.clone()));
    &'a Value => "any", |v| Some(v);
}

impl<'a, A: Arg<'a>, B: Arg<'a>> Arg<'a> for Or<A, B> {
    const TYPE: &'static str = "";
    fn of(v: &'a Value) -> Option<Self> {
        A::of(v).map(Or::A).or_else(|| B::of(v).map(Or::B))
    }
}

/// An `x?: T` argument: missing or nil is `None`, anything else must be a `T`.
pub fn opt<'a, T: Arg<'a>>(v: Option<&'a Value>) -> Option<Option<T>> {
    match v {
        None | Some(Value::Nil) => Some(None),
        Some(v) => T::of(v).map(Some),
    }
}

/// An `x: T = d` argument.
pub fn or<'a, T: Arg<'a>>(v: Option<&'a Value>, d: T) -> Option<T> {
    v.map_or(Some(d), T::of)
}

/// A `*xs: T` argument: every one must be a `T`.
pub fn rest<'a, T: Arg<'a>>(vs: &'a [Value]) -> Option<Rest<T>> {
    vs.iter().map(T::of).collect::<Option<_>>().map(Rest)
}

macro_rules! ret {
    ($($t:ty => $name:literal, |$x:ident| $e:expr;)*) => {
        $(impl Ret for $t {
            const TYPE: &'static str = $name;
            fn ret(self) -> Call {
                let $x = self;
                Ok($e)
            }
        })*
    };
}

ret! {
    Value => "any", |v| v;
    String => "str", |s| Value::str(s);
    &'static str => "str", |s| Value::str(s);
    i64 => "int", |n| Value::int(n);
    f64 => "float", |x| Value::Float(x);
    bool => "bool", |b| Value::Bool(b);
    Vec<u8> => "bytes", |b| Value::bytes(b);
    Zoned => "date", |d| Value::date(d);
    Vec<Value> => "list", |l| Value::list(l);
    List => "list", |l| Value::List(l);
    IndexMap<String, Value> => "map", |m| Value::map(m);
    Map => "map", |m| Value::Map(m);
    () => "nil", |_u| Value::Nil;
    Complex => "complex", |z| crate::modules::math::complex::mk((z.0, z.1));
    Set => "set", |s| Value::Set(s);
    Table => "table", |t| Value::table(t);
    Rc<Table> => "table", |t| Value::Table(t);
    BigInt => "int", |n| crate::modules::math::big(n);
    (BigInt, BigInt) => "mod", |m| Value::modular(m.0, m.1);
    num_rational::BigRational => "frac", |r| crate::value::exact(r, crate::ast::Radix::DEC, true);
    IndexSet<Value> => "set", |s| Value::set(s);
    Num => "num", |n| n.0;
    Qty => "quantity", |q| q.0;
    Int => "int", |n| n.0;
    Func => "fn", |f| f.0;
    Float => "float", |x| Value::Float(x.0);
}

impl<T> Ret for As<T> {
    const TYPE: &'static str = "";
    fn ret(self) -> Call {
        Ok(self.0)
    }
}

impl<T: Ret> Ret for Option<T> {
    const TYPE: &'static str = "";
    fn ret(self) -> Call {
        self.map_or(Ok(Value::Nil), T::ret)
    }
}

impl<A: Ret, B: Ret> Ret for Or<A, B> {
    const TYPE: &'static str = "";
    fn ret(self) -> Call {
        match self {
            Or::A(a) => a.ret(),
            Or::B(b) => b.ret(),
        }
    }
}

impl<T: Ret, E: Into<Fail>> Ret for Result<T, E> {
    const TYPE: &'static str = "";
    fn ret(self) -> Call {
        self.map_err(Into::into)?.ret()
    }
}

/// The length of `parts` joined, for building a sig at compile time.
pub const fn concat_len(parts: &[&str]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i < parts.len() {
        // An empty name is a compound type (`Or`, `As`) the macro couldn't see through, like a type alias of one.
        assert!(!parts[i].is_empty(), "a sig type has no name: spell out Or/As/Option/Result types instead of aliasing them");
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

/// One form of a typed signature, like `round(x: num|quantity, digits: int = 0) -> num|quantity`; a last `*xs: T` takes any
/// number more, and `-rf` a trailing map of command flags.
pub struct Form<'a> {
    pub text: &'a str,
    /// Each parameter's types, and whether it's optional.
    pub params: Vec<(Vec<&'a str>, bool)>,
    /// The last param takes the rest.
    pub rest: bool,
}

impl<'a> Form<'a> {
    fn min(&self) -> usize {
        self.params.iter().filter(|p| !p.1).count()
    }

    fn max(&self) -> Option<usize> {
        (!self.rest).then_some(self.params.len())
    }

    fn types(&self, i: usize) -> &[&'a str] {
        self.params.get(i).or(self.params.last().filter(|_| self.rest)).map_or(&[], |p| &p.0)
    }
}

/// The forms of a `Doc.sig`, separated by ` / `.
pub fn forms(sig: &str) -> Result<Vec<Form<'_>>, String> {
    sig.split(" / ")
        .map(|text| {
            let (call, ret) = text.rsplit_once(" -> ").ok_or_else(|| format!("`{text}` has no return type"))?;
            if let Some(t) = ret.split('|').find(|t| !TYPES.contains(t)) {
                return Err(format!("unknown return type `{t}` in `{text}`"));
            }
            let inner = call.split_once('(').and_then(|(_, r)| r.strip_suffix(')')).ok_or_else(|| format!("`{text}` is not `name(params)`"))?;
            let mut form = Form { text, params: vec![], rest: false };
            for p in params(inner) {
                // `-rf`: a trailing map of flags, after any rest param.
                if p.strip_prefix('-').is_some_and(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_alphabetic())) {
                    if !form.rest {
                        form.params.push((vec!["map", "nil"], true));
                    }
                    continue;
                }
                if form.rest {
                    return Err(format!("a `*` param must come last in `{text}`"));
                }
                let p = match p.strip_prefix('*') {
                    Some(p) => {
                        form.rest = true;
                        p
                    }
                    None => p,
                };
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
                form.params.push((types, short || default.is_some() || form.rest));
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

/// The first argument no form of `name` accepts, or a wrong count; `help:` lists every form.
pub fn bad_args(name: &str, forms: &[Form], args: &[Value], span: &Span, at: &dyn Fn(usize) -> Span) -> Error {
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
        let mut e = Error::new(format!("{name} takes {takes}, got {}", args.len()), first_extra);
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
                Error::new(format!("{name}: expected {}, got {got}", want.join(" or ").replace('|', " or ")), at(i))
                    .label(at(i), format!("this is {}", short(&args[i])))
            }
            None => {
                let types: Vec<_> = args.iter().map(Value::type_name).collect();
                Error::new(format!("{name}: unsupported arguments ({})", types.join(", ")), span.clone())
            }
        }
    };
    for g in forms {
        e = e.help(g.text);
    }
    e
}

#[cfg(test)]
mod tests {
    use super::forms;

    #[test]
    fn defaults() {
        let f = forms(r#"join(xs: list, sep: str = ", ", n?: int, *more: str) -> str"#).unwrap();
        assert_eq!(f[0].params, vec![(vec!["list"], false), (vec!["str"], true), (vec!["int", "nil"], true), (vec!["str"], true)]);
        assert_eq!((f[0].min(), f[0].max()), (1, None));
        assert!(forms("f(x?: int|nil) -> nil").err().unwrap().contains("drop `|nil`"));
        assert!(forms("f(x?: int = 3) -> nil").err().unwrap().contains("write `x: int`"));
    }
}
