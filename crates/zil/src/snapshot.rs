//! A session as JSON and back: its variables (functions with what they captured), user units, random numbers and the
//! network switch, with nothing run again. Lists, maps, sets, scopes and functions are numbered objects, so a value
//! two variables share comes back shared, and a list that holds itself comes back holding itself.

use crate::ast::{ExprKind, Radix};
use crate::interp::{Closure, Env, Interp};
use crate::modules::{self, units};
use crate::value::{Table, Value};
use indexmap::{IndexMap, IndexSet};
use num_bigint::BigInt;
use num_rational::BigRational;
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Bumped when the format changes; a file of any other version doesn't load.
const VERSION: u64 = 1;

/// Numbers each shared object the first time it's seen, keeping its contents in `objects`.
pub struct Saver {
    ids: HashMap<*const (), usize>,
    objects: Vec<Json>,
    /// Functions without what they captured, for the transcript, which only shows them.
    shallow: bool,
}

impl Saver {
    pub fn new(shallow: bool) -> Saver {
        Saver { ids: HashMap::new(), objects: vec![], shallow }
    }

    pub fn objects(self) -> Json {
        Json::Array(self.objects)
    }

    /// A reference to the object at `p`, saving it with `make` the first time.
    fn object(&mut self, p: *const (), make: impl FnOnce(&mut Saver) -> Json) -> Json {
        if let Some(id) = self.ids.get(&p) {
            return json!({ "ref": id });
        }
        let id = self.objects.len();
        self.ids.insert(p, id);
        self.objects.push(Json::Null);
        self.objects[id] = make(self);
        json!({ "ref": id })
    }

    pub fn value(&mut self, v: &Value) -> Json {
        match v {
            Value::Nil => Json::Null,
            Value::Bool(b) => json!(b),
            Value::Int(n, r) if *r == Radix::DEC => json!(n),
            Value::Int(n, r) => json!({ "int": n, "base": r.base, "width": r.width }),
            Value::Big(n, r) => json!({ "big": n.to_string(), "base": r.base, "width": r.width }),
            Value::Frac(r, shown) => json!({ "frac": r.to_string(), "shown": shown }),
            Value::Float(x) => json!({ "float": float(*x) }),
            Value::Qty(x, u) => json!({ "qty": float(*x), "unit": u.to_string() }),
            Value::Unc(c) => json!({ "unc": [float(c.0), float(c.1)], "unit": c.2.to_string() }),
            Value::Cplx(re, im) => json!({ "cplx": [float(*re), float(*im)] }),
            Value::Mod(m) => json!({ "mod": [m.0.to_string(), m.1.to_string()] }),
            Value::Str(s) => json!(&**s),
            Value::Bytes(b) => json!({ "bytes": crate::modules::text::encoding::hex(b) }),
            Value::Regex(r) => json!({ "regex": r.as_str() }),
            Value::Date(z) => json!({ "date": z.to_string() }),
            Value::Table(t) => {
                let rows: Vec<Json> = t.rows.iter().map(|r| Json::Array(r.iter().map(|v| self.value(v)).collect())).collect();
                json!({ "table": t.cols, "rows": rows })
            }
            Value::Builtin(_, name) => json!({ "builtin": name }),
            Value::List(l) => self.object(Rc::as_ptr(l).cast(), |s| json!({ "list": l.borrow().iter().map(|v| s.value(v)).collect::<Vec<_>>() })),
            Value::Map(m) => {
                self.object(Rc::as_ptr(m).cast(), |s| json!({ "map": m.borrow().iter().map(|(k, v)| json!([k, s.value(v)])).collect::<Vec<_>>() }))
            }
            Value::Set(set) => self.object(Rc::as_ptr(set).cast(), |s| json!({ "set": set.borrow().iter().map(|v| s.value(v)).collect::<Vec<_>>() })),
            Value::Fn(f) => self.object(Rc::as_ptr(f).cast(), |s| {
                let env = if s.shallow { Json::Null } else { s.scope(&f.env) };
                json!({ "fn": &*f.def.src, "env": env })
            }),
        }
    }

    /// A scope and the ones around it. A top-level one leaves out the builtins every session starts with.
    fn scope(&mut self, env: &Env) -> Json {
        self.object(Rc::as_ptr(env).cast(), |s| {
            let scope = env.borrow();
            let parent = scope.parent.as_ref().map_or(Json::Null, |p| s.scope(p));
            let mut vars: Vec<(&String, &Value)> = scope.vars.iter().filter(|(k, v)| scope.parent.is_some() || !builtin(k, v)).collect();
            // Sorted, so saving the same session twice writes the same file.
            vars.sort_by_key(|(k, _)| *k);
            json!({ "scope": vars.into_iter().map(|(k, v)| json!([k, s.value(v)])).collect::<Vec<_>>(), "parent": parent })
        })
    }
}

/// Whether `name = v` is one of the globals every session starts with, untouched.
fn builtin(name: &str, v: &Value) -> bool {
    match v {
        Value::Builtin(m, f) => m.fns.iter().any(|d| d.name == *f && d.is(name)),
        Value::Float(x) => modules::modules().any(|m| m.consts.iter().any(|(n, c)| *n == name && c.to_bits() == x.to_bits())),
        Value::Qty(x, u) => modules::modules().any(|m| {
            m.quantities.iter().any(|(n, c, ts)| *n == name && c.to_bits() == x.to_bits() && units::terms(ts).is_ok_and(|t| t.to_string() == u.to_string()))
        }),
        _ => false,
    }
}

/// JSON has no NaN or infinities, so those go as strings.
fn float(x: f64) -> Json {
    if x.is_finite() { json!(x) } else { json!(x.to_string()) }
}

/// The objects a `Saver` numbered, made again: empty shells first, so references between them (and cycles) resolve,
/// then their contents.
pub struct Loader {
    objects: Vec<Obj>,
    globals: Env,
}

enum Obj {
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<IndexMap<String, Value>>>),
    Set(Rc<RefCell<IndexSet<Value>>>),
    Scope(Env),
    Fn(Rc<Closure>),
}

impl Loader {
    /// `objects` made again, the one numbered `root` being `globals` itself. Functions saved without what they captured
    /// get `globals` instead, as do scopes from outside any session.
    pub fn new(objects: &Json, globals: Env, root: Option<usize>) -> Result<Loader, String> {
        let objects = objects.as_array().ok_or("no saved objects")?;
        let mut l = Loader { objects: Vec::with_capacity(objects.len()), globals };
        let mut fns = vec![];
        for (i, o) in objects.iter().enumerate() {
            l.objects.push(match o {
                o if o.get("list").is_some() => Obj::List(Default::default()),
                o if o.get("map").is_some() => Obj::Map(Default::default()),
                o if o.get("set").is_some() => Obj::Set(Default::default()),
                o if o.get("scope").is_some() && root == Some(i) => Obj::Scope(l.globals.clone()),
                o if o.get("scope").is_some() => Obj::Scope(Default::default()),
                o if o.get("fn").is_some() => {
                    fns.push(i);
                    // Made below, once every scope it could have captured exists.
                    Obj::Scope(Default::default())
                }
                o => return Err(format!("unknown saved object {}", short(o))),
            });
        }
        for i in fns {
            let src = objects[i]["fn"].as_str().ok_or("a saved function has no source")?;
            let env = match &objects[i]["env"] {
                Json::Null => l.globals.clone(),
                r => match l.obj(r)? {
                    Obj::Scope(env) => env.clone(),
                    _ => return Err(format!("the function `{src}` didn't capture a scope")),
                },
            };
            let def = match crate::parser::parse(src).map(|mut e| (e.pop(), e.is_empty())) {
                Ok((Some(crate::ast::Expr { kind: ExprKind::Fn(def), .. }), true)) => def,
                _ => return Err(format!("can't read the saved function `{src}`")),
            };
            l.objects[i] = Obj::Fn(Rc::new(Closure { def, env }));
        }
        for (i, o) in objects.iter().enumerate() {
            match &l.objects[i] {
                Obj::List(list) => *list.borrow_mut() = l.all(&o["list"])?,
                Obj::Map(map) => *map.borrow_mut() = l.pairs(&o["map"])?.into_iter().collect(),
                Obj::Set(set) => {
                    let vs = l.all(&o["set"])?;
                    if let Some(v) = vs.iter().find(|v| !v.hashable()) {
                        return Err(format!("a saved set holds a {}", v.type_name()));
                    }
                    *set.borrow_mut() = vs.into_iter().collect();
                }
                Obj::Scope(env) if o.get("scope").is_some() => {
                    let vars = l.pairs(&o["scope"])?;
                    let parent = match &o["parent"] {
                        Json::Null if root == Some(i) => None,
                        // A top-level scope of some other session: the builtins it left out come from this one's.
                        Json::Null => Some(l.globals.clone()),
                        r => match l.obj(r)? {
                            Obj::Scope(p) => Some(p.clone()),
                            _ => return Err("a saved scope's parent isn't a scope".into()),
                        },
                    };
                    let mut s = env.borrow_mut();
                    s.vars.extend(vars);
                    if root != Some(i) {
                        s.parent = parent;
                    }
                }
                _ => {}
            }
        }
        Ok(l)
    }

    fn obj(&self, r: &Json) -> Result<&Obj, String> {
        r["ref"].as_u64().and_then(|i| self.objects.get(i as usize)).ok_or_else(|| format!("bad saved reference {}", short(r)))
    }

    fn all(&self, j: &Json) -> Result<Vec<Value>, String> {
        j.as_array().ok_or("expected a saved list")?.iter().map(|v| self.value(v)).collect()
    }

    fn pairs(&self, j: &Json) -> Result<Vec<(String, Value)>, String> {
        let pair = |p: &Json| match p.as_array().map(Vec::as_slice) {
            Some([Json::String(k), v]) => Ok((k.clone(), self.value(v)?)),
            _ => Err(format!("bad saved entry {}", short(p))),
        };
        j.as_array().ok_or("expected saved entries")?.iter().map(pair).collect()
    }

    /// The value `j` saved; units resolve in the running session.
    pub fn value(&self, j: &Json) -> Result<Value, String> {
        let bad = || format!("bad saved value {}", short(j));
        let field = |k: &str| j.get(k).ok_or_else(bad);
        let text = |k: &str| field(k)?.as_str().ok_or_else(bad);
        let radix = || -> Result<Radix, String> {
            let n = |k: &str| field(k)?.as_u64().map(|n| n as u32).ok_or_else(bad);
            Ok(Radix { base: n("base")?, width: n("width")? })
        };
        let big = |s: &str| s.parse::<BigInt>().map_err(|_| bad());
        let pair = |k: &str| match field(k)?.as_array().map(Vec::as_slice) {
            Some([a, b]) => Ok((f64_of(a).ok_or_else(bad)?, f64_of(b).ok_or_else(bad)?)),
            _ => Err(bad()),
        };
        let unit = || units::parse(text("unit")?);
        let Json::Object(o) = j else {
            return Ok(match j {
                Json::Null => Value::Nil,
                Json::Bool(b) => Value::Bool(*b),
                Json::Number(n) => Value::int(n.as_i64().ok_or_else(bad)?),
                Json::String(s) => Value::str(s.as_str()),
                _ => return Err(bad()),
            });
        };
        Ok(match o.keys().find(|k| *k != "base" && *k != "width" && *k != "shown" && *k != "unit" && *k != "rows").map(String::as_str) {
            Some("ref") => match self.obj(j)? {
                Obj::List(l) => Value::List(l.clone()),
                Obj::Map(m) => Value::Map(m.clone()),
                Obj::Set(s) => Value::Set(s.clone()),
                Obj::Fn(f) => Value::Fn(f.clone()),
                Obj::Scope(_) => return Err(bad()),
            },
            Some("int") => Value::Int(field("int")?.as_i64().ok_or_else(bad)?, radix()?),
            Some("big") => Value::Big(Rc::new(big(text("big")?)?), radix()?),
            Some("frac") => Value::Frac(Rc::new(text("frac")?.parse::<BigRational>().map_err(|_| bad())?), field("shown")?.as_bool().ok_or_else(bad)?),
            Some("float") => Value::Float(f64_of(field("float")?).ok_or_else(bad)?),
            Some("qty") => Value::Qty(f64_of(field("qty")?).ok_or_else(bad)?, unit()?),
            Some("unc") => {
                let (x, e) = pair("unc")?;
                Value::unc(x, e, unit()?)
            }
            Some("cplx") => {
                let (re, im) = pair("cplx")?;
                Value::Cplx(re, im)
            }
            Some("mod") => match field("mod")?.as_array().map(Vec::as_slice) {
                Some([Json::String(r), Json::String(m)]) => Value::Mod(Rc::new((big(r)?, big(m)?))),
                _ => return Err(bad()),
            },
            Some("bytes") => Value::bytes(crate::modules::text::encoding::unhex(text("bytes")?).ok_or_else(bad)?),
            Some("regex") => Value::Regex(crate::state::regex(text("regex")?).map_err(|e| e.to_string())?.into()),
            Some("date") => Value::date(text("date")?.parse().map_err(|e| format!("bad saved date: {e}"))?),
            Some("table") => {
                let cols =
                    field("table")?.as_array().ok_or_else(bad)?.iter().map(|c| c.as_str().map(String::from).ok_or_else(bad)).collect::<Result<Vec<_>, _>>()?;
                let rows = field("rows")?.as_array().ok_or_else(bad)?.iter().map(|r| self.all(r)).collect::<Result<Vec<_>, _>>()?;
                if rows.iter().any(|r| r.len() != cols.len()) {
                    return Err(bad());
                }
                Value::table(Table { cols, rows })
            }
            Some("builtin") => {
                let name = text("builtin")?;
                let m = modules::modules().find(|m| m.fns.iter().any(|f| f.name == name)).ok_or_else(|| format!("no builtin `{name}` any more"))?;
                Value::Builtin(m, m.fns.iter().find(|f| f.name == name).unwrap().name)
            }
            _ => return Err(bad()),
        })
    }
}

fn f64_of(j: &Json) -> Option<f64> {
    j.as_f64().or_else(|| j.as_str()?.parse().ok())
}

/// The start of `j`, for errors.
fn short(j: &Json) -> String {
    let s = j.to_string();
    if s.chars().count() > 60 { format!("{}…", s.chars().take(60).collect::<String>()) } else { s }
}

pub fn save(it: &Interp) -> Json {
    let state = it.state();
    crate::state::enter(&state, || {
        let mut s = Saver::new(false);
        let globals = s.scope(&it.globals());
        let units: Vec<Json> =
            units::saved_units().into_iter().map(|(name, desc, scale, dim)| json!({ "name": name, "desc": desc, "scale": float(scale), "dim": dim })).collect();
        let rng = state.rng.borrow().get_seed();
        json!({ "version": VERSION, "units": units, "rng": rng, "globals": globals, "objects": s.objects() })
    })
}

/// Makes the session `j` saved again in `it`, which must be fresh.
pub fn load(it: &mut Interp, j: &Json) -> Result<(), String> {
    match j["version"].as_u64() {
        Some(VERSION) => {}
        Some(v) => return Err(format!("it's in format {v}, and this zil reads format {VERSION}")),
        None => return Err("it isn't a saved zil session".into()),
    }
    let state = it.state();
    crate::state::enter(&state, || {
        state.rng.borrow_mut().seed(j["rng"].as_u64().ok_or("no saved random state")?);
        for u in j["units"].as_array().ok_or("no saved units")? {
            let bad = || format!("bad saved unit {}", short(u));
            let dim = u["dim"].as_array().filter(|d| d.len() == units::NONE.len()).ok_or_else(bad)?;
            let mut d = units::NONE;
            for (slot, p) in d.iter_mut().zip(dim) {
                *slot = p.as_i64().and_then(|p| i8::try_from(p).ok()).ok_or_else(bad)?;
            }
            let (name, desc) = (u["name"].as_str().ok_or_else(bad)?, u["desc"].as_str().ok_or_else(bad)?);
            units::restore(name, desc, f64_of(&u["scale"]).ok_or_else(bad)?, d);
        }
        let root = j["globals"]["ref"].as_u64().ok_or("no saved globals")? as usize;
        Loader::new(&j["objects"], it.globals(), Some(root)).map(drop)
    })
}

#[cfg(test)]
mod tests {
    use crate::{Interp, run};

    /// `src` run in a session, then that session saved and loaded into a fresh one.
    fn reloaded(src: &str) -> Interp {
        let mut a = Interp::new();
        run(&mut a, src).unwrap();
        let saved = super::save(&a).to_string();
        let mut b = Interp::new();
        super::load(&mut b, &serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(super::save(&b).to_string(), saved, "saving again writes the same file");
        b
    }

    fn show(it: &mut Interp, src: &str) -> String {
        run(it, src).unwrap().to_string()
    }

    #[test]
    fn values_round_trip() {
        let src = r#"a = [nil, true, 2^70, 7/2, 1.5, nan, -inf, 3 kg*m/s^2, 5 ± 0.1 m, 2 - 3i, 5 mod 7, "hi", b"\x7fELF", r"a+", date("2026-10-06T14:30:00[Europe/Paris]"), {a: set(1, "x")}, table([{n: 1}]), max, 20 degC, 255 to hex, -1 to hex(8), 2^70 to bin]"#;
        let mut b = reloaded(src);
        let mut a = Interp::new();
        assert_eq!(show(&mut b, "a"), show(&mut a, &format!("{src}; a")));
        assert_eq!(show(&mut b, "a[5] == a[5]"), "false");
        assert_eq!(show(&mut b, "a[17](1, 3)"), "3");
    }

    #[test]
    fn sharing_and_cycles_survive() {
        let mut b = reloaded("a = [1]; b = a; m = {xs: a}; push(a, a)");
        assert_eq!(show(&mut b, "push(b, 2); [a.len, m.xs.len, a[1].len]"), "[3, 3, 3]");
    }

    #[test]
    fn functions_keep_what_they_captured() {
        let src = "n = 10
            counter = fn() { c = 0; fn() { c += 1; c + n } }
            tick = counter()
            tick(); tick()
            fact = |k| if k < 2 { 1 } else { k * fact(k - 1) }
            sq = {x * x | x in [1, 2]}
            typed = fn(a: int, b = a * 2, *c: num) -> int { a + b + c.len }";
        let mut b = reloaded(src);
        assert_eq!(show(&mut b, "[typed(1), typed(1, 1, 1, 2)]"), "[3, 4]");
        assert_eq!(show(&mut b, "tick()"), "13");
        assert_eq!(show(&mut b, "n = 20; tick()"), "24");
        assert_eq!(show(&mut b, "[fact(5), counter()()]"), "[120, 21]");
    }

    #[test]
    fn units_and_random_survive() {
        let mut b = reloaded("unit pizza; unit slice = pizza / 8; unit bunch = 12; x = 3 slice; seed(7)");
        assert_eq!(show(&mut b, "x to pizza"), "0.375 pizza");
        assert_eq!(show(&mut b, "24 to bunch"), "2 bunch");
        let mut a = Interp::new();
        run(&mut a, "seed(7)").unwrap();
        assert_eq!(show(&mut b, "rand(1, 1000)"), show(&mut a, "rand(1, 1000)"));
        assert_eq!(show(&mut b, "unit topping; 2 topping"), "2 topping");
    }

    #[test]
    fn bad_files_dont_load() {
        let load = |s: &str| super::load(&mut Interp::new(), &serde_json::from_str(s).unwrap());
        assert!(load(r#"{"version": 99}"#).unwrap_err().contains("format 99"));
        assert!(load("{}").is_err());
        let ok = super::save(&Interp::new());
        let mut broken = ok.clone();
        broken["objects"][0]["scope"] = serde_json::json!([["f", {"ref": 7}]]);
        assert!(load(&broken.to_string()).is_err());
        assert!(load(&ok.to_string()).is_ok());
    }
}
