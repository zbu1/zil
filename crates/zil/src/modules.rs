//! Modules group builtins with everything else a feature owns: docs, operators, ordering,
//! `to` conversions and magic identifiers. All are always loaded and share one flat namespace:
//! nesting only shapes source files, `help` and the docs, never how a name resolves.
//!
//! Adding a module:
//! 1. Write `src/modules/<path>.rs` with a `pub const MODULE`; children go in `src/modules/<path>/<child>.rs`.
//! 2. List it in its parent's `children` (or in `TREE` for a root). Help and the docs follow the tree.
//! 3. If it has an `ident`, `binary`, `compare` or `convert` hook, add it to `DISPATCH` too.
//!
//! A `pub mod` under a module is a child `Module`; a private `mod` is just a helper file of its parent
//! (`units/table.rs`, `time/parsing.rs`, `art/img.rs`), whose functions the parent documents and calls.
//!
//! Hooks return `None` for "not mine"; dispatch tries modules in `DISPATCH` order, so order matters:
//! text before units (`"a" + 5 km` concatenates), time before units (`date + 1 d` is calendar math).
//! `DISPATCH` is separate from the tree so moving a module for docs reasons never changes behavior.

pub mod art;
pub mod core;
pub mod data;
pub mod dev;
pub mod finance;
pub mod fs;
pub mod geo;
pub mod math;
pub mod sys;
pub mod text;
pub mod time;
pub mod units;

use crate::Error;
use crate::ast::{BinOp, Target};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::value::Value;
use std::cmp::Ordering;
use std::sync::LazyLock;

/// A builtin's help.
pub struct Doc {
    pub name: &'static str,
    pub sig: &'static str,
    pub desc: &'static str,
    /// Run live by `help`.
    pub examples: &'static [&'static str],
    pub see: &'static [&'static str],
    /// Shown but never run, for fns with side effects: `doc(...).shown(&[...])`.
    pub shown: &'static [&'static str],
    /// Changes its arguments or the session in place, like `push`: completion never runs it, nor a `shown` fn.
    pub mutates: bool,
    /// Its help page shows its module's "pretty" section: set on the fn that makes the module's type, like `date`.
    pub pretty: bool,
}

/// Keeps the `FNS` tables one positional row per builtin.
pub const fn doc(name: &'static str, sig: &'static str, desc: &'static str, examples: &'static [&'static str], see: &'static [&'static str]) -> Doc {
    Doc { name, sig, desc, examples, see, shown: &[], mutates: false, pretty: false }
}

impl Doc {
    pub const fn shown(self, shown: &'static [&'static str]) -> Doc {
        Doc { shown, ..self }
    }

    pub const fn mutates(self) -> Doc {
        Doc { mutates: true, ..self }
    }

    pub const fn pretty(self) -> Doc {
        Doc { pretty: true, ..self }
    }
}

/// A help page section: heading, then (label, example) rows; examples run live, and an empty one prints the label alone.
pub type Section = (&'static str, &'static [(&'static str, &'static str)]);

/// A hook's answer: `None` means "not mine, ask the next module".
pub type Claim = Option<Result<Value, String>>;

/// Why a builtin call failed; `Fail::error` turns it into an `Error` naming the builtin.
pub enum Fail {
    Msg(String),
    /// Blames argument `i`, counting `x` in `x.f(a)` as 0, so the underline lands on it.
    Arg(usize, String),
    /// Already located, e.g. from a callback: passed through untouched.
    Err(Error),
    /// The arguments fit no form of the signature; `Fail::error` works out which one is wrong.
    BadArgs,
}

impl From<String> for Fail {
    fn from(s: String) -> Fail {
        Fail::Msg(s)
    }
}

impl From<&str> for Fail {
    fn from(s: &str) -> Fail {
        Fail::Msg(s.into())
    }
}

impl From<Error> for Fail {
    fn from(e: Error) -> Fail {
        Fail::Err(e)
    }
}

impl Fail {
    /// `arg_spans` may be short (callbacks, `to` targets); missing ones fall back to the call's span.
    pub fn error(self, f: &Doc, args: &[Value], span: &Span, arg_spans: &[Span]) -> Error {
        let at = |i: usize| arg_spans.get(i).unwrap_or(span).clone();
        match self {
            Fail::Msg(m) => Error::new(format!("{}: {m}", f.name), span.clone()),
            Fail::Arg(i, m) => Error::new(format!("{}: {m}", f.name), at(i)),
            Fail::Err(e) => e,
            Fail::BadArgs => crate::signature::bad_args(f, args, span, &at),
        }
    }
}

pub type Call = Result<Value, Fail>;

pub struct Module {
    /// Short name, unique across the tree; the dotted path (`math.trig`) comes from where it sits.
    pub name: &'static str,
    /// One line under the module's help page title.
    pub about: &'static str,
    /// `help("examples")` sections: the module's less obvious tricks, each run live.
    pub examples: &'static [Section],
    /// Help page sections; the labels of a "types" section also show in the overview.
    pub guide: &'static [Section],
    /// Exported builtins and their help.
    pub fns: &'static [Doc],
    /// Function names by category for the help page; every fn in exactly one. Empty lists them all on one line.
    pub groups: &'static [(&'static str, &'static [&'static str])],
    pub call: fn(&mut Interp, &'static str, &[Value], &Span) -> Call,
    pub consts: &'static [(&'static str, f64)],
    /// Constants with units: name, value, unit terms like `m s^-1`.
    pub quantities: &'static [(&'static str, f64, &'static str)],
    /// `to` keywords and the exported fn each calls: `x to UTC` is `utc(x)`, `x to hex(8)` is `hex(x, 8)`.
    pub targets: &'static [(&'static str, &'static str)],
    /// Names that aren't variables, tried after scope lookup fails (`now`, units).
    pub ident: Option<fn(&mut Interp, &str) -> Claim>,
    pub binary: Option<fn(BinOp, &Value, &Value) -> Claim>,
    pub compare: Option<fn(&Value, &Value) -> Option<Ordering>>,
    /// Conversions to targets that aren't keywords: units (`to km`), strings (`to "Europe/Paris"`).
    pub convert: Option<fn(&Value, &Target) -> Claim>,
    /// Extra help topics; prints and returns true if it knows `topic`.
    pub topic: Option<fn(&str) -> bool>,
    /// Unit rows it adds, listed by kind on its help and book pages.
    pub units: &'static [units::Row],
    /// Submodules, shown under it in help and the docs.
    pub children: &'static [Module],
}

impl Module {
    pub const EMPTY: Module = Module {
        name: "",
        about: "",
        examples: &[],
        guide: &[],
        fns: &[],
        groups: &[],
        call: |_, _, _, _| Err(Fail::BadArgs),
        consts: &[],
        quantities: &[],
        targets: &[],
        ident: None,
        binary: None,
        compare: None,
        convert: None,
        topic: None,
        units: &[],
        children: &[],
    };
}

/// Root modules, in `help()` order.
pub const TREE: &[Module] = &[
    core::MODULE,
    fs::MODULE,
    sys::MODULE,
    text::MODULE,
    data::MODULE,
    math::MODULE,
    units::MODULE,
    finance::MODULE,
    time::MODULE,
    geo::MODULE,
    dev::MODULE,
    art::MODULE,
];

/// Modules with hooks, in the order hooks are tried.
pub const DISPATCH: &[&Module] = &[
    &text::MODULE,
    &data::lists::MODULE,
    &data::maps::MODULE,
    &data::sets::MODULE,
    &math::complex::MODULE,
    // Before units, which would otherwise claim `qty * uncertain` and fail.
    &math::uncertainty::MODULE,
    &math::bits::MODULE,
    &math::numtheory::MODULE,
    &time::MODULE,
    &time::zones::MODULE,
    &geo::MODULE,
    &units::MODULE,
    &core::MODULE,
    &dev::MODULE,
];

/// Every module with its dotted path, each parent before its children, in tree order.
pub static ALL: LazyLock<Vec<(String, &'static Module)>> = LazyLock::new(|| {
    fn walk(ms: &'static [Module], prefix: &str, out: &mut Vec<(String, &'static Module)>) {
        for m in ms {
            let path = if prefix.is_empty() { m.name.to_string() } else { format!("{prefix}.{}", m.name) };
            out.push((path.clone(), m));
            walk(m.children, &path, out);
        }
    }
    let mut out = Vec::new();
    walk(TREE, "", &mut out);
    out
});

pub fn modules() -> impl Iterator<Item = &'static Module> {
    ALL.iter().map(|(_, m)| *m)
}

/// A module by dotted path or short name.
pub fn find(name: &str) -> Option<&'static Module> {
    ALL.iter().find(|(path, m)| path == name || m.name == name).map(|(_, m)| *m)
}

/// `m`'s dotted path.
pub fn path(m: &Module) -> &'static str {
    &ALL.iter().find(|(_, o)| o.name == m.name).expect("module in the tree").0
}

/// A builtin by name, ignoring any variable that shadows it.
pub fn builtin(name: &str) -> Value {
    modules().find_map(|m| m.fns.iter().find(|f| f.name == name).map(|f| Value::Builtin(m, f.name))).expect("known builtin")
}

/// The module and fn a `to` keyword calls.
pub fn target(keyword: &str) -> Option<(&'static Module, &'static str)> {
    modules().find_map(|m| m.targets.iter().find(|t| t.0 == keyword).map(|t| (m, t.1)))
}

pub fn ident(it: &mut Interp, name: &str) -> Claim {
    DISPATCH.iter().filter_map(|m| m.ident).find_map(|f| f(it, name))
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    DISPATCH.iter().filter_map(|m| m.binary).find_map(|f| f(op, a, b))
}

pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    DISPATCH.iter().filter_map(|m| m.compare).find_map(|f| f(a, b))
}

pub fn convert(v: &Value, t: &Target) -> Claim {
    DISPATCH.iter().filter_map(|m| m.convert).find_map(|f| f(v, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry() {
        let mut seen = std::collections::HashMap::new();
        let mut names = std::collections::HashSet::new();
        let fns: Vec<_> = modules().flat_map(|m| m.fns).collect();
        for m in modules() {
            assert!(names.insert(m.name), "two modules named {}", m.name);
            assert!(!m.about.is_empty(), "{}: empty about", m.name);
            assert!(!m.guide.is_empty() || !m.examples.is_empty() || !m.children.is_empty(), "{}: needs a guide, examples or children", m.name);
            let hooked = m.ident.is_some() || m.binary.is_some() || m.compare.is_some() || m.convert.is_some();
            let listed = DISPATCH.iter().filter(|d| d.name == m.name).count();
            assert_eq!(listed, usize::from(hooked), "{}: modules with hooks are in DISPATCH once, others never", m.name);
            for f in m.fns {
                assert!(!modules().any(|o| o.name == f.name), "fn {} collides with a module name, hiding its help page", f.name);
                if let Some(other) = seen.insert(f.name, m.name) {
                    panic!("{} exported by both {other} and {}", f.name, m.name);
                }
                assert!(!f.examples.is_empty() || !f.shown.is_empty(), "{}: no examples", f.name);
                let fs = crate::signature::forms(f.sig).unwrap_or_else(|e| panic!("{}: {e}", f.name));
                for g in fs {
                    assert!(g.text.starts_with(&format!("{}(", f.name)), "{}: form `{}` names another fn", f.name, g.text);
                }
                for s in f.see {
                    assert!(fns.iter().any(|g| g.name == *s), "{}: see also {s} missing", f.name);
                }
            }
            for (name, _, terms) in m.quantities {
                assert!(units::terms(terms).is_ok(), "{name}: bad unit terms `{terms}`");
                let taken = fns.iter().any(|f| f.name == *name) || units::lookup(name).is_ok() || modules().any(|o| o.consts.iter().any(|c| c.0 == *name));
                assert!(!taken, "constant {name} collides with a fn, unit or constant");
            }
            for (kw, f) in m.targets {
                assert!(m.fns.iter().any(|d| d.name == *f), "{}: target {kw} calls {f}, which it doesn't export", m.name);
                let is_unit = units::rows().any(|row| row.0.split(' ').any(|n| n == *kw));
                assert!(!is_unit, "target {kw} would shadow the unit `to {kw}`");
            }
            let grouped: Vec<_> = m.groups.iter().flat_map(|g| g.1.iter()).collect();
            for f in m.fns.iter().filter(|_| !m.groups.is_empty()) {
                assert_eq!(grouped.iter().filter(|g| **g == &f.name).count(), 1, "{}: {} must be in exactly one group", m.name, f.name);
            }
            for g in grouped {
                assert!(m.fns.iter().any(|f| f.name == *g), "{}: group lists unknown fn {g}", m.name);
            }
        }
        assert_eq!(find("math.trig").map(|m| m.name), Some("trig"));
        assert_eq!(find("trig").map(path), Some("math.trig"));
    }
}
