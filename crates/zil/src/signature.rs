//! Typed signatures like `round(x: num|quantity, digits?: int)`: parsing them, and saying which argument a failed call got wrong.

use crate::Error;
use crate::error::{args_word, short};
use crate::lexer::Span;
use crate::modules::Doc;
use crate::value::Value;

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
            for p in inner.split(", ").filter(|p| !p.is_empty()) {
                if form.rest {
                    return Err(format!("`...` must come last in `{text}`"));
                }
                if p == "..." {
                    form.rest = true;
                    continue;
                }
                let (name, ty) = p.split_once(": ").ok_or_else(|| format!("`{p}` in `{text}` has no type"))?;
                let types: Vec<_> = ty.split('|').collect();
                if let Some(t) = types.iter().find(|t| !TYPES.contains(t)) {
                    return Err(format!("unknown type `{t}` in `{text}`"));
                }
                form.params.push((types, name.ends_with('?')));
            }
            Ok(form)
        })
        .collect()
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
