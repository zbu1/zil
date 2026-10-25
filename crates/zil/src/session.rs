//! The embedding API: one sandboxed session of zil code.

use crate::ast::{Expr, ExprKind};
use crate::host::Host;
use crate::interp::{Interp, Limits, lookup, names};
use crate::{Data, Error, Settings, Value};
use std::cell::RefMut;
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
    /// A fresh session on `host`. `host::NullHost` denies every capability, for untrusted code.
    pub fn new(host: Rc<dyn Host>) -> Session {
        Session(Interp::with_host(host))
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
        if !matches!(f, Value::Fn(_) | Value::Builtin(..)) {
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

    /// Caps on each `eval`; hitting one is an `Error` whose `limit` says which.
    pub fn limits(&mut self) -> &mut Limits {
        &mut self.0.limits
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
}
