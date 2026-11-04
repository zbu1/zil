//! What a REPL does with each input, shared by the terminal REPL (`zil-cli`) and the browser's (`zil-wasm`):
//! when a line continues, `help topic` and `topic?` shorthand, and results saved and labeled as `_n`.

use crate::ansi::{RESET, Role, highlight, tint};
use crate::lexer::{self, Tok};
use crate::snapshot::{Loader, Saver};
use crate::value::Value;
use crate::{Error, Session};
use serde_json::{Value as Json, json};
/// Typed `\name` followed by a space or Tab becomes the symbol, in the terminal REPL and the browser editor.
pub const ABBREVIATIONS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("omicron", "ο"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("to", "→"),
    ("inf", "∞"),
    ("infty", "∞"),
    ("deg", "°"),
    ("sqrt", "√"),
    ("le", "≤"),
    ("ge", "≥"),
    ("ne", "≠"),
    ("pm", "±"),
    ("div", "÷"),
    ("cross", "×"),
    ("cdot", "·"),
];

/// Runs one line of RPN on `stack`: values push, and operators and functions pop what they take and push the result. `swap`, `drop`,
/// `dup` and `clear` work the stack itself. After an error the stack is as it was.
pub fn rpn(session: &mut Session, stack: &mut Vec<Value>, line: &str) -> Result<(), Error> {
    let toks = lexer::lex(line)?;
    let mut work = stack.clone();
    let mut i = 0;
    while i < toks.len() {
        let (tok, span) = &toks[i];
        i += 1;
        let text = &line[span.clone()];
        let need = |n: usize, have: usize| match have >= n {
            true => Ok(()),
            false => Err(Error::new(format!("`{text}` takes {n} from the stack, which has {have}"), span.clone())),
        };
        match tok {
            Tok::Eof | Tok::Newline => {}
            Tok::Ident(w) if w == "clear" => work.clear(),
            Tok::Ident(w) if w == "drop" => {
                need(1, work.len())?;
                work.pop();
            }
            Tok::Ident(w) if w == "dup" => {
                need(1, work.len())?;
                work.extend(work.last().cloned());
            }
            Tok::Ident(w) if w == "swap" => {
                need(2, work.len())?;
                let n = work.len();
                work.swap(n - 1, n - 2);
            }
            // `3 -2 +`: a `-` touching a number is its sign.
            Tok::Minus if toks[i].1.start == span.end && matches!(toks[i].0, Tok::Int(_) | Tok::Float(_) | Tok::Dec(_) | Tok::Big(_) | Tok::Based(_)) => {
                work.push(session.eval(&line[span.start..toks[i].1.end])?);
                i += 1;
            }
            Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::SlashSlash
            | Tok::Percent
            | Tok::StarStar
            | Tok::Caret
            | Tok::Amp
            | Tok::Bar
            | Tok::Shl
            | Tok::Shr
            | Tok::Mod
            | Tok::Each(_) => {
                need(2, work.len())?;
                let (b, a) = (work.pop().expect("checked"), work.pop().expect("checked"));
                let v = session.enter(|| match tok {
                    Tok::Each(op) => crate::ops::each(*op, &a, &b),
                    t => crate::ops::binary(crate::parser::binop(t), &a, &b),
                });
                work.push(v.map_err(|m| Error::new(m, span.clone()))?);
            }
            Tok::Ident(_) | Tok::Bang | Tok::Sqrt => {
                let name = match tok {
                    Tok::Bang => "factorial",
                    Tok::Sqrt => "sqrt",
                    _ => text,
                };
                let v = session.eval(name)?;
                let Some(n) = arity(&v, &work) else {
                    work.push(v);
                    continue;
                };
                need(n, work.len())?;
                let args = work.split_off(work.len() - n);
                match session.call(&v, args)? {
                    Value::Nil => {}
                    r => work.push(r),
                }
            }
            _ => work.push(session.eval(text)?),
        }
    }
    *stack = work;
    Ok(())
}

/// How many values `f` takes from `stack`, or `None` if it isn't a function: the required params of its longest form that fits the
/// top of the stack, so `gcd` takes two ints but one list. With no fit, its shortest form's count, so the call reports the mismatch.
fn arity(f: &Value, stack: &[Value]) -> Option<usize> {
    use crate::signature::{Form, forms};
    let (users, hosts, builtins);
    let all: Vec<Form> = match f {
        Value::Fn(c) => vec![c.form("")],
        Value::Named(id) => {
            (users, hosts, builtins) = (id.users(), id.hosts(), id.overloads());
            let sigs = hosts.iter().map(|h| h.sig.as_str()).chain(builtins.iter().map(|o| o.doc.sig));
            users.iter().map(|c| c.form("")).chain(sigs.flat_map(|s| forms(s).unwrap_or_default())).collect()
        }
        _ => return None,
    };
    let fits = |g: &&Form| stack.len() >= g.min() && g.fits(&stack[stack.len() - g.min()..]);
    all.iter().filter(fits).map(Form::min).max().or(all.iter().map(Form::min).min())
}

/// The `\name` that ends `before`, if it's an abbreviation: its length in bytes and the symbol it stands for.
pub fn abbreviation(before: &str) -> Option<(usize, &'static str)> {
    let at = before.rfind('\\')?;
    let name = &before[at + 1..];
    ABBREVIATIONS.iter().find(|a| a.0 == name).map(|a| (before.len() - at, a.1))
}

/// More `(`/`[`/`{` than closers: the REPL keeps reading lines.
pub fn unclosed(src: &str) -> bool {
    let Ok(toks) = lexer::lex(src) else {
        return false;
    };
    let depth: i32 = toks
        .iter()
        .map(|(t, _)| match t {
            Tok::LParen | Tok::LBracket | Tok::LBrace => 1,
            Tok::RParen | Tok::RBracket | Tok::RBrace => -1,
            _ => 0,
        })
        .sum();
    depth > 0
}

/// REPL `help` / `help topic`: the topic (`None` for the overview); `None` for ordinary code.
pub fn help_shorthand(src: &str) -> Option<Option<&str>> {
    let rest = src.trim().strip_prefix("help")?;
    let topic = rest.trim_start();
    if topic.is_empty() {
        return Some(None);
    }
    let word = topic.chars().all(|c| c.is_alphanumeric() || c == '_');
    (rest.starts_with(char::is_whitespace) && word).then_some(Some(topic))
}

/// REPL `upper?` as `help upper`: a name or dotted chain asks for its last name (`now.moon_phase?` is `help moon_phase`),
/// a variable or any other expression is `help(...)` of it, and a lone `?` is the overview. Other lines come back as they are.
fn help_sugar<'a>(session: &Session, src: &'a str) -> std::borrow::Cow<'a, str> {
    // A line that starts with `to <target>` converts the last result: `to ft` is `_ to ft`.
    if let Some(rest) = src.trim_start().strip_prefix("to").filter(|r| r.starts_with(char::is_whitespace) && !r.trim_start().starts_with(['=', '(', '.'])) {
        return format!("_ to{rest}").into();
    }
    let Some(body) = src.trim_end().strip_suffix('?') else { return src.into() };
    // The sugar is a line's only `?` token, at its end: not one in a comment or string, nor a param's `x?`.
    let marks = lexer::lex(src).map(|toks| toks.into_iter().filter(|t| t.0 == Tok::Question).map(|t| t.1.start).collect::<Vec<_>>());
    if marks.ok().as_deref() != Some(&[body.len()]) {
        return src.into();
    }
    let body = body.trim();
    let name = |w: &str| w.starts_with(|c: char| c.is_alphabetic() || c == '_') && w.chars().all(|c| c.is_alphanumeric() || c == '_');
    match body.rsplit('.').next() {
        _ if body.is_empty() => "help".into(),
        _ if name(body) && session.get(body).is_some_and(|v| !matches!(v, Value::Named(..))) => format!("help({body})").into(),
        Some(last) if body.split('.').all(name) => format!("help {last}").into(),
        _ => format!("help({body})").into(),
    }
}

/// Whether `src` is help printed straight to the screen: `help upper`, or a line that is just `help(...)`, `search(...)` or `tip()`.
/// Help inside a bigger expression stays plain text for piping.
pub fn help_call(src: &str) -> bool {
    use crate::ast::ExprKind::{Call, Ident};
    // Desugared, so the command line `search str` counts as `search("str")`.
    let sugarless = crate::parser::desugar(src).unwrap_or_default();
    let call = matches!(crate::parser::parse(&sugarless).as_deref(), Ok([e]) if matches!(&e.kind, Call(f, _) if matches!(&f.kind, Ident(n) if n == "help" || n == "search" || n == "tip")));
    call || help_shorthand(src).is_some()
}

/// Units convertible from the expression before a trailing `to`, for completion. Read from literals and `s`'s
/// variables only: nothing is evaluated, so Tab has no side effects. Call it inside `s.enter` so user units count.
fn conversions(s: &Session, before: &str) -> Option<Vec<String>> {
    use crate::ast::ExprKind::{Ident, Qty};
    use crate::modules::units::{known_dim, names_with_dim};
    let toks = lexer::lex(before).ok()?;
    let [.., (Tok::To, to), (Tok::Eof, _)] = toks.as_slice() else {
        return None;
    };
    let e = last_expr(&before[..to.start])?;
    let dim = match &e.kind {
        Qty(_, spec) => known_dim(spec)?,
        Ident(name) => match s.get(name) {
            Some(Value::Qty(_, u)) => u.dim(),
            _ => known_dim(&vec![(name.clone(), 1)])?,
        },
        _ => return None,
    };
    Some(names_with_dim(dim))
}

/// Whether `src` ends in a quantity, a literal like `5 mph` or a variable holding one, that `to` could follow.
fn ends_in_quantity(s: &Session, src: &str) -> bool {
    use crate::ast::ExprKind::{Ident, Qty};
    last_expr(src).is_some_and(|e| match &e.kind {
        Qty(..) => true,
        Ident(name) => matches!(s.get(name), Some(Value::Qty(..))),
        _ => false,
    })
}

/// The last expression of the longest parseable tail of `src`, past any `x =`: `print(5 km` gives `5 km`.
fn last_expr(src: &str) -> Option<crate::ast::Expr> {
    let toks = lexer::lex(src).ok()?;
    let mut e = toks.iter().find_map(|(_, sp)| crate::parser::parse(&src[sp.start..]).ok())?.pop()?;
    while let crate::ast::ExprKind::Assign(_, rhs) = e.kind {
        e = *rhs;
    }
    Some(e)
}

/// The type of what `before` ends in, ahead of a `.`, and its keys (a map's) or columns (a table's). Found by evaluating
/// it with `Session::probe`, so `None` when that would change anything, errors or takes too long.
fn receiver(s: &Session, before: &str) -> Option<(String, Vec<String>)> {
    let v = s.probe(&last_expr(before.strip_suffix('.')?)?)?;
    let keys = match &v {
        Value::Map(m) => m.borrow().keys().cloned().collect(),
        Value::Table(t) => t.cols.clone(),
        _ => vec![],
    };
    Some((v.type_name().to_string(), keys))
}

/// After `.`: the receiver's keys or columns in order, then A-Z the functions `x.f` could call: closures, and builtins
/// whose first parameter takes its type (or `any`). Every function when the receiver's type is unknown.
fn after_dot(s: &Session, before: &str) -> Vec<String> {
    let (ty, keys) = receiver(s, before).unzip();
    let takes = |name: &str, ty: &str| {
        crate::modules::defs(name).filter_map(|(_, d)| crate::signature::forms(d.sig).ok()).any(|fs| {
            fs.iter().any(|g| {
                g.params.first().is_some_and(|p| p.0.iter().any(|t| *t == "any" || *t == ty || (*t == "num" && matches!(ty, "int" | "frac" | "float"))))
            })
        })
    };
    let mut fns: Vec<String> = s
        .names()
        .into_iter()
        .filter(|n| match s.get(n) {
            Some(Value::Fn(_)) => true,
            Some(Value::Named(id)) => ty.as_deref().is_none_or(|ty| !id.users().is_empty() || takes(&id.name(), ty)),
            _ => false,
        })
        .collect();
    fns.sort();
    fns.dedup();
    // Only keys `a.key` can spell.
    let ident = |k: &String| k.starts_with(|c: char| c.is_alphabetic() || c == '_') && k.chars().all(|c| c.is_alphanumeric() || c == '_');
    let mut out: Vec<String> = keys.unwrap_or_default().into_iter().filter(ident).collect();
    fns.retain(|f| !out.contains(f));
    out.extend(fns);
    out
}

/// Tab completions for `word`, given the text `before` it: after `to`, every unit the left side converts to, common ones
/// first; after `.`, the receiver's keys and the functions that take it (see `after_dot`); after a quantity and a space,
/// `to`; otherwise builtins, constants, keywords of three letters or more, `to` targets, modules, units, help topics and
/// the session's variables and units.
fn complete(s: &Session, before: &str, word: &str) -> Vec<String> {
    use crate::modules::{self, units};
    static WORDS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        let mut v: Vec<String> = modules::ALL.iter().map(|(path, _)| path.clone()).collect();
        v.extend(
            modules::modules()
                .flat_map(|m| m.fns.iter().map(|f| f.name).chain(m.consts.iter().map(|c| c.0)).chain(m.targets.iter().map(|t| t.0)).chain([m.name]))
                .map(String::from),
        );
        v.extend(units::TABLE.iter().flat_map(|u| u.0.split_whitespace()).map(String::from));
        v.extend(units::DIMS.iter().map(|d| d.0.to_string()));
        v.extend(["syntax", "examples", "advanced"].map(String::from));
        v.extend(["else", "while", "for", "mod", "return", "break", "continue", "true", "false", "nil", "and", "or", "not"].map(String::from));
        v
    });
    if let Some(mut units) = s.enter(|| conversions(s, before)) {
        units.retain(|n| n.starts_with(word));
        return units;
    }
    if before.ends_with('.') {
        let mut hits = s.enter(|| after_dot(s, before));
        hits.retain(|n| n.starts_with(word));
        return hits;
    }
    let mut hits: Vec<String> = s.enter(|| {
        if word.is_empty() && before.ends_with(' ') && ends_in_quantity(s, before) {
            vec!["to".into()]
        } else if word.is_empty() {
            vec![]
        } else {
            WORDS.iter().cloned().chain(s.names()).chain(units::user_units()).collect()
        }
    });
    hits.retain(|n| n.starts_with(word));
    hits.sort();
    hits.dedup();
    hits
}

/// When the word ending `before` is a command's argument (`ls src/ma`): where it starts, and the entries of its directory
/// that start with its last part, directories with a `/`. Dotfiles only once that part starts with `.`.
fn complete_path(s: &Session, before: &str) -> Option<(usize, Vec<String>)> {
    let start = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let word = &before[start..];
    if word.starts_with(['-', '"', '{', '|', '<', '>', '!']) {
        return None;
    }
    let lines = crate::parser::marks(before);
    let (_, words) = lines.iter().find(|(l, _)| l.start < start && before[l.end.min(start)..start].trim().is_empty())?;
    if words.iter().any(|(w, m)| w.start == start && matches!(m, crate::parser::Mark::Name)) {
        return None;
    }
    let dir = &word[..word.rfind(['/', '\\']).map_or(0, |i| i + 1)];
    let part = &word[dir.len()..];
    s.enter(|| {
        let host = crate::host::get();
        let mut names = host.list(if dir.is_empty() { "." } else { dir }).ok()?;
        names.retain(|n| n.starts_with(part) && (part.starts_with('.') || !n.starts_with('.')));
        names.sort();
        let path = |n: String| {
            let p = format!("{dir}{n}");
            let p = if host.metadata(&p).is_ok_and(|m| m.is_dir) { p + "/" } else { p };
            // Only plain words can go bare.
            if p.contains(|c: char| c.is_whitespace() || "\"{}'$|<>()".contains(c)) { crate::parser::quote(&p) } else { p }
        };
        Some((start, names.into_iter().map(path).collect()))
    })
}

/// A result as `-e` prints it: tables as a grid, everything else as its text.
fn display(s: &Session, v: &Value, color: bool) -> String {
    let v = &in_base(s, v);
    s.enter(|| match v {
        Value::Table(t) => crate::modules::data::tables::grid(t, color),
        v => v.to_string(),
    })
}

/// A plain integer result in the session's `--base`; anything else as it is.
fn in_base(s: &Session, v: &Value) -> Value {
    let base = s.settings().base;
    match v {
        Value::Int(n, r) if r.base == 10 && base != 10 => Value::Int(*n, crate::ast::Radix { base, width: r.width }),
        Value::Big(n, r) if r.base == 10 && base != 10 => Value::Big(n.clone(), crate::ast::Radix { base, width: r.width }),
        _ => v.clone(),
    }
}

/// `clear results`: forgets `_` and every `_n`, so numbering starts over at `_1`.
fn forget(s: &mut Session, n: &mut usize) {
    for k in 1..=*n {
        s.unset(&format!("_{k}"));
    }
    s.unset("_");
    *n = 0;
}

/// `clear session`: a fresh session in place of `s`, keeping its limits so Ctrl-C and the caps still reach it.
fn reset(s: &mut Session, n: &mut usize) {
    let limits = s.limits().clone();
    *s = Session::new(crate::host::get());
    *s.limits() = limits;
    *n = 0;
}

/// How the REPL shows a result.
pub enum Shown {
    /// nil and functions: nothing.
    Hidden,
    /// Multi-line text, like `help(upper)`, reads better bare and unnumbered.
    Bare,
    /// Everything else, labeled `_n =`.
    Numbered(usize),
    /// An assignment, labeled with what it assigned and the `_n` it's saved as: `a = 5  · _1`.
    Named(usize, Vec<(String, Value)>),
    /// A function assigned to a name: its signature, `f(x)`.
    Defined(String),
}

/// Saves `src`'s result `v` as `_`, and as the next `_n` (counted in `n`) when it's numbered.
fn record(s: &mut Session, n: &mut usize, src: &str, v: &Value) -> Shown {
    if let Some(sig) = s.enter(|| defined(src)) {
        return Shown::Defined(sig);
    }
    let named = s.enter(|| assigned(src)).map(|names| match &names[..] {
        [one] => vec![(one.clone(), v.clone())],
        _ => names.into_iter().filter_map(|k| Some((s.get(&k)?, k))).map(|(v, k)| (k, v)).collect(),
    });
    let shown = match (v, named) {
        (Value::Fn(f), Some(pairs)) if pairs.len() == 1 => return Shown::Defined(f.sig(&pairs[0].0)),
        (Value::Nil | Value::Fn(_), _) => return Shown::Hidden,
        (Value::Str(s), None) if s.contains(['\n', '\x1b']) => Shown::Bare,
        (_, named) => {
            *n += 1;
            s.set(&format!("_{n}"), v.clone());
            named.map_or(Shown::Numbered(*n), |pairs| Shown::Named(*n, pairs))
        }
    };
    s.set("_", v.clone());
    shown
}

impl Shown {
    /// How `v` shows as this, colored if `color`; empty when hidden.
    pub fn render(&self, s: &Session, v: &Value, color: bool) -> String {
        match self {
            Shown::Hidden => String::new(),
            Shown::Bare => s.display(v),
            Shown::Numbered(k) => show(s, *k, v, color),
            Shown::Named(k, pairs) => show_named(s, *k, pairs, color),
            Shown::Defined(sig) if color => format!("{}{sig}{RESET}", Role::Dim.code()),
            Shown::Defined(sig) => sig.clone(),
        }
    }
}

/// What one REPL input showed, as its transcript keeps it.
pub enum Out {
    /// Help, or nothing at all.
    Nothing,
    Shown(Shown, Value),
    /// The error and the source it points into.
    Error(Error, String),
}

/// The inputs a session ran and what each showed, saved with it so a resumed session shows them again. Each entry's
/// values are copied as it's added, so a list changed later still shows as it was.
#[derive(Default)]
pub struct Transcript(Vec<Json>);

impl Transcript {
    pub fn push(&mut self, s: &Session, src: &str, out: &Out) {
        let e = s.enter(|| {
            let mut saver = Saver::new(true);
            let mut e = json!({ "src": src });
            match out {
                Out::Nothing | Out::Shown(Shown::Hidden, _) => {}
                Out::Shown(shown, v) => {
                    e["value"] = saver.value(v);
                    match shown {
                        Shown::Hidden => unreachable!(),
                        Shown::Bare => e["shown"] = json!("bare"),
                        Shown::Numbered(k) => e["numbered"] = json!(k),
                        Shown::Named(k, pairs) => {
                            e["named"] = json!(k);
                            e["pairs"] = pairs.iter().map(|(name, v)| json!([name, saver.value(v)])).collect();
                        }
                        Shown::Defined(sig) => e["defined"] = json!(sig),
                    }
                }
                Out::Error(err, at) => {
                    let labels: Vec<Json> = err.labels.iter().map(|(span, text)| json!([span.start, span.end, text])).collect();
                    e["error"] = json!({ "msg": err.msg, "labels": labels, "notes": err.notes, "help": err.help, "src": at });
                }
            }
            e["objects"] = saver.objects();
            e
        });
        self.0.push(e);
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    /// Each input and what it showed, values made again in `s`.
    pub fn entries(&self, s: &Session) -> Result<Vec<(String, Out)>, String> {
        s.enter(|| self.0.iter().map(|e| entry(s, e)).collect())
    }
}

fn entry(s: &Session, e: &Json) -> Result<(String, Out), String> {
    let bad = || "bad transcript entry".to_string();
    let src = e["src"].as_str().ok_or_else(bad)?.to_string();
    let l = Loader::new(&e["objects"], s.0.globals(), None)?;
    let n = |k: &str| e[k].as_u64().map(|n| n as usize).ok_or_else(bad);
    let out = if let Some(err) = e.get("error") {
        let texts = |k: &str| err[k].as_array().ok_or_else(bad)?.iter().map(|t| t.as_str().map(String::from).ok_or_else(bad)).collect::<Result<Vec<_>, _>>();
        let label = |l: &Json| match l.as_array().map(Vec::as_slice) {
            Some([Json::Number(a), Json::Number(b), Json::String(t)]) => {
                Ok((a.as_u64().ok_or_else(bad)? as usize..b.as_u64().ok_or_else(bad)? as usize, t.clone()))
            }
            _ => Err(bad()),
        };
        let labels = err["labels"].as_array().ok_or_else(bad)?.iter().map(label).collect::<Result<Vec<_>, _>>()?;
        if labels.is_empty() {
            return Err(bad());
        }
        let msg = err["msg"].as_str().ok_or_else(bad)?.to_string();
        Out::Error(Error { msg, labels, notes: texts("notes")?, help: texts("help")?, limit: None }, err["src"].as_str().ok_or_else(bad)?.into())
    } else if let Some(v) = e.get("value") {
        let shown = if e.get("shown").is_some() {
            Shown::Bare
        } else if e.get("numbered").is_some() {
            Shown::Numbered(n("numbered")?)
        } else if e.get("named").is_some() {
            let pair = |p: &Json| match p.as_array().map(Vec::as_slice) {
                Some([Json::String(k), v]) => Ok((k.clone(), l.value(v)?)),
                _ => Err(bad()),
            };
            Shown::Named(n("named")?, e["pairs"].as_array().ok_or_else(bad)?.iter().map(pair).collect::<Result<_, String>>()?)
        } else {
            Shown::Defined(e["defined"].as_str().ok_or_else(bad)?.into())
        };
        Out::Shown(shown, l.value(v)?)
    } else {
        Out::Nothing
    };
    Ok((src, out))
}

/// A REPL's session file: the session, how many `_n` it has numbered, and its transcript.
fn save(s: &Session, n: usize, t: &Transcript) -> String {
    let mut j = crate::snapshot::save(&s.0);
    j["results"] = json!(n);
    if let Ok(dir) = s.0.state().host.cwd() {
        j["cwd"] = json!(dir);
    }
    j["transcript"] = Json::Array(t.0.clone());
    j.to_string()
}

/// What `save` wrote, on `host`: the session, its `_n` count and the transcript's entries, checked to load.
fn load(host: std::rc::Rc<dyn crate::host::Host>, saved: &str) -> Result<(Session, usize, Transcript), String> {
    let j: Json = serde_json::from_str(saved).map_err(|e| format!("it isn't a saved zil session: {e}"))?;
    let mut s = Session::new(host.clone());
    crate::snapshot::load(&mut s.0, &j)?;
    if let Some(dir) = j["cwd"].as_str()
        && host.cd(dir).is_err()
    {
        host.note(&format!("can't return to {dir}; staying in the current directory"));
    }
    let t = Transcript(j["transcript"].as_array().ok_or("it has no transcript")?.clone());
    t.entries(&s)?;
    Ok((s, j["results"].as_u64().unwrap_or(0) as usize, t))
}

/// How many inputs a session file's transcript holds; `None` if it isn't one.
fn saved_inputs(saved: &str) -> Option<usize> {
    Some(serde_json::from_str::<Json>(saved).ok()?["transcript"].as_array()?.len())
}

/// The signature the last statement of `src` defines, `f(x: int)`, if it is `f(x: int) = …`.
fn defined(src: &str) -> Option<String> {
    match crate::parser::parse(src).ok()?.pop()?.kind {
        crate::ast::ExprKind::Def(name, def) => Some(crate::interp::Closure { def, env: Default::default() }.sig(&name)),
        _ => None,
    }
}

/// What the last statement of `src` assigns to, as written: `a`, `m.k`, `xs[0]`, or each name in `[a, [b, _]] = ...`.
fn assigned(src: &str) -> Option<Vec<String>> {
    use crate::ast::{ExprKind, Pat};
    fn names(p: &Pat, out: &mut Vec<String>) {
        match p {
            Pat::Name(n) => out.push(n.clone()),
            Pat::Skip => {}
            Pat::List(ps, _) => ps.iter().for_each(|p| names(p, out)),
            Pat::Rest(p) => names(p, out),
            _ => unreachable!("only `match` has these patterns"),
        }
    }
    match crate::parser::parse(src).ok()?.pop()?.kind {
        ExprKind::Assign(lhs, _) => Some(vec![src.get(lhs.span)?.to_string()]),
        ExprKind::Unpack(p, _) => {
            let mut out = vec![];
            names(&p, &mut out);
            Some(out)
        }
        _ => None,
    }
}

/// An assignment's result: `a = 5  · _1`, or a table's grid labeled `· a · _1`.
fn show_named(s: &Session, n: usize, pairs: &[(String, Value)], color: bool) -> String {
    let pairs: Vec<_> = pairs.iter().map(|(k, v)| (k.clone(), in_base(s, v))).collect();
    let pairs = &pairs[..];
    s.enter(|| named(n, pairs, color))
}

fn named(n: usize, pairs: &[(String, Value)], color: bool) -> String {
    let (dim, reset) = if color { (Role::ResultLabel.code(), RESET) } else { ("", "") };
    if let [(name, Value::Table(t))] = pairs {
        return format!("{}{dim} · {name} · _{n}{reset}", crate::modules::data::tables::grid(t, color));
    }
    let tinted = |v: &Value| if color { paint(v) } else { clipped(&v.to_string()) };
    let body: Vec<String> = pairs.iter().map(|(k, v)| format!("{k} = {}", tinted(v))).collect();
    format!("{}  {dim}· _{n}{reset}", body.join(", "))
}

/// A numbered result labeled with the `_n` it's saved as, colored by type if `color`.
fn show(s: &Session, n: usize, v: &Value, color: bool) -> String {
    let v = &in_base(s, v);
    s.enter(|| numbered(n, v, color))
}

fn numbered(n: usize, v: &Value, color: bool) -> String {
    if let Value::Table(t) = v {
        let label = if color { format!("{} · _{n}{RESET}", Role::ResultLabel.code()) } else { format!(" · _{n}") };
        return crate::modules::data::tables::grid(t, color) + &label;
    }
    if !color {
        return format!("_{n} = {}", clipped(&v.to_string()));
    }
    format!("{}_{n} ={RESET} {}", Role::ResultLabel.code(), paint(v))
}

/// The most screen lines of one result shown before the rest is cut off.
const MAX_LINES: usize = 20;

fn more(rest: &str) -> String {
    format!("… ({} more chars)", rest.chars().count())
}

/// Where `text` passes `MAX_LINES` lines of `cols` columns, counting a long line's wrapped rows.
fn cut(text: &str, cols: usize) -> Option<usize> {
    let (mut row, mut col) = (0, 0);
    for (i, c) in text.char_indices() {
        if c == '\n' {
            (row, col) = (row + 1, 0);
        } else {
            if col == cols {
                (row, col) = (row + 1, 0);
            }
            col += 1;
        }
        if row == MAX_LINES {
            return Some(i);
        }
    }
    None
}

fn clipped(text: &str) -> String {
    match cut(text, crate::ansi::columns()) {
        Some(i) => format!("{}{}", &text[..i], more(&text[i..])),
        None => text.to_string(),
    }
}

/// A result in color: strings quoted and collections highlighted as typed, so `a = "5"` echoes like the input line.
fn paint(v: &Value) -> String {
    match v {
        Value::Str(s) if s.contains(['\n', '\x1b']) => format!("{}{v}{RESET}", Role::Str.code()),
        Value::Str(_) | Value::List(_) | Value::Map(_) | Value::Set(_) => {
            let text = format!("{v:?}");
            match cut(&text, crate::ansi::columns()) {
                Some(i) => format!("{}{}{}{RESET}", highlight(&text[..i]), Role::Dim.code(), more(&text[i..])),
                None => highlight(&text),
            }
        }
        v => format!("{}{v}{RESET}", tint(v).code()),
    }
}

/// What a REPL does with each input and result; the free functions above are these methods' bodies.
impl Session {
    /// `topic?` as `help topic` and a leading `to ft` as `_ to ft`: the line to run, as it is when neither applies.
    pub fn help_sugar<'a>(&self, src: &'a str) -> std::borrow::Cow<'a, str> {
        help_sugar(self, src)
    }

    /// Tab completions for `word`, given the text `before` it.
    pub fn complete(&self, before: &str, word: &str) -> Vec<String> {
        complete(self, before, word)
    }

    /// When the word ending `before` is a command's argument: where it starts, and the matching entries of its directory.
    pub fn complete_path(&self, before: &str) -> Option<(usize, Vec<String>)> {
        complete_path(self, before)
    }

    /// A result as `-e` prints it: tables as a grid, everything else as its text.
    pub fn show_plain(&self, v: &Value, color: bool) -> String {
        display(self, v, color)
    }

    /// `clear results`: forgets `_` and every `_n`, so numbering starts over at `_1`.
    pub fn forget_results(&mut self, n: &mut usize) {
        forget(self, n)
    }

    /// `clear session`: a fresh session in place of this one, keeping its limits.
    pub fn reset(&mut self, n: &mut usize) {
        reset(self, n)
    }

    /// Saves `src`'s result `v` as `_`, and as the next `_n` (counted in `n`) when it's numbered.
    pub fn record(&mut self, n: &mut usize, src: &str, v: &Value) -> Shown {
        record(self, n, src, v)
    }

    /// A numbered result labeled with the `_n` it's saved as, colored by type if `color`.
    pub fn show(&self, n: usize, v: &Value, color: bool) -> String {
        show(self, n, v, color)
    }

    /// An assignment's result: `a = 5  · _1`.
    pub fn show_named(&self, n: usize, pairs: &[(String, Value)], color: bool) -> String {
        show_named(self, n, pairs, color)
    }
}

impl Transcript {
    /// A REPL's session file: the session, how many `_n` it has numbered, and this transcript.
    pub fn save(&self, s: &Session, n: usize) -> String {
        save(s, n, self)
    }

    /// What `save` wrote, on `host`: the session, its `_n` count and the transcript, checked to load.
    pub fn load(host: std::rc::Rc<dyn crate::host::Host>, saved: &str) -> Result<(Session, usize, Transcript), String> {
        load(host, saved)
    }

    /// How many inputs a session file's transcript holds; `None` if it isn't one.
    pub fn saved_inputs(saved: &str) -> Option<usize> {
        saved_inputs(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defining_on_a_user_type_shows_its_signature() {
        let mut s = Session::new(crate::host::get());
        let (mut n, mut t) = (0, Transcript::default());
        for src in ["h(x) = x", "type v = {x: num}", "f(a: v) = a.x", "g = fn(a: v) { a.x }", "r = v(1)"] {
            let v = s.eval(src).unwrap();
            let shown = s.record(&mut n, src, &v);
            t.push(&s, src, &Out::Shown(shown, v));
        }
        assert_eq!(t.entries(&s).unwrap().len(), 5);
        let f = s.eval("f").unwrap();
        let shown = s.record(&mut n, "f(a: v) = a.x", &f);
        assert_eq!(shown.render(&s, &f, false), "f(a: v)");
        let (mut back, _, t) = Transcript::load(crate::host::get(), &t.save(&s, n)).unwrap();
        assert_eq!(t.entries(&back).unwrap().len(), 5);
        assert_eq!(back.eval("r.x = 2; f(r)").unwrap().to_string(), "2");
    }

    #[test]
    fn long_results_are_cut() {
        assert_eq!(clipped("short"), "short");
        // Without a terminal the width is 80, so 20 rows hold 1600 chars.
        assert_eq!(clipped(&"x".repeat(1607)), format!("{}… (7 more chars)", "x".repeat(1600)));
        assert_eq!(clipped(&"a\n".repeat(30)), format!("{}a… (21 more chars)", "a\n".repeat(19)));
    }

    #[test]
    fn rpn_stack() {
        let mut s = Session::new(crate::host::get());
        let mut stack = vec![];
        let mut run = |line: &str| rpn(&mut s, &mut stack, line).map(|_| stack.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).map_err(|e| e.msg);
        assert_eq!(run("3 4 + 2 *").unwrap(), "14");
        assert_eq!(run("clear 2 3 swap -").unwrap(), "1");
        assert_eq!(run("clear 3 -2 +").unwrap(), "1");
        assert_eq!(run("clear 16 sqrt dup").unwrap(), "4 4");
        assert_eq!(run("drop").unwrap(), "4");
        assert_eq!(run("clear 12 18 gcd").unwrap(), "6");
        assert_eq!(run("clear 5 !").unwrap(), "120");
        assert_eq!(run("clear pi 2 *").unwrap(), "6.28319");
        assert_eq!(run(r#"clear "ab" upper"#).unwrap(), "AB");
        assert_eq!(run("clear 1").unwrap(), "1");
        assert_eq!(run("2 + +").unwrap_err(), "`+` takes 2 from the stack, which has 1");
        assert_eq!(run("").unwrap(), "1");
    }

    #[test]
    fn abbreviations() {
        assert_eq!(abbreviation(r"x = \alpha"), Some((6, "α")));
        assert_eq!(abbreviation(r"x = \nope"), None);
        assert_eq!(abbreviation("alpha"), None);
        for (name, symbol) in ABBREVIATIONS {
            assert!(lexer::lex(&format!("1{symbol}")).is_ok(), "\\{name} gives {symbol}, which doesn't lex");
        }
        assert_eq!(crate::help::eval("α = 3; π + α - α"), "3.14159");
    }

    #[test]
    fn session() {
        assert!(unclosed("f = fn(x) {") && !unclosed("[1, 2]") && !unclosed(r#""(""#));
        assert_eq!(help_shorthand("help upper"), Some(Some("upper")));
        assert_eq!(help_shorthand("help"), Some(None));
        assert_eq!(help_shorthand("help(upper)"), None);
        assert!(help_call("search str") && help_call(r#"search("str")"#) && !help_call(r#"search("str") |> grep("x")"#));
        let mut s = Session::new(crate::host::get());
        s.eval("a = 1").unwrap();
        let help_sugar = |src| help_sugar(&s, src);
        assert_eq!(help_sugar("a?"), "help(a)");
        assert_eq!(help_sugar("now.moon_phase?"), "help moon_phase");
        assert_eq!(help_sugar("upper ?"), "help upper");
        assert_eq!(help_sugar(" ? "), "help");
        assert_eq!(help_sugar("(1..3)?"), "help((1..3))");
        assert_eq!(help_sugar("1.5?"), "help(1.5)");
        assert_eq!(help_sugar("x # why?"), "x # why?");
        assert_eq!(help_sugar(r#""what?"#), r#""what?"#);
        assert_eq!(help_sugar("a? + b?"), "a? + b?");
        assert_eq!(help_sugar("f = |x?| x"), "f = |x?| x");
        assert_eq!(help_sugar("to ft"), "_ to ft");
        assert_eq!(help_sugar("to = 5"), "to = 5");

        let (mut it, mut n) = (Session::new(crate::host::get()), 0);
        assert_eq!(complete(&it, "x.", "upp"), ["upper"]);
        assert_eq!(complete(&it, "print(5 km to ", "ma"), ["marathon"]);
        let dot = |it: &Session, before: &str| complete(it, before, "");
        assert!(dot(&it, r#""hi"."#).contains(&"upper".into()) && !dot(&it, "{}.").contains(&"upper".into()));
        assert!(dot(&it, "{}.").contains(&"keys".into()) && dot(&it, "{}.").contains(&"type".into()));
        assert!(dot(&it, "f(x).").contains(&"upper".into()) && dot(&it, "f(x).").contains(&"keys".into()));
        it.eval(r#"m = {zed: {s: "x"}, "a b": 1, keys: 2}; g = |x| x"#).unwrap();
        let m = dot(&it, "m.");
        assert_eq!(m[..2], ["zed", "keys"]);
        assert!(m.contains(&"g".into()) && !m.contains(&"upper".into()) && m.iter().filter(|k| *k == "keys").count() == 1);
        assert!(dot(&it, "m.zed.s.").contains(&"upper".into()) && !dot(&it, "m.zed.s.").contains(&"keys".into()));
        assert_eq!(complete(&it, "print(m.", "ze"), ["zed"]);
        let mut put = |src: &str| {
            let v = it.eval(src).unwrap();
            match record(&mut it, &mut n, src, &v) {
                Shown::Hidden => "hidden".to_string(),
                Shown::Bare => "bare".to_string(),
                Shown::Numbered(k) => show(&it, k, &v, false),
                Shown::Named(k, pairs) => show_named(&it, k, &pairs, false),
                Shown::Defined(sig) => sig,
            }
        };
        assert_eq!(put("1 + 1"), "_1 = 2");
        assert_eq!(put("nil"), "hidden");
        assert_eq!(put(r#""a\nb""#), "bare");
        assert_eq!(put("_1 * 10"), "_2 = 20");
        assert_eq!(put("_"), "_3 = 20");
        assert_eq!(put("a = [1, 2]"), "a = [1, 2]  · _4");
        assert_eq!(put("a += [3]"), "a = [1, 2, 3]  · _5");
        assert_eq!(put("a[0] = 9"), "a[0] = 9  · _6");
        assert_eq!(put("[x, [_, y]] = [1, [2, 3]]"), "x = 1, y = 3  · _7");
        assert_eq!(put("f = |x, [a, _]| x"), "f(x, [a, _])");
        assert_eq!(put("|x| x"), "hidden");
        assert_eq!(put("b = 1; 2"), "_8 = 2");
        assert_eq!(
            put(r#"s = "a
b""#),
            "s = a
b  · _9"
        );
        forget(&mut it, &mut n);
        assert!(it.get("_").is_none() && it.get("_9").is_none());
        assert!(matches!(record(&mut it, &mut n, "3", &Value::int(3)), Shown::Numbered(1)));
        let a = vec![("a".to_string(), Value::Str("5".into()))];
        assert_eq!(crate::ansi::strip_ansi(&show_named(&it, 1, &a, true)), r#"a = "5"  · _1"#);
        assert!(show_named(&it, 1, &a, true).contains(&format!("{}\"5\"{RESET}", Role::Str.code())));

        it.limits().steps = 1234;
        reset(&mut it, &mut n);
        assert!(it.get("a").is_none() && it.get("_1").is_none() && n == 0 && it.limits().steps == 1234);
    }

    #[test]
    fn dot_completes_by_type() {
        let mut it = Session::new(crate::host::get());
        // The test host's clock stands still, so the probe's 0.2 s never passes: cap steps instead.
        it.limits().steps = 1_000_000;
        let all = complete(&it, "nope.", "").len();
        #[rustfmt::skip]
        let exprs = ["nil", "true", "5", "1/3", "1.5", "5 km", "5 ± 0.1", "2+3i", "5 mod 7", r#""s""#, r#"r"a""#, "now", "[1]", "{}",
            "set([1])", "table([{a: 1}])", "|x| x", "(1/3)", "sq(3)", "{a: 1}.a"];
        it.eval("sq = fn(x) { y = x * x; y }").unwrap();
        for src in exprs {
            let v = it.eval(src).unwrap();
            it.set("v", v);
            let by_value = complete(&it, "v.", "");
            assert!(by_value.len() < all, "{src}");
            assert_eq!(complete(&it, &format!("{src}."), ""), by_value, "{src}");
        }
        assert!(it.get("y").is_none());

        // Receivers that would change something aren't run: every function, and nothing changed.
        it.eval("xs = [1, 2]; n = 0; inc = fn() { n += 1; n }").unwrap();
        let all = complete(&it, "nope.", "").len();
        for src in ["xs.push(3)", "inc()", "[rand()]", r#"print("hi")"#, "(fn() { while true {} })()", "(fn() { xs[0] = 9; 1 })()"] {
            assert_eq!(complete(&it, &format!("{src}."), "").len(), all, "{src}");
        }
        assert_eq!(it.eval("[xs, n]").unwrap().to_string(), "[[1, 2], 0]");
        let roll = |it: &mut Session| it.eval("rand()").unwrap().to_string();
        it.eval("seed(7)").unwrap();
        let first = roll(&mut it);
        it.eval("seed(7)").unwrap();
        complete(&it, "[rand()].", "");
        assert_eq!(roll(&mut it), first);
    }

    #[test]
    fn common_units_convert() {
        use crate::modules::units::{COMMON, DIMS, parse};
        for (kind, units) in COMMON {
            let dim = DIMS.iter().find(|d| d.0 == *kind).unwrap_or_else(|| panic!("no dim {kind}")).1;
            for u in *units {
                assert_eq!(parse(u).map(|u| u.dim()), Ok(dim), "{kind}: {u}");
            }
        }
    }

    #[test]
    fn transcript_round_trips() {
        let (mut s, mut n, mut t) = (Session::new(crate::host::get()), 0, Transcript::default());
        let mut shown = vec![];
        for src in [
            "xs = [1]",
            "push(xs, 2)",
            "f = |x| x",
            "a, b = 1, 255 to hex",
            "\"a
b\"",
            "nope",
            "help upper",
        ] {
            let out = match s.eval(src) {
                Ok(_) if help_shorthand(src).is_some() => Out::Nothing,
                Ok(v) => Out::Shown(record(&mut s, &mut n, src, &v), v),
                Err(e) => Out::Error(e, src.into()),
            };
            t.push(&s, src, &out);
            shown.push(render(&s, &out));
        }
        let (mut s, k, t) = load(crate::host::get(), &save(&s, n, &t)).unwrap();
        assert_eq!(k, n);
        let past = t.entries(&s).unwrap();
        assert_eq!(past.iter().map(|(_, o)| render(&s, o)).collect::<Vec<_>>(), shown);
        assert_eq!(past[0].0, "xs = [1]");
        assert!(shown[0].starts_with("xs = [1]") && shown[5].contains("nope"));
        assert_eq!(s.eval("xs").unwrap().to_string(), "[1, 2]");
        assert_eq!(saved_inputs(&save(&s, k, &t)), Some(7));
    }

    fn render(s: &Session, out: &Out) -> String {
        match out {
            Out::Nothing => String::new(),
            Out::Shown(shown, v) => shown.render(s, v, false),
            Out::Error(e, src) => e.render("<repl>", src, false),
        }
    }
}
