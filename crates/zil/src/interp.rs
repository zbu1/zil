//! Tree-walking evaluator: scopes, closures, calls, operators and `to` conversions.

use crate::Error;
use crate::ast::{BinOp, Expr, ExprKind, FnDef, Pat, Radix, Target, UnOp};
use crate::error::Limit;
use crate::lexer::Span;
use crate::modules::{self, data::tables, math::uncertainty, units};
use crate::ops::{binary, verb};
use crate::signature;
use crate::state::State;
use crate::value::{Value, exact, num};
use indexmap::IndexMap;
use num_bigint::BigInt;
use num_rational::BigRational;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// ponytail: closures hold their defining Env, so recursive fns form Rc cycles and leak; add a GC/arena if long-running scripts care.
pub struct Closure {
    pub(crate) def: Rc<FnDef>,
    pub(crate) env: Env,
}

impl Closure {
    /// Its signature as written, named `name`: `f(x: int, [a, _], n = 2) -> int`.
    pub fn sig(&self, name: &str) -> String {
        fn pat(p: &Pat) -> String {
            match p {
                Pat::Name(n) => n.clone(),
                Pat::Skip => "_".into(),
                Pat::List(ps, _) => format!("[{}]", ps.iter().map(pat).collect::<Vec<_>>().join(", ")),
                Pat::Rest(p) => format!("*{}", pat(p)),
            }
        }
        let params: Vec<_> = (self.def.params.iter())
            .map(|p| {
                let mut s = pat(&p.pat);
                if p.opt {
                    s += "?";
                }
                if !p.ty.is_empty() {
                    s = format!("{s}: {}", p.ty.join("|"));
                }
                if let Some((_, d)) = &p.default {
                    s = format!("{s} = {d}");
                }
                s
            })
            .collect();
        let ret = if self.def.ret.is_empty() { String::new() } else { format!(" -> {}", self.def.ret.join("|")) };
        format!("{name}({}){ret}", params.join(", "))
    }

    /// Its params as a `Form` for `signature::bad_args`, shown as `text`.
    fn form<'a>(&'a self, text: &'a str) -> signature::Form<'a> {
        let rest = self.def.params.last().is_some_and(|p| matches!(p.pat, Pat::Rest(_)));
        let params = (self.def.params.iter())
            .map(|p| {
                let mut types: Vec<_> = if p.ty.is_empty() { vec!["any"] } else { p.ty.iter().map(String::as_str).collect() };
                if p.opt {
                    types.push("nil");
                }
                (types, p.opt || p.default.is_some() || matches!(p.pat, Pat::Rest(_)))
            })
            .collect();
        signature::Form { text, params, rest }
    }

    /// Whether `args` are as many as it takes, each of its param's types.
    fn fits(&self, args: &[Value]) -> bool {
        let ps = &self.def.params;
        let rest = ps.last().is_some_and(|p| matches!(p.pat, Pat::Rest(_)));
        let min = ps.iter().take_while(|p| !p.opt && p.default.is_none() && !matches!(p.pat, Pat::Rest(_))).count();
        if args.len() < min || (!rest && args.len() > ps.len()) {
            return false;
        }
        args.iter().enumerate().all(|(i, a)| {
            let p = &ps[i.min(ps.len() - 1)];
            p.ty.is_empty() || (p.opt && matches!(a, Value::Nil)) || p.ty.iter().any(|t| signature::is_type(a, t))
        })
    }
}

pub type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
pub struct Scope {
    pub(crate) vars: HashMap<String, Value>,
    pub(crate) parent: Option<Env>,
}

fn child(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope { vars: HashMap::new(), parent: Some(parent.clone()) }))
}

pub fn lookup(env: &Env, name: &str) -> Option<Value> {
    let s = env.borrow();
    match s.vars.get(name) {
        Some(v) => Some(v.clone()),
        None => lookup(s.parent.as_ref()?, name),
    }
}

/// Update an existing variable in the nearest scope that has it.
fn assign(env: &Env, name: &str, v: Value) -> Result<(), Value> {
    let mut s = env.borrow_mut();
    if let Some(slot) = s.vars.get_mut(name) {
        *slot = v;
        return Ok(());
    }
    match &s.parent {
        Some(p) => assign(p, name, v),
        None => Err(v),
    }
}

/// Binds `v` to `pat`: `define` makes new names in `env` (params, loop variables), else names follow `=`.
fn bind(pat: &Pat, v: Value, env: &Env, define: bool) -> Result<(), Error> {
    match pat {
        Pat::Skip => {}
        Pat::Name(name) if define => {
            env.borrow_mut().vars.insert(name.clone(), v);
        }
        Pat::Name(name) => {
            if let Err(v) = assign(env, name, v) {
                env.borrow_mut().vars.insert(name.clone(), v);
            }
        }
        Pat::List(pats, span) => {
            let Value::List(l) = &v else {
                return Err(Error::new(format!("expected a list to unpack, got {}", v.type_name()), span.clone())
                    .note(format!("the value is {}", crate::error::short(&v))));
            };
            let items = l.borrow().clone();
            let rest = pats.iter().any(|p| matches!(p, Pat::Rest(_)));
            let fixed = pats.len() - rest as usize;
            if items.len() < fixed || !rest && items.len() > fixed {
                let least = if rest { "at least " } else { "" };
                return Err(Error::new(format!("expected {least}{fixed} items to unpack, got {}", items.len()), span.clone())
                    .note(format!("the value is {}", crate::error::short(&v))));
            }
            let extra = items.len() - fixed;
            let mut items = items.into_iter();
            for p in pats {
                match p {
                    Pat::Rest(p) => bind(p, Value::list(items.by_ref().take(extra).collect()), env, define)?,
                    p => bind(p, items.next().expect("lengths checked"), env, define)?,
                }
            }
        }
        Pat::Rest(p) => bind(p, v, env, define)?,
    }
    Ok(())
}

/// What `for` loops over and `*` spreads: a list's items, a map's keys, a set's items, a table's rows or a string's characters.
fn items(v: Value) -> Result<Vec<Value>, Value> {
    Ok(match v {
        Value::List(l) => l.borrow().clone(),
        Value::Map(m) => m.borrow().keys().map(|k| Value::str(k.as_str())).collect(),
        Value::Set(s) => s.borrow().iter().cloned().collect(),
        Value::Table(t) => t.maps(),
        Value::Str(s) => s.chars().map(|c| Value::str(c.to_string())).collect(),
        Value::Bytes(b) => b.iter().map(|&c| Value::int(c.into())).collect(),
        v => return Err(v),
    })
}

/// Non-local exits threaded through `?`.
enum Ctl {
    Err(Box<Error>),
    Return(Value),
    Break(Span),
    Continue(Span),
}

impl Ctl {
    /// What escapes a function body or the top level: the returned value, or an error.
    fn finish(self) -> Result<Value, Error> {
        match self {
            Ctl::Return(v) => Ok(v),
            Ctl::Err(e) => Err(*e),
            Ctl::Break(s) => Err(Error::new("`break` outside a loop", s)),
            Ctl::Continue(s) => Err(Error::new("`continue` outside a loop", s)),
        }
    }
}

impl From<Error> for Ctl {
    fn from(e: Error) -> Ctl {
        Ctl::Err(Box::new(e))
    }
}

type EResult = Result<Value, Ctl>;

pub struct Interp {
    globals: Env,
    /// Stdin, read on first use of `input`.
    pub input: Option<Rc<str>>,
    /// Command-line arguments after the script or `-e` code, for `args()`.
    pub args: Vec<String>,
    /// Nested user fn calls, capped at `limits.depth` so runaway recursion errors instead of overflowing.
    depth: usize,
    pub limits: Limits,
    /// Eval steps since the last `run` began.
    steps: u64,
    /// When the running `run` must stop, from `limits.seconds`.
    deadline: Option<jiff::Timestamp>,
    state: Rc<State>,
}

/// Caps on one `run`, so untrusted code ends with an error instead of hanging. Builtins' own `call`s count toward the run.
#[derive(Clone)]
pub struct Limits {
    /// Eval steps per `run`.
    pub steps: u64,
    /// Wall-clock seconds per `run`, read through `Host::now`.
    pub seconds: Option<f64>,
    /// Nested user fn calls; never above the platform's `MAX_DEPTH`.
    pub depth: usize,
    /// Items in one list, map or range, and characters in one string, made by an operator or builtin.
    pub len: usize,
    /// Bits in one exact int or fraction.
    pub bits: u64,
    /// Bytes printed per `run`, by `print`, `page` and `animate`.
    pub output: usize,
    /// Set from anywhere (a Ctrl-C handler, another thread) to stop the run; it clears once it does.
    pub interrupt: Arc<AtomicBool>,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { steps: u64::MAX, seconds: None, depth: MAX_DEPTH, len: 10_000_000, bits: 1 << 22, output: usize::MAX, interrupt: Default::default() }
    }
}

/// How often, in steps, the clock and interrupt flag are checked.
const CHECK_EVERY: u64 = 1 << 12;

/// Fits in `crate::STACK` with room to spare in release; debug frames are ~10x bigger, so heavy bodies can still overflow there.
#[cfg(not(target_arch = "wasm32"))]
const MAX_DEPTH: usize = 10_000;
/// The browser's own stack (about 1 MB in Chrome) holds wasm frames too, and no page can raise it: ~1000 simple calls fit.
/// Heavier bodies can still overflow it, which kills the worker; the page then starts a new one.
#[cfg(target_arch = "wasm32")]
const MAX_DEPTH: usize = 500;

impl Interp {
    /// A session on the running session's host, or outside any, the thread's default one.
    #[allow(clippy::new_without_default)] // builds a whole unit registry: not a cheap default
    pub fn new() -> Interp {
        Interp::with_host(crate::host::get())
    }

    pub fn with_host(host: Rc<dyn crate::host::Host>) -> Interp {
        let globals: Env = Default::default();
        let state = State::new(host);
        crate::state::enter(&state, || {
            let mut g = globals.borrow_mut();
            for m in modules::modules() {
                for f in m.fns {
                    for name in [f.name].iter().chain(f.aliases) {
                        g.vars.insert(name.to_string(), Value::Builtin(m, f.name));
                    }
                }
                for (name, x) in m.consts {
                    g.vars.insert(name.to_string(), Value::Float(*x));
                }
                for (name, x, terms) in m.quantities {
                    g.vars.insert(name.to_string(), Value::Qty(*x, units::terms(terms).expect("checked by modules::tests::registry")));
                }
            }
        });
        Interp { globals, input: None, args: vec![], depth: 0, limits: Limits::default(), steps: 0, deadline: None, state }
    }

    /// Shared handle to this session's units, rates and switches, for code that runs outside `run`, like completion.
    pub fn state(&self) -> Rc<State> {
        self.state.clone()
    }

    /// Shared handle to the global scope, so the REPL completer can peek at variables.
    pub fn globals(&self) -> Env {
        self.globals.clone()
    }

    pub fn set_global(&mut self, name: &str, v: Value) {
        self.globals.borrow_mut().vars.insert(name.into(), v);
    }

    pub fn unset_global(&mut self, name: &str) {
        self.globals.borrow_mut().vars.remove(name);
    }

    pub fn run(&mut self, prog: &[Expr]) -> Result<Value, Error> {
        let (globals, state) = (self.globals.clone(), self.state.clone());
        self.begin();
        self.share_caps();
        crate::state::enter(&state, || {
            let mut last = Value::Nil;
            for stmt in prog {
                last = self.eval(stmt, &globals).or_else(Ctl::finish)?;
            }
            Ok(last)
        })
        .map_err(|e| self.tag(e))
    }

    /// Starts a fresh run's budgets: steps, the deadline and the output count.
    pub fn begin(&mut self) {
        self.steps = 0;
        self.state.printed.set(0);
        self.deadline = self.limits.seconds.and_then(|s| self.state.host.now().checked_add(jiff::SignedDuration::try_from_secs_f64(s).ok()?).ok());
    }

    pub fn call(&mut self, f: &Value, args: Vec<Value>, span: &Span) -> Result<Value, Error> {
        let state = self.state.clone();
        self.share_caps();
        crate::state::enter(&state, || self.call_at(f, args, span, &[], None)).map_err(|e| self.tag(e))
    }

    /// `e`'s value for completion, or `None` if it errors, runs out its 0.2 s, or would change anything: it may write
    /// only names it made itself, call no `mutates` or `shown` fn and draw no random numbers.
    pub fn probe(&self, e: &Expr) -> Option<Value> {
        let input = Some(self.input.clone().unwrap_or_default());
        let limits = Limits { seconds: Some(0.2), ..self.limits.clone() };
        let mut it =
            Interp { globals: self.globals.clone(), input, args: self.args.clone(), depth: 0, limits, steps: 0, deadline: None, state: self.state.clone() };
        let (state, env) = (self.state.clone(), child(&self.globals));
        let seed = state.rng.borrow().get_seed();
        it.begin();
        it.share_caps();
        state.probing.set(true);
        let v = crate::state::enter(&state, || it.eval(e, &env).or_else(Ctl::finish));
        state.probing.set(false);
        if state.rng.borrow().get_seed() != seed {
            state.rng.borrow_mut().seed(seed);
            return None;
        }
        v.ok()
    }

    /// While probing, errors if setting `name` from `env` would reach a global.
    fn probe_write(&self, env: &Env, name: &str, span: &Span) -> Result<(), Ctl> {
        fn owner(env: &Env, name: &str) -> Option<Env> {
            let s = env.borrow();
            if s.vars.contains_key(name) { Some(env.clone()) } else { owner(s.parent.as_ref()?, name) }
        }
        let global = Rc::ptr_eq(&owner(env, name).unwrap_or_else(|| env.clone()), &self.globals);
        if self.state.probing.get() && global {
            return Err(Ctl::from(Error::new(format!("completion can't set `{name}`"), span.clone())));
        }
        Ok(())
    }

    /// Puts the size or output cap a builtin hit on its error.
    fn tag(&self, mut e: Error) -> Error {
        e.limit = e.limit.or(self.state.hit.take());
        e
    }

    fn share_caps(&self) {
        self.state.hit.set(None);
        self.state.max_len.set(self.limits.len);
        self.state.max_bits.set(self.limits.bits);
        self.state.max_output.set(self.limits.output);
    }

    /// `call` with each argument's span and the callee's name as written, for sharper errors.
    fn call_at(&mut self, f: &Value, args: Vec<Value>, span: &Span, arg_spans: &[Span], name: Option<&str>) -> Result<Value, Error> {
        match f {
            Value::Fn(c) => {
                let who = name.unwrap_or("the function");
                if !c.fits(&args) {
                    let sig = c.sig(name.unwrap_or("fn"));
                    return Err(signature::bad_args(who, &[c.form(&sig)], &args, span, &|i| arg_spans.get(i).unwrap_or(span).clone()));
                }
                if self.depth >= self.limits.depth.min(MAX_DEPTH) {
                    return Err(Error { limit: Some(Limit::Depth), ..Error::new("recursion too deep", span.clone()) });
                }
                // Counted from here, since a default can call the fn again.
                self.depth += 1;
                let r = self.bind_params(c, args, who).and_then(|env| self.eval(&c.def.body, &env).or_else(Ctl::finish));
                self.depth -= 1;
                match r {
                    Ok(v) if !c.def.ret.is_empty() && !c.def.ret.iter().any(|t| signature::is_type(&v, t)) => {
                        Err(Error::new(format!("{who}: expected to return {}, got {}", c.def.ret.join(" or "), v.type_name()), span.clone())
                            .help(c.sig(name.unwrap_or("fn"))))
                    }
                    r => r,
                }
            }
            Value::Builtin(m, name) => self.call_builtin(m, name, args, span, arg_spans),
            v => Err(Error::new(format!("cannot call a {}", v.type_name()), span.clone()).note(format!(
                "{} is {}",
                name.unwrap_or("it"),
                crate::error::short(v)
            ))),
        }
    }

    /// A closure's params bound to `args` in a new scope; a missing one gets its default, evaluated after the params before
    /// it, or nil. Out of `call_at` so its locals don't grow every recursion level.
    #[inline(never)]
    fn bind_params(&mut self, c: &Closure, args: Vec<Value>, who: &str) -> Result<Env, Error> {
        let env = child(&c.env);
        let mut args = args.into_iter();
        for p in &c.def.params {
            let (pat, v) = match (&p.pat, &p.default) {
                (Pat::Rest(inner), _) => (&**inner, Value::list(args.by_ref().collect())),
                (pat, d) => match (args.next(), d) {
                    (Some(v), _) => (pat, v),
                    (None, Some((e, _))) => {
                        let v = self.eval(e, &env).or_else(Ctl::finish)?;
                        if let Pat::Name(name) = pat
                            && !p.ty.is_empty()
                            && !p.ty.iter().any(|t| signature::is_type(&v, t))
                        {
                            let want = p.ty.join(" or ");
                            return Err(Error::new(format!("{who}: the default of `{name}` should be {want}, got {}", v.type_name()), e.span.clone())
                                .label(e.span.clone(), format!("this is {}", crate::error::short(&v))));
                        }
                        (pat, v)
                    }
                    (None, None) => (pat, Value::Nil),
                },
            };
            bind(pat, v, &env, true)?;
        }
        Ok(env)
    }

    /// Out of `call_at` so its locals don't grow every recursion level.
    #[inline(never)]
    fn call_builtin(&mut self, m: &'static modules::Module, name: &'static str, args: Vec<Value>, span: &Span, arg_spans: &[Span]) -> Result<Value, Error> {
        let doc = m.fns.iter().find(|f| f.name == name).expect("builtins have docs");
        if self.state.probing.get() && (doc.mutates || !doc.shown.is_empty()) {
            return Err(Error::new(format!("completion can't run `{name}`"), span.clone()));
        }
        let mut call = |args: &[Value]| (m.call)(self, name, args, span);
        let r = if matches!(args.first(), Some(Value::Table(_))) && tables::LIFTED.contains(&name) {
            tables::lift(name, &args, call)
        } else if args.iter().any(|a| matches!(a, Value::Unc(..))) && uncertainty::LIFTED.contains(&name) {
            uncertainty::propagate(&args, call)
        } else {
            call(&args)
        };
        r.map_err(|f| f.error(doc, &args, span, arg_spans))
    }

    /// `obj.key`: a map's key or a table's column, else a zero-arg method (`s.upper` is `upper(s)`), else nil for maps.
    /// The second half is the keys of a map that just lacked `key`, so `{a: 1}.b.c` can blame `.b`.
    fn field(&mut self, e: &Expr, obj: &Expr, key: &str, env: &Env) -> Result<(Value, Option<Vec<String>>), Ctl> {
        let (o, missed) = match &obj.kind {
            ExprKind::Field(inner, k) => self.field(obj, inner, k, env)?,
            _ => (self.eval(obj, env)?, None),
        };
        let missing_key = |keys: Vec<String>| {
            let ExprKind::Field(_, k) = &obj.kind else { unreachable!("only fields miss keys") };
            let at = obj.span.end - k.len()..obj.span.end;
            let hint = crate::error::did_you_mean(k, keys.iter().map(String::as_str));
            Ctl::from(Error::new(format!("map has no key `{k}`{hint}"), at))
        };
        if let Value::Map(m) = &o
            && let Some(v) = m.borrow().get(key)
        {
            return Ok((v.clone(), None));
        }
        if let Value::Table(t) = &o
            && let Some(col) = t.column(key)
        {
            return Ok((col, None));
        }
        match (lookup(env, key), missed) {
            (Some(f @ (Value::Fn(_) | Value::Builtin(..))), missed) => match self.call_at(&f, vec![o], &e.span, std::slice::from_ref(&obj.span), Some(key)) {
                Ok(v) => Ok((v, None)),
                Err(_) if missed.is_some() => Err(missing_key(missed.unwrap())),
                Err(err) => Err(err.into()),
            },
            (_, _) if let Value::Map(m) = &o => Ok((Value::Nil, Some(m.borrow().keys().cloned().collect()))),
            (_, Some(keys)) => Err(missing_key(keys)),
            _ => {
                let at = e.span.end - key.len()..e.span.end;
                let hint = crate::error::did_you_mean(key, methods(env).iter().map(String::as_str));
                Err(Ctl::from(
                    Error::new(format!("{} has no field or method `{key}`{hint}", o.type_name()), at)
                        .label(obj.span.clone(), format!("this is {}", crate::error::short(&o))),
                ))
            }
        }
    }

    /// `x to target` for unit and string targets. Kept out of `eval` (like `call_expr`) so its locals don't grow every recursion level.
    #[inline(never)]
    fn convert_to(&mut self, e: &Expr, x: &Expr, target: &Target, env: &Env) -> EResult {
        let v = self.eval(x, env)?;
        let computed;
        let target = match target {
            Target::Expr(t) => match self.eval(t, env)? {
                Value::Str(s) => {
                    computed = Target::Str(s.to_string());
                    &computed
                }
                other => {
                    let msg = format!("`to` needs a string here, not a {}", other.type_name());
                    return Err(Ctl::from(Error::new(msg, t.span.clone()).label(t.span.clone(), format!("this is {}", crate::error::short(&other)))));
                }
            },
            _ => target,
        };
        match modules::convert(&v, target) {
            Some(Ok(r)) => Ok(r),
            Some(Err(msg)) => {
                // Name both sides as written: `5 kmh to kg` says kmh, not kph.
                let msg = match (&v, target) {
                    (Value::Qty(_, u), Target::Unit(spec)) if units::target_of(spec).is_ok_and(|t| t.dim() != u.dim()) => {
                        let t = units::target_of(spec).unwrap();
                        format!("cannot convert {} to {}", units::describe(u, written(x).as_deref()), units::describe(&t, Some(&units::spec_name(spec))))
                    }
                    _ => msg,
                };
                if msg.starts_with("unknown unit") {
                    // ponytail: assumes the target is written compactly (`km/h`, not `km / h`).
                    let len = target_name(target).len() - 2;
                    return Err(Ctl::from(Error::new(msg, e.span.end.saturating_sub(len)..e.span.end)));
                }
                let e = Error::new(msg, x.span.clone());
                Err(Ctl::from(if written(x).is_some() { e } else { e.label(x.span.clone(), format!("this is {}", crate::error::short(&v))) }))
            }
            None => Err(Ctl::from(Error::new(format!("cannot convert {} to {}", v.type_name(), target_name(target)), e.span.clone()))),
        }
    }

    /// `f(a)` and `x.f(a)`: a map's own fn field wins, else sugar for `f(x, a)`.
    #[inline(never)]
    fn call_expr(&mut self, e: &Expr, callee: &Expr, args: &[Expr], env: &Env) -> EResult {
        let (f, mut argv) = match &callee.kind {
            ExprKind::Field(obj, name) => {
                let o = self.eval(obj, env)?;
                let own = match &o {
                    Value::Map(m) => m.borrow().get(name).cloned(),
                    _ => None,
                };
                match own {
                    Some(f) => (f, vec![]),
                    None => {
                        let Some(f) = lookup(env, name) else {
                            let at = callee.span.end - name.len()..callee.span.end;
                            let hint = crate::error::did_you_mean(name, methods(env).iter().map(String::as_str));
                            return Err(Ctl::from(Error::new(format!("{} has no method `{name}`{hint}", o.type_name()), at)));
                        };
                        (f, vec![o])
                    }
                }
            }
            _ => (self.eval(callee, env)?, vec![]),
        };
        let mut spans: Vec<Span> = match &callee.kind {
            ExprKind::Field(obj, _) if !argv.is_empty() => vec![obj.span.clone()],
            _ => vec![],
        };
        for a in args {
            if let ExprKind::Spread(xs) = &a.kind {
                let vs = self.spread(xs, env)?;
                spans.extend(std::iter::repeat_n(a.span.clone(), vs.len()));
                argv.extend(vs);
                continue;
            }
            argv.push(self.eval(a, env)?);
            spans.push(a.span.clone());
        }
        let name = match &callee.kind {
            ExprKind::Ident(n) | ExprKind::Field(_, n) => Some(n.as_str()),
            _ => None,
        };
        Ok(self.call_at(&f, argv, &e.span, &spans, name)?)
    }

    /// The items `*xs` spreads.
    fn spread(&mut self, xs: &Expr, env: &Env) -> Result<Vec<Value>, Ctl> {
        Ok(items(self.eval(xs, env)?).map_err(|v| {
            Error::new(format!("cannot spread {}", v.type_name()), xs.span.clone()).label(xs.span.clone(), format!("this is {}", crate::error::short(&v)))
        })?)
    }

    /// `obj[idx]`; out of `eval` to keep its frame small.
    #[inline(never)]
    fn index(&mut self, e: &Expr, obj: &Expr, idx: &Expr, env: &Env) -> EResult {
        let err = |msg: String| Ctl::from(Error::new(msg, e.span.clone()));
        Ok(match (self.eval(obj, env)?, self.eval(idx, env)?) {
            (Value::List(l), Value::Int(i, _)) => {
                let l = l.borrow();
                let at = list_index(i, l.len()).ok_or_else(|| out_of_range(i, l.len(), obj, idx))?;
                l[at].clone()
            }
            (Value::Str(s), Value::Int(i, _)) => {
                let n = s.chars().count();
                let at = list_index(i, n).ok_or_else(|| out_of_range(i, n, obj, idx))?;
                Value::str(s.chars().nth(at).unwrap().to_string())
            }
            (Value::Bytes(b), Value::Int(i, _)) => Value::int(b[list_index(i, b.len()).ok_or_else(|| out_of_range(i, b.len(), obj, idx))?].into()),
            (Value::Map(m), Value::Str(k)) => m.borrow().get(&*k).cloned().unwrap_or(Value::Nil),
            (Value::Table(t), Value::Int(i, _)) => t.row(list_index(i, t.rows.len()).ok_or_else(|| out_of_range(i, t.rows.len(), obj, idx))?),
            (Value::Table(t), Value::Str(k)) => t.column(&k).ok_or_else(|| {
                let hint = crate::error::did_you_mean(&k, t.cols.iter().map(String::as_str));
                Ctl::from(Error::new(format!("table has no column `{k}`{hint}"), idx.span.clone()))
            })?,
            (o, i) => {
                return Err(err(format!("cannot index {} with {}", o.type_name(), i.type_name())));
            }
        })
    }

    /// `unit pizza`, `unit slice = pizza / 8`; out of `eval` to keep its frame small.
    #[inline(never)]
    fn unit_def(&mut self, name: &str, value: Option<&Expr>, env: &Env, span: &Span) -> EResult {
        let v = match value {
            Some(x) => Some(self.eval(x, env)?),
            None => None,
        };
        units::define(name, v.as_ref()).map_err(|m| Ctl::from(Error::new(format!("unit: {m}"), span.clone())))?;
        Ok(Value::Nil)
    }

    /// Counts one eval step and stops the run when a limit is hit.
    #[inline]
    fn tick(&mut self, span: &Span) -> Result<(), Error> {
        self.steps += 1;
        if self.steps > self.limits.steps || self.steps.is_multiple_of(CHECK_EVERY) { self.check_limits(span) } else { Ok(()) }
    }

    /// `tick`'s slow path, out of line so `eval` stays small.
    #[inline(never)]
    fn check_limits(&mut self, span: &Span) -> Result<(), Error> {
        let l = &self.limits;
        let (limit, msg) = if self.steps > l.steps {
            (Limit::Steps, format!("step limit reached\nnote: the limit is {} steps per run", l.steps))
        } else if l.interrupt.swap(false, Ordering::Relaxed) {
            (Limit::Interrupt, "interrupted".into())
        } else if self.deadline.is_some_and(|d| self.state.host.now() >= d) {
            (Limit::Time, format!("time limit reached\nnote: the limit is {} s per run", l.seconds.unwrap_or_default()))
        } else {
            return Ok(());
        };
        Err(Error { limit: Some(limit), ..Error::new(msg, span.clone()) })
    }

    fn eval(&mut self, e: &Expr, env: &Env) -> EResult {
        self.tick(&e.span)?;
        let err = |msg: String| Ctl::from(Error::new(msg, e.span.clone()));
        Ok(match &e.kind {
            ExprKind::Nil => Value::Nil,
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Int(n, b) => Value::Int(*n, Radix { base: *b, width: 0 }),
            ExprKind::Big(n, b) => Value::Big(n.clone(), Radix { base: *b, width: 0 }),
            ExprKind::Float(n) => Value::Float(*n),
            ExprKind::Dec(r) => Value::Frac(r.clone(), false),
            ExprKind::Str(s) => Value::Str(s.clone()),
            ExprKind::Bytes(b) => Value::Bytes(b.clone()),
            ExprKind::Regex(r) => Value::Regex(r.clone()),
            ExprKind::Ident(name) => match lookup(env, name) {
                Some(v) => v,
                None => modules::ident(self, name)
                    .unwrap_or_else(|| Err(format!("undefined variable `{name}`{}", crate::error::did_you_mean(name, names(env).iter().map(String::as_str)))))
                    .map_err(err)?,
            },
            ExprKind::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    match &it.kind {
                        ExprKind::Spread(xs) => v.extend(self.spread(xs, env)?),
                        _ => v.push(self.eval(it, env)?),
                    }
                }
                Value::list(v)
            }
            ExprKind::Spread(_) => return Err(err("`*` spreads only inside a call's arguments or a list: `f(*xs)`, `[*xs, 1]`".into())),
            ExprKind::Map(entries) => {
                let mut m = IndexMap::new();
                for (k, v) in entries {
                    m.insert(k.clone(), self.eval(v, env)?);
                }
                Value::map(m)
            }
            ExprKind::Qty(n, spec) => {
                let v = self.eval(n, env)?;
                let n = num(&v).ok_or_else(|| {
                    Ctl::from(
                        Error::new(format!("cannot attach a unit to a {}", v.type_name()), n.span.clone())
                            .label(n.span.clone(), format!("this is {}", crate::error::short(&v))),
                    )
                })?;
                Value::Qty(n, units::unit_of(spec).map_err(err)?)
            }
            // `x to hex(8)` is the builtin call `hex(x, 8)`, even if `hex` is shadowed.
            ExprKind::To(v, Target::Named(name, arg)) => {
                let (m, f) = modules::target(name).expect("parser only accepts registered targets");
                let mut args = vec![self.eval(v, env)?];
                args.extend(arg.map(Value::int));
                self.call(&Value::Builtin(m, f), args, &e.span)?
            }
            ExprKind::To(x, target) => self.convert_to(e, x, target, env)?,
            // One arm for both: each arm of `eval` costs stack in debug builds, on every recursion level.
            ExprKind::Percent(x) | ExprKind::Imag(x) => {
                let (op, k) = if matches!(e.kind, ExprKind::Imag(_)) { (BinOp::Mul, Value::Cplx(0.0, 1.0)) } else { (BinOp::Div, Value::int(100)) };
                binary(op, &self.eval(x, env)?, &k).map_err(err)?
            }
            ExprKind::Format(x, spec) => {
                let v = self.eval(x, env)?;
                let spec = modules::math::formatting::Spec::parse(spec).expect("parser checks specs");
                Value::str(modules::math::formatting::render(&v, &spec).map_err(err)?)
            }
            ExprKind::Unary(op, x) => match (op, self.eval(x, env)?) {
                (UnOp::Not, v) => Value::Bool(!v.truthy()),
                (UnOp::Factorial, v) => self.call(&modules::builtin("factorial"), vec![v], &e.span)?,
                (UnOp::DoubleFactorial, v) => self.call(&modules::builtin("double_factorial"), vec![v], &e.span)?,
                (UnOp::Neg, Value::Int(n, b)) => n.checked_neg().map_or_else(|| Value::Big(Rc::new(-BigInt::from(n)), b), |n| Value::Int(n, b)),
                (UnOp::Neg, Value::Big(n, b)) => exact(BigRational::from_integer(-&*n), b, false),
                (UnOp::Neg, Value::Frac(r, f)) => Value::Frac(Rc::new(-&*r), f),
                (UnOp::Neg, Value::Float(n)) => Value::Float(-n),
                (UnOp::Neg, Value::Qty(n, u)) => Value::Qty(-n, u),
                (UnOp::Neg, Value::Unc(c)) => Value::unc(-c.0, c.1, c.2.clone()),
                (UnOp::Neg, Value::Cplx(re, im)) => Value::Cplx(-re, -im),
                (UnOp::Neg, Value::Mod(x)) => Value::modular(-&x.0, x.1.clone()),
                (UnOp::Neg, v) => return Err(err(format!("cannot negate a {}", v.type_name()))),
                (UnOp::BitNot, Value::Int(n, b)) => Value::Int(!n, b),
                (UnOp::BitNot, Value::Big(n, b)) => exact(BigRational::from_integer(!&*n), b, false),
                (UnOp::BitNot, v) => {
                    return Err(err(format!("cannot bit-invert a {}", v.type_name())));
                }
            },
            ExprKind::Binary(BinOp::And, a, b) => {
                let a = self.eval(a, env)?;
                if a.truthy() { self.eval(b, env)? } else { a }
            }
            ExprKind::Binary(BinOp::Or, a, b) => {
                let a = self.eval(a, env)?;
                if a.truthy() { a } else { self.eval(b, env)? }
            }
            // `x in v` is `contains(v, x)`, even if `contains` is shadowed; `5 km in mi` is `to`.
            ExprKind::Binary(BinOp::In, x, v) => match (self.eval(x, env)?, self.eval(v, env)?) {
                (Value::Qty(x, u), Value::Qty(_, t)) => units::to_unit(x, &u, t).map_err(err)?,
                (xv, vv) => self.call_at(&modules::builtin("contains"), vec![vv, xv], &e.span, &[v.span.clone(), x.span.clone()], None)?,
            },
            ExprKind::Chain(first, rest) => {
                let (mut l, mut le) = (self.eval(first, env)?, &**first);
                for (op, r) in rest {
                    let rv = self.eval(r, env)?;
                    if !binary(*op, &l, &rv).map_err(|m| operands(m, *op, (le, &l), (r, &rv)))?.truthy() {
                        return Ok(Value::Bool(false));
                    }
                    (l, le) = (rv, r);
                }
                Value::Bool(true)
            }
            ExprKind::Binary(op, a, b) => {
                let (mut av, mut bv) = (self.eval(a, env)?, self.eval(b, env)?);
                // A builtin can't be multiplied or divided, so a name that is also a unit means the unit: `km / day`, `cd * 3`.
                if matches!(op, BinOp::Mul | BinOp::Div) {
                    for (e, v) in [(a, &mut av), (b, &mut bv)] {
                        if let (ExprKind::Ident(n), Value::Builtin(..)) = (&e.kind, &*v)
                            && let Ok(u) = crate::modules::units::unit(n)
                        {
                            *v = Value::Qty(1.0, u);
                        }
                    }
                }
                binary(*op, &av, &bv).map_err(|m| operands(m, *op, (a, &av), (b, &bv)))?
            }
            ExprKind::Assign(target, v) => {
                let v = self.eval(v, env)?;
                // ponytail: a probe can't set fields or items even of a list it made; track ownership if that blocks real receivers
                if self.state.probing.get() && !matches!(target.kind, ExprKind::Ident(_)) {
                    return Err(err("completion can't set fields or items".into()));
                }
                match &target.kind {
                    ExprKind::Ident(name) => {
                        self.probe_write(env, name, &e.span)?;
                        if let Err(v) = assign(env, name, v.clone()) {
                            env.borrow_mut().vars.insert(name.clone(), v);
                        }
                    }
                    ExprKind::Field(obj, key) => match self.eval(obj, env)? {
                        Value::Map(m) => {
                            m.borrow_mut().insert(key.clone(), v.clone());
                        }
                        o => return Err(err(format!("cannot set a field on a {}", o.type_name()))),
                    },
                    ExprKind::Index(obj, idx) => {
                        let (ov, iv) = (self.eval(obj, env)?, self.eval(idx, env)?);
                        match (ov, iv) {
                            (Value::List(l), Value::Int(i, _)) => {
                                let mut l = l.borrow_mut();
                                let i = list_index(i, l.len()).ok_or_else(|| out_of_range(i, l.len(), obj, idx))?;
                                l[i] = v.clone();
                            }
                            (Value::Map(m), Value::Str(k)) => {
                                m.borrow_mut().insert(k.to_string(), v.clone());
                            }
                            (o, i) => {
                                return Err(err(format!("cannot index {} with {}", o.type_name(), i.type_name())));
                            }
                        }
                    }
                    _ => unreachable!("parser validates assignment targets"),
                }
                v
            }
            ExprKind::Unpack(pat, v) => {
                let v = self.eval(v, env)?;
                fn names<'a>(p: &'a Pat, out: &mut Vec<&'a str>) {
                    match p {
                        Pat::Name(n) => out.push(n),
                        Pat::Skip => {}
                        Pat::List(ps, _) => ps.iter().for_each(|p| names(p, out)),
                        Pat::Rest(p) => names(p, out),
                    }
                }
                let mut out = vec![];
                names(pat, &mut out);
                for name in out {
                    self.probe_write(env, name, &e.span)?;
                }
                bind(pat, v.clone(), env, false)?;
                v
            }
            ExprKind::Call(callee, args) => self.call_expr(e, callee, args, env)?,
            // A variable beats a command: `cat foo` with `cat = 5` is `5 foo`.
            ExprKind::Command(name, call, alt) => {
                let own = match lookup(env, name) {
                    Some(Value::Builtin(m, f)) => m.fns.iter().any(|d| d.name == f && d.is(name)),
                    v => v.is_none(),
                };
                match if own { call } else { alt } {
                    Ok(x) => self.eval(x, env)?,
                    Err(e) => return Err(Ctl::from((**e).clone())),
                }
            }
            ExprKind::Field(obj, key) => self.field(e, obj, key, env)?.0,
            ExprKind::Index(obj, idx) => self.index(e, obj, idx, env)?,
            ExprKind::Slice(obj, from, to, incl) => {
                let o = self.eval(obj, env)?;
                let mut bound = |b: &Option<Box<Expr>>| -> Result<Option<i64>, Ctl> {
                    match b {
                        None => Ok(None),
                        Some(b) => match self.eval(b, env)? {
                            Value::Int(n, _) => Ok(Some(n)),
                            v => Err(Ctl::from(Error::new(format!("slice bounds must be int, got {}", v.type_name()), b.span.clone()))),
                        },
                    }
                };
                let (from, to, incl) = (bound(from)?, bound(to)?, *incl);
                match o {
                    Value::List(l) => {
                        let l = l.borrow();
                        let (a, b) = slice_range(from, to, incl, l.len());
                        Value::list(l[a..b].to_vec())
                    }
                    Value::Str(s) => {
                        let (a, b) = slice_range(from, to, incl, s.chars().count());
                        Value::str(s.chars().skip(a).take(b - a).collect::<String>())
                    }
                    Value::Bytes(s) => {
                        let (a, b) = slice_range(from, to, incl, s.len());
                        Value::bytes(&s[a..b])
                    }
                    Value::Table(t) => {
                        let (a, b) = slice_range(from, to, incl, t.rows.len());
                        Value::table(crate::value::Table { cols: t.cols.clone(), rows: t.rows[a..b].to_vec() })
                    }
                    o => return Err(err(format!("cannot slice {}", o.type_name()))),
                }
            }
            ExprKind::Fn(def) => Value::Fn(Rc::new(Closure { def: def.clone(), env: env.clone() })),
            ExprKind::If(cond, then, els) => {
                if self.eval(cond, env)?.truthy() {
                    self.eval(then, env)?
                } else if let Some(els) = els {
                    self.eval(els, env)?
                } else {
                    Value::Nil
                }
            }
            ExprKind::While(cond, body) => {
                while self.eval(cond, env)?.truthy() {
                    match self.eval(body, env) {
                        Ok(_) | Err(Ctl::Continue(_)) => {}
                        Err(Ctl::Break(_)) => break,
                        Err(c) => return Err(c),
                    }
                }
                Value::Nil
            }
            ExprKind::For(pat, iter, body) => {
                let items = items(self.eval(iter, env)?).map_err(|v| {
                    Error::new(format!("cannot loop over a {}", v.type_name()), iter.span.clone())
                        .label(iter.span.clone(), format!("this is {}", crate::error::short(&v)))
                })?;
                for it in items {
                    let scope = child(env);
                    bind(pat, it, &scope, true)?;
                    match self.eval(body, &scope) {
                        Ok(_) | Err(Ctl::Continue(_)) => {}
                        Err(Ctl::Break(_)) => break,
                        Err(c) => return Err(c),
                    }
                }
                Value::Nil
            }
            ExprKind::Block(stmts) => {
                let scope = child(env);
                let mut last = Value::Nil;
                for s in stmts {
                    last = self.eval(s, &scope)?;
                }
                last
            }
            ExprKind::Return(v) => {
                let v = match v {
                    Some(v) => self.eval(v, env)?,
                    None => Value::Nil,
                };
                return Err(Ctl::Return(v));
            }
            ExprKind::UnitDef(..) if self.state.probing.get() => return Err(err("completion can't define units".into())),
            ExprKind::UnitDef(name, value) => self.unit_def(name, value.as_deref(), env, &e.span)?,
            ExprKind::Break => return Err(Ctl::Break(e.span.clone())),
            ExprKind::Continue => return Err(Ctl::Continue(e.span.clone())),
        })
    }
}

/// Negative indices count from the end.
fn list_index(i: i64, len: usize) -> Option<usize> {
    let i = if i < 0 { len as i64 + i } else { i };
    (0..len as i64).contains(&i).then_some(i as usize)
}

/// Clamped `[from..to)`, negatives counting from the end.
fn slice_range(from: Option<i64>, to: Option<i64>, incl: bool, len: usize) -> (usize, usize) {
    let fix = |i: i64, d: i64| ((if i < 0 { len as i64 + i } else { i }) + d).clamp(0, len as i64) as usize;
    let a = from.map_or(0, |i| fix(i, 0));
    let b = to.map_or(len, |i| fix(i, incl as i64));
    (a, b.max(a))
}

/// Every name in scope, innermost first.
pub fn names(env: &Env) -> Vec<String> {
    let s = env.borrow();
    let mut out: Vec<String> = s.vars.keys().cloned().collect();
    out.extend(s.parent.as_ref().map(names).unwrap_or_default());
    out
}

/// Names in scope that hold functions, for `x.method` suggestions.
pub fn methods(env: &Env) -> Vec<String> {
    names(env).into_iter().filter(|n| matches!(lookup(env, n), Some(Value::Fn(_) | Value::Builtin(..)))).collect()
}

/// A unit as the source wrote it, when `e` is a literal like `5 kmh`.
fn written(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Qty(_, spec) => Some(units::spec_name(spec)),
        _ => None,
    }
}

fn target_name(t: &Target) -> String {
    match t {
        Target::Unit(spec) => format!("`{}`", units::spec_name(spec)),
        Target::Units(specs) => format!("`{}`", specs.iter().map(units::spec_name).collect::<Vec<_>>().join(" ")),
        Target::Named(n, _) => format!("`{n}`"),
        Target::Str(s) => format!("{s:?}"),
        Target::Expr(_) => "a string".into(),
    }
}

/// `index 5 out of range` on the index, with the length in a note.
fn out_of_range(i: i64, len: usize, obj: &Expr, idx: &Expr) -> Ctl {
    let what = match &obj.kind {
        ExprKind::Ident(n) => format!("`{n}`"),
        _ => "it".into(),
    };
    let items = if matches!(obj.kind, ExprKind::Str(_)) { "character" } else { "item" };
    Ctl::from(Error::new(format!("index {i} out of range"), idx.span.clone()).note(format!("{what} has {len} {items}{}", if len == 1 { "" } else { "s" })))
}

/// A binary operator's error, labeling each operand with its value. Mismatched quantities are named
/// by kind and unit as written: `cannot add length (km) and mass (kg)`.
fn operands(msg: String, op: BinOp, (a, av): (&Expr, &Value), (b, bv): (&Expr, &Value)) -> Ctl {
    let msg = match (av, bv) {
        (Value::Qty(_, u), Value::Qty(_, w)) if u.dim() != w.dim() => {
            format!("cannot {} {} and {}", verb(op), units::describe(u, written(a).as_deref()), units::describe(w, written(b).as_deref()))
        }
        _ => msg,
    };
    Ctl::from(Error::new(msg, a.span.clone()).label(a.span.clone(), crate::error::short(av)).label(b.span.clone(), crate::error::short(bv)))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn eval(src: &str) -> Value {
        try_eval(src).unwrap_or_else(|e| panic!("{src}: {}", e.msg))
    }

    pub fn try_eval(src: &str) -> Result<Value, Error> {
        let ast = crate::parser::parse(src)?;
        Interp::new().run(&ast)
    }

    pub fn show(src: &str) -> String {
        eval(src).to_string()
    }

    #[test]
    fn arithmetic() {
        assert_eq!(show("1 + 2 * 3"), "7");
        assert_eq!(show("1 / 3"), "0.333333");
        assert_eq!(show("6 / 3"), "2");
        assert_eq!(show("-7 // 2"), "-4");
        assert_eq!(show("-7 % 3"), "2");
        assert_eq!(show("2 ** 10"), "1024");
        assert_eq!(show("-2 ** 2"), "-4");
        assert_eq!(show("2 ** 3 ** 2"), "512");
        assert_eq!(show("0.1 + 0.2"), "0.3");
        assert_eq!(show("1e20"), "1e20");
        assert_eq!(show(r#""ab" * 3"#), "ababab");
        assert!(try_eval("1 / 0").is_err());
    }

    #[test]
    fn percent() {
        assert_eq!(show("[20%, 50 + 10%, 50 - 10%, 20% of 50, 50 * 10%, 7 % 3, 7%3, 20 % -3]"), "[0.2, 55, 45, 10, 5, 1, 1, 2]");
        assert_eq!(show("[80 km + 25%, 15% of 2 h to min, 1.5%]"), "[100 km, 18 min, 0.015]");
        assert_eq!(
            show(
                "x = 200
x -= 5%
x += 50%
x"
            ),
            "285"
        );
        assert_eq!(show("(100 + 10%) + 10%"), "121");
    }

    #[test]
    fn bitwise() {
        assert_eq!(show("0b1100 & 0b1010 to bin"), "0b1000");
        assert_eq!(show("0b1100 | 0b1010 to bin"), "0b1110");
        assert_eq!(show("0b1100 ^ 0b1010 to bin"), "0b110");
        assert_eq!(show("1 << 4"), "16");
        assert_eq!(show("-16 >> 2"), "-4");
        assert_eq!(show("~0"), "-1");
        assert_eq!(
            show(
                "x = 0xff00 to hex
x & 0x0ff0"
            ),
            "0xf00"
        );
        assert_eq!(show("1 | 2 == 3"), "true");
        assert_eq!(show("0xf0 >> 4 + 0"), "0xf");
        assert!(try_eval("1 << 64").is_err());
        assert!(try_eval("1.5 & 1").is_err());
    }

    #[test]
    fn strings() {
        assert_eq!(
            show(
                r#"x = 2
"x={x}, sq={x ** 2}""#
            ),
            "x=2, sq=4"
        );
        assert_eq!(show(r#""hello"[1..4]"#), "ell");
        assert_eq!(show(r#""hello"[-3..]"#), "llo");
        assert_eq!(show(r#""hello".upper"#), "HELLO");
        assert_eq!(show(r#"" hi ".trim.reverse"#), "ih");
        assert_eq!(show(r#""a,b".split(",").len"#), "2");
    }

    #[test]
    fn variables_and_closures() {
        assert_eq!(show("x = 1\nx = x + 1\nx"), "2");
        let src = "
make = fn(n) { |x| x + n }
add2 = make(2)
add2(40)";
        assert_eq!(show(src), "42");
        assert_eq!(show("f = || 7\nf() + (|a, b| a * b)(2, 3)"), "13");
        let src = "
total = 0
for x in 1..5 { total = total + x }
total";
        assert_eq!(show(src), "10");
    }

    #[test]
    fn typed_params() {
        assert_eq!(show("f = fn(a, b = a * 2, c?: str) { [a, b, c] }; [f(1), f(1, 5, \"x\")]"), "[[1, 2, nil], [1, 5, \"x\"]]");
        assert_eq!(show("f = fn(xs = []) { xs.push(1); xs }; [f(), f()]"), "[[1], [1]]");
        assert_eq!(show("f = fn(x, *xs: num) { [x, xs] }; [f(1), f(1, 2, 3)]"), "[[1, []], [1, [2, 3]]]");
        assert_eq!(show("g = |a, b = a | 1| b; g(2)"), "3");
        assert_eq!(show("help(fn(n: int, [a, _] = [1, 2], *xs: num) -> num { n })"), "fn(n: int, [a, _] = [1, 2], *xs: num) -> num");
        let err = |src: &str| try_eval(src).unwrap_err().msg;
        assert_eq!(err("f = fn(n: int) { n }; f(\"x\")"), "f: expected int, got str");
        assert_eq!(err("f = fn(n: int) -> str { n }; f(3)"), "f: expected to return str, got int");
        assert_eq!(err("f = fn(x, *xs: num) { x }; f(1, 2, nil)"), "f: expected num, got nil");
        assert_eq!(err("f = fn(a, b = 1) { a }; f(1, 2, 3)"), "f takes 1-2 arguments, got 3");
        assert_eq!(err("f = fn(n: int = \"a\") { n }; f()"), "f: the default of `n` should be int, got str");
        assert_eq!(err("fn(x?: int = 3) { x }"), "`x?: int = 3` defaults to nil already, so drop the `?` to give it another default");
        assert_eq!(err("fn(x: int = nil) { x }"), "`x: int = nil` doesn't allow nil, so write `x?: int`");
        assert_eq!(err("fn(x = 1, y) { x }"), "`y` needs a default too, since a param before it has one");
        assert_eq!(err("fn(x: flaot) { x }"), "unknown type `flaot`");
        assert_eq!(err("|x: int| x"), "types go in `fn(...)`, since `|x: int|str|` would be ambiguous");
    }

    #[test]
    fn recursion_and_return() {
        let src = "
fib = fn(n) {
  if n < 2 { return n }
  fib(n - 1) + fib(n - 2)
}
fib(10)";
        assert_eq!(show(src), "55");
    }

    #[test]
    fn recursion_depth() {
        let run = || {
            let g = "g = |n| if n == 0 {0} else {g(n - 1)}
";
            assert_eq!(show(&format!("{g}g(5000)")), "0");
            assert_eq!(try_eval(&format!("{g}g(10_000_000)")).unwrap_err().msg, "recursion too deep");
            // A default that calls its own fn counts as recursion too.
            assert_eq!(try_eval("f = fn(n = f()) { n }; f()").unwrap_err().msg, "recursion too deep");
        };
        std::thread::Builder::new().stack_size(crate::STACK).spawn(run).unwrap().join().unwrap();
    }

    #[test]
    fn limits() {
        let run = |it: &mut Interp, src: &str| it.run(&crate::parser::parse(src).unwrap()).unwrap_err().msg;
        let kind = |it: &mut Interp, src: &str| it.run(&crate::parser::parse(src).unwrap()).unwrap_err().limit;
        let mut it = Interp::new();
        assert_eq!(kind(&mut it, "1 + nil"), None);
        it.limits.steps = 10_000;
        assert_eq!(run(&mut it, "while true {}"), "step limit reached");
        assert_eq!(kind(&mut it, "while true {}"), Some(Limit::Steps));
        // Callbacks from builtins count too.
        assert_eq!(run(&mut it, "(1..1_000_000).map(|x| x)"), "step limit reached");
        it.limits.steps = u64::MAX;
        it.limits.interrupt.store(true, Ordering::Relaxed);
        assert_eq!(run(&mut it, "while true {}"), "interrupted");
        assert!(!it.limits.interrupt.load(Ordering::Relaxed));
        it.limits.depth = 10;
        assert_eq!(run(&mut it, "g = |n| g(n + 1)\ng(0)"), "recursion too deep");
        assert_eq!(kind(&mut it, "g(0)"), Some(Limit::Depth));
        it.limits.len = 100;
        it.limits.bits = 1000;
        for src in [
            "\"ab\" * 51",
            "s = \"ab\"; while true { s = s + s }",
            "x = [1]; while true { x = x + x }",
            "0..101",
            "\"\".pad(101)",
            "x = 3; while true { x = x * x }",
            "2 ** 1001",
            "factorial(200)",
            "x = (0..11).map(|_| 0..10); x.flatten",
            "table((0..11).map(|i| [[\"k{i}\", i]].from_entries))",
            "set(0..60) | set(50..100)",
            "{a: 1} + (0..100).map(|i| [\"k{i}\", i]).from_entries",
            "banner(\"abc\")",
            "banner(\"abc\", \"lean\")",
        ] {
            assert!(run(&mut it, src).ends_with("result too large"), "{src}");
        }
        assert_eq!(kind(&mut it, "2 ** 1001"), Some(Limit::Bits));
        assert_eq!(kind(&mut it, "0..101"), Some(Limit::Len));
        assert_eq!(kind(&mut it, "1 + nil"), None, "a hit doesn't carry over to the next run");
        it.limits.output = 10;
        assert_eq!(run(&mut it, "print(\"12345\"); print(\"12345\")"), "print: output limit reached");
        assert_eq!(kind(&mut it, "print(\"12345678901\")"), Some(Limit::Output));
        // The count starts over each run.
        assert!(it.run(&crate::parser::parse("print(\"123456789\")").unwrap()).is_ok());
    }

    #[test]
    fn semicolons() {
        assert_eq!(show("x = 1; y = 2; x + y;"), "3");
        assert_eq!(show("f = fn(n) { a = n; a * 2 }; f(4)"), "8");
    }

    #[test]
    fn loop_control() {
        assert_eq!(show("t = 0\nfor x in 1..10 { if x == 5 { break }\n if x % 2 == 0 { continue }\n t += x }\nt"), "4");
        assert_eq!(show("n = 0\nwhile true { n += 1\n if n >= 3 { break } }\nn"), "3");
        assert_eq!(show("f = fn() { for x in [1, 2] { return x } }\nf()"), "1");
        assert!(try_eval("break").is_err());
        assert!(try_eval("for x in [1] { [1].map(|y| continue) }").is_err());
    }

    #[test]
    fn compound_assignment() {
        assert_eq!(show("x = 5\nx += 2\nx *= 3\nx -= 1\nx //= 4\nx **= 2\nx"), "25");
        assert_eq!(show("xs = [1, 2]\nxs[0] += 10\nu = {n: 1}\nu.n <<= 3\n[xs, u.n]"), "[[11, 2], 8]");
        assert_eq!(show("s = \"a\"\ns += \"b\"\ns"), "ab");
        assert!(try_eval("1 += 2").is_err());
    }

    #[test]
    fn pipe_placeholder() {
        assert_eq!(show("3.14159 |> round(_, 2)"), "3.14");
        assert_eq!(show("5 |> _ * 2 |> [_, _]"), "[10, 10]");
        assert_eq!(show("[1, 2] |> map(_, |x| x |> _ + 1)"), "[2, 3]");
        assert_eq!(show("\"x\" |> upper"), "X");
    }

    #[test]
    fn membership_and_chains() {
        assert_eq!(show("[2 in [1, 2], \"b\" in \"abc\", \"k\" in {k: 1}, 9 in 1..5]"), "[true, true, true, false]");
        assert_eq!(show("x = 3\n[x in [3], 2 in 1..=2]"), "[true, true]");
        assert_eq!(show("5 in to cm"), "12.7 cm");
        assert_eq!(show("[5 km in mi, (x = 3 ft) in cm]"), "[3.10686 mi, 91.44 cm]");
        assert!(try_eval("5 km in kg").is_err());
        assert_eq!(show("[0 < 5 < 10, 0 < 15 < 10, 1 <= 1 < 2 <= 2]"), "[true, false, true]");
        assert_eq!(show("n = 0\nf = fn() { n += 1\n 5 }\n0 < f() < 10\nn"), "1");
    }

    #[test]
    fn destructuring() {
        assert_eq!(show("[a, [b, _], c] = [1, [2, 3], 4]\n[a, b, c]"), "[1, 2, 4]");
        assert_eq!(show("a = 1\nb = 2\n[a, b] = [b, a]\n[a, b]"), "[2, 1]");
        assert_eq!(show("t = 0\nfor [k, v] in {x: 1, y: 2}.list { t += v }\nt"), "3");
        assert_eq!(show("s = \"\"\nfor [i, x] in [\"a\", \"b\"].enumerate { s += \"{i}{x}\" }\ns"), "0a1b");
        assert_eq!(show("[[1, 2], [3, 4]].map(|[a, b]| a * b)"), "[2, 12]");
        assert_eq!(show("f = fn([x, y], z) { x + y + z }\nf([1, 2], 3)"), "6");
        assert_eq!(try_eval("[a, b] = [1]").unwrap_err().msg, "expected 2 items to unpack, got 1");
        assert_eq!(try_eval("[a] = 5").unwrap_err().msg, "expected a list to unpack, got int");
        assert!(try_eval("[a, 1] = [1, 2]").is_err());
    }

    #[test]
    fn commands_yield_to_variables() {
        assert_eq!(show("cat = 5\ncat m"), "5 m");
        assert_eq!(show("ls = 2\nls - 1"), "1");
        assert_eq!(try_eval("ls - l").unwrap_err().msg, "flags touch their dash");
        assert_eq!(show("5 cd"), "5 cd");
        assert_eq!(show("f = ls\nf == ls"), "true");
    }

    #[test]
    fn spread_and_bare_unpacking() {
        assert_eq!(show("data = [[1, 2, 3], [4, 5, 6]]\nc1, c2, c3 = zip(*data)\n[c1, c2, c3]"), "[[1, 4], [2, 5], [3, 6]]");
        assert_eq!(show("a, b = 3, 4\na, b = b, a\n[a, b]"), "[4, 3]");
        assert_eq!(show("first, *rest = [1, 2, 3]\n[first, rest]"), "[1, [2, 3]]");
        assert_eq!(show("[x, *mid, z] = [1, 2]\n[x, mid, z]"), "[1, [], 2]");
        assert_eq!(show("*init, _ = 1, 2, 3\ninit"), "[1, 2]");
        assert_eq!(show("s = \"\"\nfor i, x in [\"a\", \"b\"].enumerate { s += \"{i}{x}\" }\ns"), "0a1b");
        assert_eq!(show("[*[1, 2], 3, *\"ab\"]"), r#"[1, 2, 3, "a", "b"]"#);
        assert_eq!(show("max(*[3, 9], 4)"), "9");
        assert_eq!(try_eval("[a, *b] = []").unwrap_err().msg, "expected at least 1 items to unpack, got 0");
        assert!(try_eval("[*a, *b] = [1]").is_err());
        assert!(try_eval("x = 1, 2").is_err());
        assert!(try_eval("f(*5)").is_err());
    }

    #[test]
    fn ranges() {
        assert_eq!(show("1..=4"), "[1, 2, 3, 4]");
        assert_eq!(show("a = [1, 2, 3, 4, 5, 6]; [a[1..=3], a[..=-2], a[-2..=-1], \"hello\"[..=1]]"), "[[2, 3, 4], [1, 2, 3, 4, 5], [5, 6], \"he\"]");
        assert_eq!(show("(0..=20).step(5)"), "[0, 5, 10, 15, 20]");
        assert!(try_eval("(1..3).step(0)").is_err());
    }

    #[test]
    fn big_ints() {
        assert_eq!(show("2 ** 100"), "1267650600228229401496703205376");
        assert_eq!(show("9223372036854775807 + 1"), "9223372036854775808");
        assert_eq!(show("(2 ** 64) // 2 ** 60"), "16");
        assert_eq!(type_of("2 ** 64 - 2 ** 64 + 1"), "Int");
        assert_eq!(show("99999999999999999999 % 7"), "1");
        assert_eq!(show("0xffffffffffffffffff + 1"), "0x1000000000000000000");
        assert_eq!(show("(2 ** 100).digits.sum"), "115");
        assert_eq!(show("-(-9223372036854775807 - 1)"), "9223372036854775808");
        assert_eq!(show("\"123456789012345678901234567890\".int"), "123456789012345678901234567890");
        assert_eq!(show("2 ** 64 > 2 ** 63"), "true");
        assert_eq!(show("(-1) ** 99999999999"), "-1");
        assert!(try_eval("2 ** 99999999999").is_err());
    }

    #[test]
    fn fractions() {
        assert_eq!(show("[0.1 + 0.2 == 0.3, 0.1 + 0.2 - 0.3, type(0.1), type(1.0), type(1.5e3)]"), r#"[true, 0, "frac", "float", "float"]"#);
        assert_eq!(show("round(2.675, 2)"), "2.68");
        assert_eq!(show("7 / 2"), "3.5");
        assert_eq!(show("1 / 3 * 3"), "1");
        assert_eq!(show("type(1 / 3)"), "frac");
        assert_eq!(show("1/3 + 1/6 to frac"), "1/2");
        assert_eq!(show("x = 2/3 to frac\nx + 1"), "5/3");
        assert_eq!(show("0.1 + 0.2 to frac"), "3/10");
        assert_eq!(show("2 ** -2"), "0.25");
        assert_eq!(show("1/3 + 0.5"), "0.833333");
        assert_eq!(show("[1/2 == 0.5, 1/3 < 1/2, round(7/2), floor(-7/2), int(-7/2)]"), "[true, true, 4, -4, -3]");
        assert_eq!(show("str(1/3)"), "0.3333333333333333");
        assert_eq!(show("(7/2 to frac) % 1"), "1/2");
        assert_eq!(
            show(
                "h = 1/2
h km to m"
            ),
            "500 m"
        );
    }

    fn type_of(src: &str) -> &'static str {
        match eval(src) {
            Value::Int(..) => "Int",
            Value::Big(..) => "Big",
            _ => "other",
        }
    }

    #[test]
    fn records_and_mutation() {
        let src = r#"
u = {name: "ann", tags: []}
u.age = 30
u.tags.push("x")
u.tags[0] = "y"
[u.age, u.tags, u.missing]"#;
        assert_eq!(show(src), r#"[30, ["y"], nil]"#);
    }
}
