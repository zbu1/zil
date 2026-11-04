//! The embedding API: one sandboxed session of zil code.

use crate::ast::{Expr, ExprKind};
use crate::host::{Host, NullHost, Permission, mask};
use crate::interp::{Interp, Limits, lookup, names};
use crate::modules::{HostFn, HostHooks, UserDoc};
use crate::signature;
use crate::{Data, Error, Settings, Value};
use std::cell::RefMut;
use std::cmp::Ordering;
use std::ops::Range;
use std::rc::Rc;

/// A zil session: globals, user units and rates that persist across `eval`s, with every effect going through `host`.
///
/// ```
/// use std::rc::Rc;
/// let mut s = zil::Session::new(Rc::new(zil::host::NullHost));
/// s.limits().steps = 1_000_000;
/// s.set("n", zil::Value::int(20));
/// assert_eq!(s.eval("n * 2 + 2").unwrap().to_string(), "42");
/// ```
pub struct Session(pub(crate) Interp);

impl Session {
    /// A fresh session on `host`, with every permission the host permits. `host::NullHost` denies every capability, for
    /// untrusted code.
    pub fn new(host: Rc<dyn Host>) -> Session {
        Session(Interp::with_host(host))
    }

    /// A session set up in one go; without `host`, on `NullHost`, and without `permissions`, none.
    ///
    /// ```
    /// let mut s = zil::Session::builder()
    ///     .limits(zil::Limits { steps: 1_000_000, ..Default::default() })
    ///     .settings(zil::Settings { precision: 4, ..Default::default() })
    ///     .prelude("tax = |x| x * 1.0825")
    ///     .build()
    ///     .unwrap();
    /// let v = s.eval("tax(pi)").unwrap();
    /// assert_eq!(s.display(&v), "3.401");
    /// ```
    pub fn builder() -> Builder {
        Builder {
            host: Rc::new(NullHost),
            permissions: 0,
            limits: Limits::default(),
            settings: Settings::default(),
            prelude: vec![],
            module: HostModule::default(),
        }
    }

    /// The session as JSON, for `load` to make again without running anything: variables (functions with what they
    /// captured, and values they share still shared), user units, random numbers and the network switch. Limits and
    /// `set_args` aren't kept.
    pub fn save(&self) -> String {
        crate::snapshot::save(&self.0).to_string()
    }

    /// The session `save` gave, on `host`.
    pub fn load(host: Rc<dyn Host>, saved: &str) -> Result<Session, String> {
        let j = serde_json::from_str(saved).map_err(|e| format!("it isn't a saved zil session: {e}"))?;
        let mut s = Session::new(host);
        crate::snapshot::load(&mut s.0, &j)?;
        Ok(s)
    }

    /// Runs `src`, returning the last statement's value.
    pub fn eval(&mut self, src: &str) -> Result<Value, Error> {
        crate::run(&mut self.0, src)
    }

    /// Runs one statement from `zil::parse`, e.g. to show each result.
    pub fn run(&mut self, stmt: &Statement) -> Result<Value, Error> {
        self.0.run(std::slice::from_ref(&stmt.0))
    }

    /// Calls a function value (from `eval` or `get`) with `args`, under the same limits as `eval`.
    pub fn call(&mut self, f: &Value, args: Vec<Value>) -> Result<Value, Error> {
        if !matches!(f, Value::Fn(_) | Value::Named(..)) {
            return Err(Error::new(format!("a {} isn't a function", f.type_name()), 0..0));
        }
        self.0.begin();
        self.0.call(f, args, &(0..0))
    }

    /// What the script's `args()` returns.
    pub fn set_args(&mut self, args: Vec<String>) {
        self.0.args = args
    }

    /// Sets a global variable.
    pub fn set(&mut self, name: &str, v: Value) {
        self.0.set_global(name, v)
    }

    /// Removes a global variable.
    pub fn unset(&mut self, name: &str) {
        self.0.unset_global(name)
    }

    /// A global variable or builtin.
    pub fn get(&self, name: &str) -> Option<Value> {
        lookup(&self.0.globals(), name)
    }

    /// Every global variable's name, builtins included.
    pub fn names(&self) -> Vec<String> {
        names(&self.0.globals())
    }

    /// Runs `f` with this session's units (user units too) and host in scope, for unit lookups outside `eval`.
    pub fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        crate::state::enter(&self.0.state(), f)
    }

    /// `v` as this session shows it: its settings and host time zone, which plain `to_string` only sees inside `eval`.
    pub fn display(&self, v: &Value) -> String {
        self.enter(|| v.to_string())
    }

    /// How results show, e.g. `s.settings().precision = 10`.
    pub fn settings(&self) -> RefMut<'_, Settings> {
        self.0.state.settings.borrow_mut()
    }

    /// The value `d` describes, with units resolved in this session (so user units work too).
    pub fn from_data(&self, d: Data) -> Result<Value, String> {
        self.enter(|| Value::from_data(d))
    }

    /// `e`'s value if computing it changes nothing, for completion; see `Interp::probe`.
    pub(crate) fn probe(&self, e: &Expr) -> Option<Value> {
        self.0.probe(e)
    }

    /// Grants nothing until the code declares what it needs with `config.permissions`, as a script file must; the REPL
    /// and `zil -e` don't.
    pub fn as_script(&mut self) {
        self.0.state.gate.grants.set(0)
    }

    /// Caps on each `eval`; hitting one is an `Error` whose `limit` says which.
    pub fn limits(&mut self) -> &mut Limits {
        &mut self.0.limits
    }
}

impl HostModule {
    /// A function written in Rust, e.g. `.function("twice(x: num) -> num", "x doubled", |ctx, a| ...)`. Calls whose arguments fit `sig`
    /// run it (after any the user defines, before the builtin of that name); an `Err` becomes the call's error.
    pub fn function(mut self, sig: &str, desc: &str, run: impl Fn(&mut Ctx, &[Value]) -> Result<Value, String> + 'static) -> HostModule {
        let name = sig.split('(').next().unwrap_or(sig).to_string();
        self.functions.push((name, HostFn { sig: sig.into(), desc: desc.into(), run: Box::new(run) }));
        self
    }

    /// A global variable the session starts with.
    pub fn constant(mut self, name: &str, v: Value) -> HostModule {
        self.constants.push((name.into(), v));
        self
    }

    /// What `help(name)` shows for a host function: a description, examples (code that help runs) and tags, as `doc(f, ...)` does.
    pub fn doc(mut self, name: &str, desc: &str, examples: &[&str], tags: &[&str]) -> HostModule {
        let strs = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect();
        self.docs.push((name.into(), UserDoc { desc: desc.into(), examples: strs(examples), tags: strs(tags) }));
        self
    }

    /// A unit the session starts with, e.g. `.unit("sprint", "2 wk")`; an empty `definition` makes a new base unit.
    pub fn unit(mut self, name: &str, definition: &str) -> HostModule {
        self.units.push(format!("unit {name} = {definition}").trim_end().trim_end_matches('=').trim_end().to_string());
        self
    }

    /// Answers names that aren't variables, as `now` and units are answered: `None` leaves `name` to the builtins. Completion never runs it.
    pub fn ident(mut self, f: impl Fn(&mut Ctx, &str) -> Option<Result<Value, String>> + 'static) -> HostModule {
        self.hooks.ident.push(Box::new(f));
        self
    }

    /// Gives an operator a meaning for values the builtins don't handle: `f(op, a, b)` gets it as written (`"+"`, `"**"`) and `None` leaves it to
    /// the builtins. Runs for the operators a user `op` can define, other than comparisons, which go through `compare`.
    pub fn binary(mut self, f: impl Fn(&str, &Value, &Value) -> Option<Result<Value, String>> + 'static) -> HostModule {
        self.hooks.binary.push(Box::new(f));
        self
    }

    /// Orders values the builtins don't, for `<` and `sort`; `None` leaves them to the builtins. Ints, strings and bytes always compare as usual.
    pub fn compare(mut self, f: impl Fn(&Value, &Value) -> Option<Ordering> + 'static) -> HostModule {
        self.hooks.compare.push(Box::new(f));
        self
    }

    /// Handles `v to name` and `v to "name"`: `None` leaves it to the builtins. `to` keywords like `hex` stay the builtins'.
    pub fn convert(mut self, f: impl Fn(&Value, &str) -> Option<Result<Value, String>> + 'static) -> HostModule {
        self.hooks.convert.push(Box::new(f));
        self
    }
}

/// What a host function can do while it runs: call zil functions and use globals.
pub struct Ctx<'a>(pub(crate) &'a mut Interp);

impl Ctx<'_> {
    /// `d` as a value in the running session.
    pub fn from_data(&self, d: Data) -> Result<Value, String> {
        Value::from_data(d)
    }

    /// Calls a function value, e.g. a lambda passed as an argument, under the running call's limits.
    pub fn call(&mut self, f: &Value, args: Vec<Value>) -> Result<Value, String> {
        self.0.call_at(f, args, &(0..0), &[], None).map_err(|e| e.msg)
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        lookup(&self.0.globals(), name)
    }

    pub fn set(&mut self, name: &str, v: Value) {
        self.0.set_global(name, v)
    }
}

/// Sets up a `Session`; from `Session::builder`.
pub struct Builder {
    host: Rc<dyn Host>,
    permissions: u16,
    limits: Limits,
    settings: Settings,
    prelude: Vec<String>,
    module: HostModule,
}

/// Functions, constants, units, docs and hooks an embedder ships together; `Builder::module` adds one, and the same methods exist on
/// `Builder` itself.
#[derive(Default)]
pub struct HostModule {
    functions: Vec<(String, HostFn)>,
    constants: Vec<(String, Value)>,
    units: Vec<String>,
    docs: Vec<(String, UserDoc)>,
    hooks: HostHooks,
}

impl Builder {
    pub fn host(mut self, host: Rc<dyn Host>) -> Builder {
        self.host = host;
        self
    }

    /// What the session may do outside itself, within what the host permits. Code can narrow it with
    /// `config.permissions`, never widen it.
    pub fn permissions(mut self, permissions: impl IntoIterator<Item = Permission>) -> Builder {
        self.permissions = mask(permissions);
        self
    }

    pub fn limits(mut self, limits: Limits) -> Builder {
        self.limits = limits;
        self
    }

    pub fn settings(mut self, settings: Settings) -> Builder {
        self.settings = settings;
        self
    }

    /// Code `build` runs first, under the limits, e.g. to define functions; several run in order.
    pub fn prelude(mut self, src: &str) -> Builder {
        self.prelude.push(src.into());
        self
    }

    /// Everything `module` holds, as if added here.
    pub fn module(mut self, module: HostModule) -> Builder {
        self.module.functions.extend(module.functions);
        self.module.constants.extend(module.constants);
        self.module.units.extend(module.units);
        self.module.docs.extend(module.docs);
        self.module.hooks.ident.extend(module.hooks.ident);
        self.module.hooks.binary.extend(module.hooks.binary);
        self.module.hooks.compare.extend(module.hooks.compare);
        self.module.hooks.convert.extend(module.hooks.convert);
        self
    }

    /// See `HostModule::function`.
    pub fn function(mut self, sig: &str, desc: &str, run: impl Fn(&mut Ctx, &[Value]) -> Result<Value, String> + 'static) -> Builder {
        self.module = self.module.function(sig, desc, run);
        self
    }

    /// A global variable the session starts with.
    pub fn constant(mut self, name: &str, v: Value) -> Builder {
        self.module = self.module.constant(name, v);
        self
    }

    /// See `HostModule::doc`.
    pub fn doc(mut self, name: &str, desc: &str, examples: &[&str], tags: &[&str]) -> Builder {
        self.module = self.module.doc(name, desc, examples, tags);
        self
    }

    /// A unit the session starts with, e.g. `.unit("sprint", "2 wk")`; an empty `definition` makes a new base unit.
    pub fn unit(mut self, name: &str, definition: &str) -> Builder {
        self.module = self.module.unit(name, definition);
        self
    }

    /// See `HostModule::ident`.
    pub fn ident(mut self, f: impl Fn(&mut Ctx, &str) -> Option<Result<Value, String>> + 'static) -> Builder {
        self.module = self.module.ident(f);
        self
    }

    /// See `HostModule::binary`.
    pub fn binary(mut self, f: impl Fn(&str, &Value, &Value) -> Option<Result<Value, String>> + 'static) -> Builder {
        self.module = self.module.binary(f);
        self
    }

    /// See `HostModule::compare`.
    pub fn compare(mut self, f: impl Fn(&Value, &Value) -> Option<Ordering> + 'static) -> Builder {
        self.module = self.module.compare(f);
        self
    }

    /// See `HostModule::convert`.
    pub fn convert(mut self, f: impl Fn(&Value, &str) -> Option<Result<Value, String>> + 'static) -> Builder {
        self.module = self.module.convert(f);
        self
    }

    /// The session, or the first error a prelude hit.
    pub fn build(self) -> Result<Session, Error> {
        let mut s = Session::new(self.host);
        s.0.state.gate.policy.set(self.permissions);
        s.0.state.gate.grants.set(self.permissions);
        *s.limits() = self.limits;
        *s.settings() = self.settings;
        let m = self.module;
        for (name, f) in m.functions {
            signature::forms(&f.sig).map_err(|e| Error::new(format!("host function `{name}`: {e}"), 0..0))?;
            let sig = f.sig.clone();
            let id = s.0.state.fns.add_host(&name, f);
            let mine = signature::forms(&sig).expect("checked above");
            let why = crate::state::enter(&s.0.state, || id.overloads()).iter().find_map(|o| mine.iter().find_map(|m| signature::clash(&name, m, o.doc.sig)));
            if let Some(why) = why {
                s.0.state.host.note(&format!("warning: host function {sig} {why}"));
            }
            if !matches!(s.get(&name), Some(Value::Named(_))) {
                s.set(&name, Value::Named(id));
            }
        }
        for (name, d) in m.docs {
            match s.get(&name) {
                Some(Value::Named(id)) if !s.0.state.fns.hosts_of(&name).is_empty() => crate::state::enter(&s.0.state, || id.set_doc(d)),
                _ => return Err(Error::new(format!("no host function `{name}` to document"), 0..0)),
            }
        }
        for (name, v) in m.constants {
            s.set(&name, v);
        }
        *s.0.state.hooks.borrow_mut() = Rc::new(m.hooks);
        for src in m.units.iter().chain(&self.prelude) {
            s.eval(src)?;
        }
        Ok(s)
    }
}

/// One top-level statement of a program, from `zil::parse`.
pub struct Statement(pub(crate) Expr);

impl Statement {
    /// Where it sits in the source, in bytes.
    pub fn span(&self) -> Range<usize> {
        self.0.span.clone()
    }

    /// A `for` or `while` loop, whose value is just its last pass.
    pub fn is_loop(&self) -> bool {
        matches!(self.0.kind, ExprKind::For(..) | ExprKind::While(..))
    }

    /// An assignment, `x = 1` or `a, b = ...`.
    pub fn is_assignment(&self) -> bool {
        matches!(self.0.kind, ExprKind::Assign(..) | ExprKind::Unpack(..))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Limit;
    use crate::host::NullHost;
    use std::cell::RefCell;

    #[test]
    fn embeds() {
        let mut s = Session::new(Rc::new(NullHost));
        s.eval("f = |x| x * 2").unwrap();
        s.set("y", Value::int(21));
        assert_eq!(s.eval("f(y)").unwrap().to_string(), "42");
        assert!(s.get("f").is_some() && s.get("nope").is_none());
        let f = s.get("f").unwrap();
        assert_eq!(s.call(&f, vec![Value::int(4)]).unwrap().to_string(), "8");
        assert_eq!(s.call(&s.get("max").unwrap(), vec![Value::int(1), Value::int(3)]).unwrap().to_string(), "3");
        assert!(s.call(&Value::int(1), vec![]).is_err());
        s.limits().steps = 1000;
        assert_eq!(s.eval("while true {}").unwrap_err().limit, Some(Limit::Steps));
        assert!(s.eval(r#"read_file("x")"#).is_err());
        let built = Session::builder().limits(Limits { steps: 1000, ..Default::default() }).prelude("while true {}").build();
        assert_eq!(built.err().and_then(|e| e.limit), Some(Limit::Steps));
        assert!(Session::builder().prelude("x = 1").prelude("y = x + 1").build().unwrap().get("y").is_some());
    }

    #[test]
    fn precision() {
        let mut s = Session::new(Rc::new(NullHost));
        let v = s.eval("pi").unwrap();
        assert_eq!(s.display(&v), "3.14159");
        s.settings().precision = 10;
        assert_eq!(s.display(&v), "3.141592654");
        assert_eq!(s.eval(r#""{1/3 * 1.0}""#).unwrap().to_string(), "0.3333333333");
        let big = s.eval("2e20 / 3").unwrap();
        assert_eq!(s.display(&big), "6.666666667e19");
        s.settings().precision = 1;
        assert_eq!(s.display(&v), "3");
        assert_eq!(v.to_string(), "3.14159", "outside the session, the defaults");
    }

    #[test]
    fn saves_and_loads() {
        let mut s = Session::new(Rc::new(NullHost));
        s.eval("xs = [1, 2]; f = |x| push(xs, x)").unwrap();
        let mut t = Session::load(Rc::new(NullHost), &s.save()).unwrap();
        assert_eq!(t.eval("f(3); xs").unwrap().to_string(), "[1, 2, 3]");
        assert!(Session::load(Rc::new(NullHost), "not json").is_err());
    }

    #[test]
    fn data_round_trips() {
        let mut s = Session::new(Rc::new(NullHost));
        let src = r#"unit widget; [nil, true, 2^70, 7/2, 1.5, 3 kg*m/s^2, 2 widget/h, 5 ± 0.1 m, 2 - 3i, "hi", b"\x7fELF", r"a+", date("2026-10-06T14:30:00[Europe/Paris]"), {a: set(1, "x")}, table([{n: 1}])]"#;
        let v = s.eval(src).unwrap();
        let d = v.to_data();
        assert_eq!(s.from_data(d.clone()).unwrap().to_data(), d);
        assert_eq!(s.from_data(d).unwrap().to_string(), v.to_string());
        assert!(matches!(s.eval("|x| x").unwrap().to_data(), Data::Fn(_)));
        assert!(s.from_data(Data::Set(vec![Data::List(vec![])])).is_err());
        assert!(s.from_data(Data::Table { cols: vec!["a".into()], rows: vec![vec![]] }).is_err());
        assert!(s.from_data(Data::Qty { value: 1.0, unit: "nope".into() }).is_err());
    }

    #[test]
    fn host_hooks() {
        let mut s = Session::builder()
            .ident(|ctx, n| {
                let key = n.strip_prefix("env_")?;
                ctx.set("seen", Value::str(key));
                Some(Ok(Value::str(key.to_uppercase())))
            })
            .binary(|op, a, b| matches!((op, a, b), ("*", Value::Str(_), Value::Str(_))).then(|| Ok(Value::str(format!("{a}{b}")))))
            .compare(|a, b| match (a, b) {
                (Value::Map(x), Value::Map(y)) => Some(x.borrow().len().cmp(&y.borrow().len())),
                _ => None,
            })
            .convert(|v, t| (t == "shout").then(|| Ok(Value::str(format!("{v}!")))))
            .build()
            .unwrap();
        assert_eq!(s.eval("env_home").unwrap().to_string(), "HOME");
        assert_eq!(s.eval("seen").unwrap().to_string(), "home");
        assert!(s.probe(&crate::parse("env_probe").unwrap()[0].0).is_none(), "completion runs no ident hook");
        assert_eq!(s.eval("seen").unwrap().to_string(), "home");
        assert_eq!(s.eval("env_x = 1; env_x").unwrap().to_string(), "1", "a variable comes first");
        assert_eq!(s.eval(r#""a" * "b""#).unwrap().to_string(), "ab");
        assert_eq!(s.eval("2 * 3").unwrap().to_string(), "6", "a hook's None leaves it to the builtins");
        assert_eq!(s.eval("{a: 1} < {a: 1, b: 2}").unwrap().to_string(), "true");
        assert_eq!(s.eval("[{a: 1, b: 2}, {a: 1}].sort[0].len").unwrap().to_string(), "1");
        assert_eq!(s.eval("5 to shout").unwrap().to_string(), "5!");
        assert_eq!(s.eval(r#"5 to "shout""#).unwrap().to_string(), "5!");
        assert_eq!(s.eval("1 km to m").unwrap().to_string(), "1000 m");
        s.eval("op *(a: str, b: str) = 1").unwrap();
        assert_eq!(s.eval(r#""a" * "b""#).unwrap().to_string(), "1", "a user op comes first");
    }

    #[test]
    fn host_functions_and_constants() {
        let mut s = Session::builder()
            .function("twice(x: num) -> num", "x doubled", |_, a| Ok(Value::Float(2.0 * crate::value::num(&a[0]).unwrap())))
            .function("twice(s: str) -> str", "s repeated", |_, a| Ok(Value::str(format!("{0}{0}", a[0]))))
            .function("boom() -> nil", "always fails", |_, _| Err("no".into()))
            .function("apply(f: fn, x: any) -> any", "f of x", |ctx, a| ctx.call(&a[0], vec![a[1].clone()]))
            .constant("answer", Value::int(42))
            .unit("sprint", "2 wk")
            .doc("twice", "doubles it", &["twice(4)"], &[])
            .build()
            .unwrap();
        assert_eq!(s.eval("twice(21)").unwrap().to_string(), "42");
        assert_eq!(s.eval("twice(\"ab\")").unwrap().to_string(), "abab");
        assert_eq!(s.eval("apply(|x| x + 1, 41)").unwrap().to_string(), "42");
        assert_eq!(s.eval("answer").unwrap().to_string(), "42");
        assert_eq!(s.eval("3 sprint to d").unwrap().to_string(), "42 d");
        assert!(s.eval("twice(nil)").unwrap_err().msg.contains("twice"));
        assert!(s.eval("boom()").unwrap_err().msg.contains("no"));
        assert!(s.eval("help(twice)").is_ok());
        let notes = Rc::new(RefCell::new(vec![]));
        struct Noter(Rc<RefCell<Vec<String>>>);
        impl crate::host::Host for Noter {
            fn note(&self, m: &str) {
                self.0.borrow_mut().push(m.into())
            }
        }
        Session::builder()
            .host(Rc::new(Noter(notes.clone())))
            .function("factorial(n: int) -> int", "", |_, _| Ok(Value::Nil))
            .function("twice(x: int) -> int", "", |_, _| Ok(Value::Nil))
            .build()
            .unwrap();
        Session::builder().host(Rc::new(Noter(notes.clone()))).function("factorial(n: num) -> num", "", |_, _| Ok(Value::Nil)).build().unwrap();
        assert!(notes.borrow().len() == 2 && notes.borrow()[1].contains("overlaps the builtin factorial"), "{:?}", notes.borrow());
        notes.borrow_mut().truncate(1);
        assert!(notes.borrow().len() == 1 && notes.borrow()[0].contains("hides the builtin factorial"), "{:?}", notes.borrow());
        notes.borrow_mut().clear();
        let mut w = Session::builder().host(Rc::new(Noter(notes.clone()))).build().unwrap();
        w.eval("sum = 5").unwrap();
        w.eval("sum = 6").unwrap();
        w.eval("op +(a: list, b: list) = 1").unwrap();
        w.eval(
            "type v = {x: num}
op +(a: v, b: v) = 1",
        )
        .unwrap();
        let seen = notes.borrow();
        assert!(seen.len() == 2 && seen[0].contains("sum now hides the builtin sum") && seen[1].contains("changes `+` on builtin types"), "{seen:?}");
        drop(seen);
        let mut t = Session::builder().module(HostModule::default().function("inc(x: int) -> int", "x plus one", |_, a| Ok(a[0].clone()))).build().unwrap();
        assert_eq!(t.eval("inc(1)").unwrap().to_string(), "1");
        assert!(Session::builder().doc("nope", "", &[], &[]).build().is_err());
        assert!(Session::builder().function("bad", "", |_, _| Ok(Value::Nil)).build().is_err());
    }

    #[test]
    fn help_lists_user_overloads() {
        let mut s = Session::new(Rc::new(NullHost));
        s.eval("area(r: int) = r; area(s: str) = s; len(x: int) = x").unwrap();
        let page = |t| s.enter(|| crate::help::help(Some(t), false)).unwrap();
        let area = page("area");
        assert!(area.contains("defined here") && area.contains("area(r: int)") && area.contains("area(s: str)") && !area.contains("HIDES"), "{area}");
        let len = page("len");
        assert!(len.contains("defined here") && len.contains("HIDES"), "{len}");
    }

    #[test]
    fn methods_dispatch_through_user_overloads() {
        let mut s = Session::new(Rc::new(NullHost));
        s.eval("area(r: int) = r * 2; area(s: str) = len(s); len(x: int) = x").unwrap();
        assert_eq!(s.eval("4.area").unwrap().to_string(), "8");
        assert_eq!(s.eval("\"abc\".area()").unwrap().to_string(), "3");
        assert_eq!(s.eval("5.len").unwrap().to_string(), "5");
        assert_eq!(s.eval("[1, 2].len").unwrap().to_string(), "2");
    }

    #[test]
    fn doc_attaches_to_user_fns() {
        let mut s = Session::new(Rc::new(NullHost));
        s.eval(r#"area(r: int) = r * 2; doc(area, "twice r", examples: ["area(4)"], tags: ["geometry"])"#).unwrap();
        let page = s.enter(|| crate::help::help(Some("area"), false)).unwrap();
        assert!(page.contains("twice r") && page.contains("area(4)") && page.contains("geometry"), "{page}");
        let src = s.enter(|| crate::snapshot::prelude(&s.0));
        assert!(src.contains(r#"doc(area, "twice r", examples: ["area(4)"], tags: ["geometry"])"#), "{src}");
        let mut u = Session::new(Rc::new(NullHost));
        u.eval(&src).unwrap();
        assert!(u.enter(|| crate::help::help(Some("area"), false)).unwrap().contains("geometry"));
        let t = Session::load(Rc::new(NullHost), &s.save()).unwrap();
        let page = t.enter(|| crate::help::help(Some("area"), false)).unwrap();
        assert!(page.contains("twice r") && page.contains("geometry"), "{page}");
        assert!(s.eval("doc(sin, \"x\")").is_err());
        assert!(s.eval("doc(5, \"x\")").is_err());
    }
}
