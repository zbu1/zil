//! `help`: rendered from each module's docs; every example shown is evaluated live in a fresh interpreter.

use crate::ansi::{RESET, Role};
use crate::guide::{ADVANCED, ADVANCED_UNRUN, HIGHLIGHTS, KEYWORDS, Keyword, SHELL, SHELL_INTRO, SYNTAX};
use crate::interp::Interp;
use crate::modules::{self, ALL, Doc, Module, Section, modules, units};
use std::cell::{Cell, RefCell};

pub mod docs;

thread_local! {
    /// What the page renderers below write to, read back by `capture`.
    static OUT: RefCell<String> = const { RefCell::new(String::new()) };
    static COLOR: Cell<bool> = const { Cell::new(false) };
    /// Set by `live`: help is going straight to a terminal, so color it if `.0` and wrap it to width `.1`.
    static TERM: Cell<Option<(bool, usize)>> = const { Cell::new(None) };
    /// Set by `linked`: names and examples become `zil:help/...` and `zil:run/...` hyperlinks, for the sandbox to make clickable.
    static LINKS: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with every help page it renders shaped for a terminal: `Some((color, width))`.
pub fn live<T>(term: Option<(bool, usize)>, f: impl FnOnce() -> T) -> T {
    TERM.set(term);
    let r = f();
    TERM.set(None);
    r
}

/// Runs `f` with help's names and examples rendered as links if `on`.
#[cfg(any(target_arch = "wasm32", test))]
pub fn linked<T>(on: bool, f: impl FnOnce() -> T) -> T {
    LINKS.set(on);
    let r = f();
    LINKS.set(false);
    r
}

/// `text` linked to `zil:{kind}/{target}` when links are on: `help` opens a help topic, `run` runs code.
fn link(kind: &str, target: &str, text: String) -> String {
    if !LINKS.get() {
        return text;
    }
    // Percent-encoded, so the link holds no spaces for `wrap` to split at.
    let target: String =
        target.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    crate::ansi::link(&format!("zil:{kind}/{target}"), &text)
}

/// `println!` into the help buffer.
macro_rules! out {
    ($($t:tt)*) => {
        $crate::help::push(format!($($t)*))
    };
}
pub(crate) use out;

pub fn push(line: String) {
    OUT.with_borrow_mut(|o| {
        o.push_str(&line);
        o.push('\n');
    });
}

/// Runs a page renderer, returning its text; `Err` holds the miss note if it returned false.
pub fn capture(color: bool, render: impl FnOnce() -> bool) -> Result<String, String> {
    COLOR.set(TERM.get().map_or(color, |t| t.0));
    OUT.take();
    let found = render();
    let mut text = OUT.take().trim_start_matches('\n').trim_end().to_string();
    if let Some((_, width)) = TERM.get() {
        text = text.lines().map(|l| wrap(l, width)).collect::<Vec<_>>().join("\n");
    }
    if found { Ok(text) } else { Err(text) }
}

/// `line` fit to `width` by breaking its last column (after the last run of 2+ spaces) at spaces, continuing under
/// that column, or under a `→ result`'s result. Earlier columns (labels, code, signatures) are never broken; when the
/// last column starts with under 20 to work with, it moves to the next line, under the second column or the indent + 4.
/// Escape codes take no width.
fn wrap(line: &str, width: usize) -> String {
    let esc = &*crate::ansi::ESCAPES;
    let vis = |s: &str| esc.replace_all(s, "").chars().count();
    if vis(line) <= width {
        return line.to_string();
    }
    // Words with their start column and whether 2+ spaces come before them (a new column). Escape codes hold no
    // spaces, so splitting on spaces keeps each one inside a word.
    let (mut words, mut at, mut gap) = (Vec::new(), 0, 0);
    for (i, p) in line.split(' ').enumerate() {
        if i > 0 {
            (at, gap) = (at + 1, gap + 1);
        }
        if !p.is_empty() {
            words.push((p, at, gap >= 2 && !words.is_empty()));
            (at, gap) = (at + vis(p), 0);
        }
    }
    let lead = words.first().map_or(0, |w| w.1);
    let last = words.iter().rposition(|w| w.2).unwrap_or(0);
    let second = words.iter().find(|w| w.2).map_or(lead, |w| w.1);
    let roomy = |c: usize| width.saturating_sub(c) >= 20;
    let (mut out, mut at, mut hang) = (String::new(), 0, 0);
    for (i, &(p, start, _)) in words.iter().enumerate() {
        let spaces = if i == 0 { start } else { start - words[i - 1].1 - vis(words[i - 1].0) };
        let n = vis(p);
        let newline = if i == last && last > 0 && !roomy(start) {
            Some(if second < start && roomy(second) { second } else { lead + 4 })
        } else {
            (i > last && at + spaces + n > width).then_some(hang)
        };
        match newline {
            Some(c) => {
                out.push('\n');
                out.push_str(&" ".repeat(c));
                at = c;
            }
            None => {
                out.push_str(&" ".repeat(spaces));
                at += spaces;
            }
        }
        if i == last {
            hang = at + if esc.replace_all(p, "") == "→" { 2 } else { 0 };
        }
        out.push_str(p);
        at += n;
    }
    out
}

/// Help for `topic` (an overview if `None`), with ANSI colors if `color`.
pub fn help(topic: Option<&str>, color: bool) -> Result<String, String> {
    capture(color, || render(topic))
}

/// Writes help for `topic`, every page it names when it names several (`table` is a function and a type); false if nothing matched.
fn render(topic: Option<&str>) -> bool {
    let Some(topic) = topic else {
        overview();
        return true;
    };
    let mut any = false;
    // Each page after the first follows a blank line; a renderer that misses leaves nothing behind.
    let mut also = |render: &mut dyn FnMut() -> bool| {
        let before = OUT.with_borrow(|o| o.len());
        let hit = render();
        OUT.with_borrow_mut(|o| match hit {
            false => o.truncate(before),
            true if any && !o[before..].starts_with('\n') => o.insert(before, '\n'),
            true => {}
        });
        any |= hit;
    };
    also(&mut || docs().find(|(_, f)| f.is(topic)).map(|(m, f)| function(m, f)).is_some());
    also(&mut || KEYWORDS.iter().find(|k| k.0.contains(&topic)).map(keyword).is_some());
    also(&mut || {
        match topic {
            "examples" => examples(),
            "syntax" => syntax(),
            "shell" => shell(),
            "advanced" => advanced(),
            _ => return false,
        }
        true
    });
    let module = modules::find(topic);
    also(&mut || module.map(page).is_some());
    // A module's page already shows its own topic.
    for f in modules().filter(|m| !module.is_some_and(|p| std::ptr::eq(p, *m))).filter_map(|m| m.topic) {
        also(&mut || f(topic));
    }
    also(&mut || type_page(topic));
    any || found(topic)
}

/// `help("shell")`: how command lines work, every command and its flags, then the shorthands.
fn shell() {
    for (i, (para, section)) in SHELL_INTRO.iter().enumerate() {
        out!("{}{}", if i == 0 { "" } else { "\n" }, ticked(para));
        for (heading, lines) in rows(&[*section], true).into_iter().filter(|r| !r.1.is_empty()) {
            framed(&paint(Role::Heading, heading), &lines);
        }
    }
    framed(&paint(Role::Heading, "commands"), &commands());
    for (heading, lines) in rows(SHELL, true) {
        framed(&paint(Role::Heading, heading), &lines);
    }
}

/// `search(word)`: functions mentioning `word`, even when it names a function or page, with colors if `color`.
pub fn search_page(topic: &str, color: bool) -> Result<String, String> {
    capture(color, || found(topic))
}

/// Writes the search hits for `topic`; false if there were none.
fn found(topic: &str) -> bool {
    let (hits, stems) = (search(topic), stems(topic));
    let kws: Vec<_> = KEYWORDS.iter().filter(|k| stems.iter().any(|s| k.0.iter().any(|w| w.contains(s.as_str())) || k.2.contains(s.as_str()))).collect();
    if hits.is_empty() && kws.is_empty() {
        let near: Vec<_> = near(topic).iter().map(|n| link("help", n, paint(Role::Name, n))).collect();
        let hint = if near.is_empty() { String::new() } else { format!("; did you mean {}?", near.join(", ")) };
        out!("no help for {topic:?}{hint}  help() for an overview");
        return false;
    }
    let count = |n: usize, what: &str| (n > 0).then(|| format!("{n} {what}{}", if n == 1 { "" } else { "s" }));
    let counts: Vec<_> = [count(hits.len(), "function"), count(kws.len(), "keyword")].into_iter().flatten().collect();
    let head = paint(Role::Heading, &format!("search {topic:?}: {}", counts.join(", ")));
    framed(
        &head,
        &collected(|| {
            if !kws.is_empty() {
                out!("{}", paint(Role::ModulePath, "keywords"));
            }
            for k in kws {
                out!("  {}\n    {}", link("help", k.0[0], mark(k.1, &stems, Role::Keyword)), mark(k.2, &stems, Role::Plain));
            }
            listing(hits, &stems)
        }),
    );
    true
}

/// `help(value)`: every builtin whose first parameter takes that type; false for a type with none.
pub fn type_page(ty: &str) -> bool {
    // A typed first parameter that names `ty` (or `num`, for numbers); `any` would list everything.
    let takes = |f: &Doc| {
        let num = matches!(ty, "int" | "frac" | "float");
        crate::signature::forms(f.sig).is_ok_and(|fs| fs.iter().any(|g| g.params.first().is_some_and(|p| p.0.iter().any(|t| *t == ty || (num && *t == "num")))))
    };
    let hits: Vec<_> = docs().filter(|(_, f)| takes(f)).collect();
    if hits.is_empty() {
        return false;
    }
    framed(&format!("functions taking a {} first, so x.f(...) works", paint(Role::Type, ty)), &collected(|| listing(hits, &[])));
    true
}

/// `help()`: what zil is, its module tree, and how to dig deeper.
fn overview() {
    out!("{}: a terse calculator and scripting language for units, dates and much more.", paint(Role::ModulePath, "zil"));
    let start = [
        ("help examples", "a few dozen one-liners, module by module"),
        ("help syntax", "the language at a glance"),
        ("help shell", "shell command shorthands like ls -la, cd, or !git status, which can all be piped"),
        ("help advanced", "longer recipes that combine several modules"),
        ("clear", "clear the screen (or Ctrl+L)"),
        ("clear history", "forget past inputs, so Up starts empty"),
        ("clear results", "forget _ and _1, _2…, so numbering restarts"),
        ("clear session", "start over: forget variables, functions and units"),
        ("clear all", "all of the above, and clear the screen"),
        ("exit", "leave the REPL (or quit, Ctrl+D)"),
        ("Alt+Q", "set the line aside into history, Up brings it back"),
    ];
    let usage = [
        ("help upper", "a function: signature, live examples, related functions"),
        ("search str", "every function whose name or description mentions a word"),
        (r#"help("text") |> grep("case")"#, "help is text, so grep narrows a long page to matching lines"),
        ("help(today)", "a value: every function that takes its type"),
        ("help if", "a keyword: if, for, fn, to and the rest"),
        ("now.moon_phase?", "? after anything shows its help, here the same as help moon_phase"),
    ];
    // One width across both tables so their descriptions line up.
    let w = start.iter().chain(&usage).map(|r| r.0.chars().count()).max().unwrap_or(0);
    for (title, rows) in [("start", &start[..]), ("help usage", &usage[..])] {
        framed(
            &paint(Role::Heading, title),
            &rows.iter().map(|(ex, what)| format!("{}{}  {}", code(ex), pad(ex, w), paint(Role::Dim, what))).collect::<Vec<_>>(),
        );
    }
    let types = |m: &Module| m.guide.iter().find(|s| s.0 == "types").map(|s| s.1.iter().map(|r| r.0.split(':').next().unwrap()).collect::<Vec<_>>().join(" "));
    // Indented by depth: two spaces per dot in the path.
    let rows: Vec<_> = ALL.iter().map(|(path, m)| (format!("{}{}", "  ".repeat(path.matches('.').count()), m.name), *m)).collect();
    let w = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let rows: Vec<_> = rows
        .into_iter()
        .map(|(name, m)| {
            let types = types(m).map_or(String::new(), |t| format!(" {}", paint(Role::Dim, &format!("[{t}]"))));
            let label = name.replace(m.name, &link("help", m.name, paint(if name.starts_with(' ') { Role::Submodule } else { Role::ModulePath }, m.name)));
            format!("{label}{}  {}{types}", pad(&name, w), m.about)
        })
        .collect();
    framed(&paint(Role::Heading, "modules"), &rows);
}

/// `help("examples")`: the highlights, then every module's showcase, aligned per module so one long line doesn't stretch them all.
fn examples() {
    sections(HIGHLIGHTS);
    for m in modules() {
        sections(m.examples);
    }
    out!("\n{}", paint(Role::Dim, r#"help("time") etc. for a module's full function list"#));
}

/// `help("advanced")`: each recipe's title and modules, its script, then its live result.
fn advanced() {
    let unrun = std::iter::once((ADVANCED_UNRUN, false));
    for ((theme, recipes), run) in ADVANCED.iter().map(|t| (*t, true)).chain(unrun) {
        out!("\n{}", paint(Role::Heading, theme));
        for (title, uses, src) in recipes {
            let mut lines: Vec<_> = src.lines().map(|l| snippet(l, src)).collect();
            if run {
                lines.push(format!("{} {}", paint(Role::Dim, "→"), below(&result(src).1)));
            }
            framed(&format!("{}  {}", paint(Role::Recipe, title), paint(Role::Dim, &format!("({uses})"))), &lines);
        }
    }
}

/// `help("syntax")`: the language at a glance, then every keyword.
fn syntax() {
    sections(SYNTAX);
    let w = KEYWORDS.iter().map(|k| k.0.join(", ").chars().count()).max().unwrap_or(0);
    let rows: Vec<_> = KEYWORDS
        .iter()
        .map(|k| {
            let words = k.0.iter().map(|w| link("help", w, paint(Role::Keyword, w))).collect::<Vec<_>>().join(", ");
            format!("{words}{}  {}", pad(&k.0.join(", "), w), k.2)
        })
        .collect();
    framed(&paint(Role::Heading, "keywords"), &rows);
}

/// A keyword's page: its forms and what it does, live examples, related names.
fn keyword(&(words, form, desc, examples, see): &Keyword) {
    let lines = collected(|| {
        out!("{}  {desc}", lit(form));
        show(examples.iter().map(|e| e.to_string()).collect());
        out!("{} {}", paint(Role::Dim, "see also:"), names(see, ", "));
    });
    framed(&format!("{} {}", paint(Role::Heading, "keyword"), paint(Role::Keyword, &words.join(", "))), &lines);
}

/// Every builtin, with its module.
fn docs() -> impl Iterator<Item = (&'static Module, &'static Doc)> {
    modules().flat_map(|m| m.fns.iter().map(move |f| (m, f)))
}

/// A function's page: where it lives, signature, live examples, related functions.
fn function(m: &Module, f: &Doc) {
    let group = m.groups.iter().find(|g| g.1.contains(&f.name)).map_or(String::new(), |g| format!(" › {}", g.0));
    let path = modules::path(m);
    framed(&format!("{}{}", link("help", path, paint(Role::ModulePath, path)), paint(Role::Dim, &group)), &collected(|| body(m, f, true)));
}

/// A function's signature and one-line description, then on its own page (`full`) the rest of its description, live examples
/// and related functions, or on its module's page its first example.
fn body(m: &Module, f: &Doc, full: bool) {
    let sig = sig(f, link("help", f.name, paint(Role::Name, f.name)));
    if !full {
        out!("{sig}\n  {}", f.desc);
        match (f.examples.first(), f.shown.first()) {
            (Some(ex), _) => show(vec![ex.to_string()]),
            (None, Some(ex)) => out!("  {}", code(ex)),
            (None, None) => {}
        }
        return;
    }
    out!("{sig}  {}", f.desc);
    if !f.aliases.is_empty() {
        out!("{} {}", paint(Role::Dim, "also:"), names(f.aliases, ", "));
    }
    if !f.detail.is_empty() {
        out!("{}", f.detail);
    }
    if !f.fields.is_empty() {
        out!(
            "
{}",
            paint(Role::Heading, "returns a map with")
        );
        let (kw, tw) = (f.fields.iter().map(|k| k.0.len()).max().unwrap_or(0), f.fields.iter().map(|k| k.1.len()).max().unwrap_or(0));
        for (key, ty, meaning) in f.fields {
            out!("  {key}{}  {}{}  {}", pad(key, kw), paint(Role::Type, ty), pad(ty, tw), ticked(meaning));
        }
        out!("");
    }
    show(f.examples.iter().map(|e| e.to_string()).collect());
    for ex in f.shown {
        out!("  {}", code(ex));
    }
    if f.flags.is_some() {
        out!("\n{}", paint(Role::Heading, "command"));
        for line in command(f) {
            out!("{line}");
        }
        out!("  {} {} {}", paint(Role::Dim, "see"), code("help shell"), paint(Role::Dim, "for more on shell commands"));
    }
    if f.name == "help" {
        out!("  {}  {}", code("help upper"), paint(Role::Dim, "(REPL shorthand)"));
        out!("  {}  {}", code("upper?"), paint(Role::Dim, "(REPL shorthand)"));
    }
    // Unboxed: the function is already in a box.
    if f.pretty {
        for (heading, lines) in rows(&m.guide.iter().filter(|s| s.0 == "pretty").copied().collect::<Vec<_>>(), false) {
            out!("\n{}", paint(Role::Heading, heading));
            for line in lines {
                out!("  {line}");
            }
        }
    }
    if !f.see.is_empty() {
        out!("{} {}", paint(Role::Dim, "see also:"), names(f.see, ", "));
    }
}

/// Search or type-page hits, grouped under their module (and help-page group, when that's what matched),
/// with whatever matched a search stem in reverse video.
fn listing(hits: Vec<(&Module, &Doc)>, stems: &[String]) {
    let mut last = String::new();
    for (m, f) in hits {
        let group = m.groups.iter().find(|g| g.1.contains(&f.name) && stems.iter().any(|s| s == g.0));
        let head = link("help", modules::path(m), mark(modules::path(m), stems, Role::ModulePath))
            + &group.map_or(String::new(), |g| paint(Role::Dim, " › ") + &mark(g.0, stems, Role::Dim));
        if head != last {
            out!("{head}");
            last = head;
        }
        out!("  {}\n    {}", sig(f, link("help", f.name, mark(f.name, stems, Role::Name))), mark(f.desc, stems, Role::Plain));
    }
}

/// `s` painted `color`, with every hit of the longest stem it contains in reverse video.
fn mark(s: &str, stems: &[String], role: Role) -> String {
    let lower = s.to_lowercase();
    // Byte offsets in `lower` only line up with `s` when lowercasing kept the length.
    let stem = stems.iter().filter(|t| lower.len() == s.len() && lower.contains(t.as_str())).max_by_key(|t| t.len());
    let Some(stem) = stem else { return paint(role, s) };
    let (mut out, mut at) = (String::new(), 0);
    for (i, _) in lower.match_indices(stem.as_str()) {
        out += &paint(role, &s[at..i]);
        let hit = &s[i..i + stem.len()];
        out += &if COLOR.get() { format!("{}{}{hit}{RESET}", role.code(), Role::SearchMatch.code()) } else { hit.to_string() };
        at = i + stem.len();
    }
    out + &paint(role, &s[at..])
}

/// `topic` lowercased, plus crude suffix stems ("sorting" → "sort").
fn stems(topic: &str) -> Vec<String> {
    let t = topic.to_lowercase();
    [Some(&t[..]), t.strip_suffix("ing"), t.strip_suffix("es"), t.strip_suffix('s')].into_iter().flatten().filter(|s| s.len() >= 2).map(String::from).collect()
}

/// Builtins whose name, description, module (`trig`) or help-page group (`spread`) mentions `topic`, or its stem.
// ponytail: crude suffix stemming ("sorting" → "sort"); real fuzzy matching if this misses too often.
fn search(topic: &str) -> Vec<(&'static Module, &'static Doc)> {
    let stems = stems(topic);
    let hit = |m: &Module, f: &Doc| {
        stems.iter().any(|s| s == m.name)
            || m.groups.iter().any(|g| stems.iter().any(|s| s == g.0) && g.1.contains(&f.name))
            || stems.iter().any(|s| f.name.contains(s.as_str()) || f.desc.to_lowercase().contains(s.as_str()) || f.detail.to_lowercase().contains(s.as_str()))
    };
    docs().filter(|(m, f)| hit(m, f)).collect()
}

/// Up to three function, module, unit or topic names a typo or two from `topic`, closest first.
fn near(topic: &str) -> Vec<&'static str> {
    let units = crate::modules::units::TABLE.iter().flat_map(|u| u.0.split(' '));
    let names = docs()
        .map(|(_, f)| f.name)
        .chain(ALL.iter().flat_map(|(path, m)| [path.as_str(), m.name]))
        .chain(["examples", "syntax", "advanced"])
        .chain(units)
        .chain(KEYWORDS.iter().flat_map(|k| k.0.iter().copied()));
    crate::error::near(topic, names)
}

/// A module's page: what it adds (types, operators, conversions, units...), its submodules, then its functions by group.
fn page(m: &Module) {
    out!("{}: {}", paint(Role::ModulePath, modules::path(m)), m.about);
    for (heading, text) in m.intro {
        if !heading.is_empty() {
            out!("\n{}", paint(Role::Heading, heading));
        }
        let indent = if heading.is_empty() { "" } else { "  " };
        for (i, para) in text.split("\n\n").enumerate() {
            let para = ticked(para);
            // The opening paragraph and each heading follow a blank line, a section's paragraphs one each.
            out!("{}{indent}{para}", if i == 0 && !heading.is_empty() { "" } else { "\n" });
        }
    }
    sections(m.guide);
    let kinds = units::kinds(m.units);
    if !kinds.is_empty() {
        let w = kinds.iter().map(|k| k.0.chars().count()).max().unwrap_or(0);
        let rows: Vec<_> = kinds
            .iter()
            .map(|(kind, rows)| {
                let units: Vec<_> = rows.iter().map(|r| r.0.split(' ').next().unwrap()).collect();
                format!("{}{}  {}", paint(Role::Dim, kind), pad(kind, w), names(&units, " "))
            })
            .collect();
        framed(&paint(Role::Heading, "units"), &rows);
    }
    if let Some(topic) = m.topic {
        topic(m.name);
    }
    if !m.children.is_empty() {
        let w = m.children.iter().map(|c| c.name.chars().count()).max().unwrap_or(0);
        let rows: Vec<_> =
            m.children.iter().map(|c| format!("{}{}  {}", link("help", c.name, paint(Role::Submodule, c.name)), pad(c.name, w), c.about)).collect();
        framed(&paint(Role::Heading, "submodules"), &rows);
    }
    if m.fns.is_empty() {
        return;
    }
    let all = [("functions", m.fns.iter().map(|f| f.name).collect::<Vec<_>>())];
    let groups: Vec<_> = m.groups.iter().map(|g| (g.0, g.1.to_vec())).collect();
    let groups = if groups.is_empty() { &all[..] } else { &groups[..] };
    // One box per group; a module without groups gets one "functions" box.
    for (name, fns) in groups {
        let lines = collected(|| {
            for f in m.fns.iter().filter(|f| fns.contains(&f.name)) {
                out!("");
                body(m, f, false);
            }
        });
        framed(&paint(if m.groups.is_empty() { Role::Heading } else { Role::Group }, name), &lines);
    }
}

/// Help sections, each in its own box of aligned `label  example  → result` rows; an empty example prints the label alone.
fn sections(guide: &[Section]) {
    for (heading, lines) in rows(guide, false) {
        framed(&paint(Role::Heading, heading), &lines);
    }
}

/// Each section's heading and its rows, aligned across the whole guide. With `shell`, examples are command lines
/// shown with what they desugar to, and not linked to run: `rm *.tmp` would.
fn rows(guide: &[Section], shell: bool) -> Vec<(&str, Vec<String>)> {
    let rows: Vec<_> = guide.iter().flat_map(|s| s.1).filter(|r| !r.1.is_empty()).collect();
    let lw = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let ew = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
    let row = |&(label, ex): &(&str, &str)| {
        if ex.is_empty() {
            return paint(Role::Dim, label);
        }
        let (plain, colored) = if shell {
            let s = crate::parser::desugar(ex).unwrap_or_else(|e| format!("error: {}", e.msg));
            (s.clone(), lit(&s))
        } else {
            result(ex)
        };
        let label = format!("{}{}", paint(Role::Dim, label), pad(label, lw));
        let shown = if shell { lit(ex) } else { code(ex) };
        if plain == ex { format!("{label}  {shown}") } else { format!("{label}  {shown}{}  {} {}", pad(ex, ew), paint(Role::Dim, "→"), below(&colored)) }
    };
    guide.iter().map(|(heading, rows)| (*heading, rows.iter().map(row).collect())).collect()
}

/// Every command's name and description, each followed by its `command` lines.
fn commands() -> Vec<String> {
    let mut lines = vec![];
    for f in modules().flat_map(|m| m.fns).filter(|f| f.flags.is_some()) {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("{}  {}", link("help", f.name, paint(Role::Name, f.name)), f.desc));
        lines.extend(command(f));
    }
    lines
}

/// A command's flags and what each does, then its example line and the call it desugars to.
fn command(f: &Doc) -> Vec<String> {
    let call = crate::parser::desugar(f.command).unwrap_or_else(|e| format!("error: {}", e.msg));
    let names: Vec<_> = f.flags.unwrap_or(&[]).iter().map(|(c, n, what)| (format!("-{c}, --{}", n.replace('_', "-")), what)).collect();
    let w = names.iter().map(|n| n.0.len()).max().unwrap_or(0);
    let flags = names.iter().map(|(n, what)| format!("  {}{}  {what}", paint(Role::Flag, n), pad(n, w)));
    flags.chain([format!("  {}  {} {}", lit(f.command), paint(Role::Dim, "→"), lit(&call))]).collect()
}

/// `s` highlighted as zil or a command line, when coloring.
fn lit(s: &str) -> String {
    if COLOR.get() { crate::ansi::highlight(s) } else { s.to_string() }
}

/// `lines` in a dim box headed by `label`, preceded by a blank line. Lines are wrapped to fit inside first, so the
/// terminal-width pass in `capture` leaves the box whole; code is never broken, so when it still doesn't fit, the
/// lines go out unboxed under the label instead.
fn framed(label: &str, lines: &[String]) {
    // Rows indented for an unboxed page drop that indent: the box sets them apart now.
    let indented = lines.iter().flat_map(|l| l.split('\n')).all(|l| l.is_empty() || l.starts_with("  "));
    let lines: Vec<_> = lines.iter().flat_map(|l| l.split('\n')).map(|l| if indented { l.get(2..).unwrap_or("") } else { l }.to_string()).collect();
    let inside = TERM.get().map(|t| t.1.saturating_sub(4));
    let body: Vec<_> = lines.iter().map(|l| inside.map_or(l.to_string(), |w| wrap(l, w))).collect();
    let width = crate::ansi::width;
    let fits = inside.is_none_or(|w| body.iter().flat_map(|l| l.lines()).all(|l| width(l) <= w) && width(label) + 3 <= w);
    if !fits {
        out!("\n{label}");
        for line in &lines {
            out!("  {line}");
        }
        return;
    }
    let (dim, reset) = if COLOR.get() { (Role::Dim.code(), RESET) } else { ("", "") };
    out!("\n{}", crate::ansi::boxed(label, &body.join("\n"), dim, reset));
}

/// A random example or recipe from `help("examples")` and `help("advanced")`, boxed under its label.
pub fn tip(color: bool) -> String {
    let examples = HIGHLIGHTS.iter().chain(modules().flat_map(|m| m.examples)).flat_map(|s| s.1.iter().copied());
    let recipes = ADVANCED.iter().chain([&ADVANCED_UNRUN]).flat_map(|t| t.1).map(|&(what, _, code)| (what, code));
    // The highlights end on a syntax note with no code.
    let tips: Vec<_> = examples.chain(recipes).filter(|(_, code)| !code.is_empty()).collect();
    let (what, src) = tips[crate::host::rng(|r| r.usize(..tips.len()))];
    capture(color, || {
        framed(&format!("tip, {what}"), &src.lines().map(|l| snippet(l, src)).collect::<Vec<_>>());
        true
    })
    .unwrap_or_else(|e| e)
}

/// What `f` writes to the help buffer, taken back out as lines, without blank lines at either end.
fn collected(f: impl FnOnce()) -> Vec<String> {
    let before = OUT.take();
    f();
    OUT.replace(before).trim_matches('\n').lines().map(String::from).collect()
}

/// Print each example with its live result, arrows aligned.
pub fn show(examples: Vec<String>) {
    let w = examples.iter().map(|e| e.chars().count()).max().unwrap_or(0);
    for ex in &examples {
        out!("  {}{}  {} {}", code(ex), pad(ex, w), paint(Role::Dim, "→"), below(&result(ex).1));
    }
}

pub fn eval(src: &str) -> String {
    result(src).0
}

/// A multi-line result on its own indented lines, so art keeps its shape.
fn below(r: &str) -> String {
    if r.contains('\n') { r.lines().map(|l| format!("\n      {l}")).collect() } else { r.to_string() }
}

/// `src`'s result, plain and colored by type like a REPL result.
/// Text art (multi-line or colored strings) comes back bare, as the REPL prints it.
fn result(src: &str) -> (String, String) {
    match crate::run(&mut Interp::new(), src) {
        Ok(crate::value::Value::Str(s)) if s.contains(['\n', '\x1b']) => {
            let plain = crate::ansi::strip_ansi(&s);
            let colored = if COLOR.get() { s.to_string() } else { plain.clone() };
            (plain, colored)
        }
        Ok(v) => {
            let s = format!("{v:?}");
            (s.clone(), paint(crate::ansi::tint(&v), &s))
        }
        Err(e) => {
            let s = format!("error: {}", e.msg);
            (s.clone(), s)
        }
    }
}

/// A signature with each form's function name shown as `name` and parameter types colored.
fn sig(f: &Doc, name: String) -> String {
    let types = regex::Regex::new(r"(: |-> )([^,) ]+)").unwrap();
    let form = |form: &str| match form.strip_prefix(f.name) {
        Some(rest) => format!("{}{}", name, types.replace_all(rest, |c: &regex::Captures| format!("{}{}", &c[1], paint(Role::Type, &c[2])))),
        None => form.to_string(),
    };
    f.sig.split(" / ").map(form).collect::<Vec<_>>().join(" / ")
}

/// Function or unit names, colored and joined.
pub fn names(names: &[&str], sep: &str) -> String {
    names.iter().map(|n| link("help", n, paint(Role::Name, n))).collect::<Vec<_>>().join(sep)
}

pub fn paint(role: Role, s: &str) -> String {
    if role == Role::Plain || !COLOR.get() { s.to_string() } else { format!("{}{s}{RESET}", role.code()) }
}

/// Code, syntax-highlighted when color is on.
pub fn code(s: &str) -> String {
    snippet(s, s)
}

/// A `line` of the code `src`, syntax-highlighted when color is on; clicked, it runs all of `src`.
fn snippet(line: &str, src: &str) -> String {
    link("run", src, if COLOR.get() { crate::ansi::highlight(line) } else { line.to_string() })
}

/// Prose with its backticked spans highlighted as code, not linked: they're often fragments like `.text`.
fn ticked(s: &str) -> String {
    s.split('`').enumerate().map(|(i, p)| if i % 2 == 1 && COLOR.get() { crate::ansi::highlight(p) } else { p.to_string() }).collect()
}

/// Spaces padding uncolored `s` to width `w`; `{:<w$}` would count escape codes.
pub fn pad(s: &str, w: usize) -> String {
    " ".repeat(w.saturating_sub(s.chars().count()))
}

/// Every example in the module tree, for seeding fuzzers.
pub fn all_examples() -> Vec<&'static str> {
    let fns = modules().flat_map(|m| m.fns.iter().flat_map(|f| f.examples.iter().chain(f.shown)));
    let sections = modules().flat_map(|m| m.examples.iter().chain(m.guide).flat_map(|(_, rows)| rows.iter().map(|(_, ex)| ex)));
    fns.chain(sections).copied().filter(|ex| !ex.is_empty()).collect()
}

/// `help("examples")`'s sections as JSON `[[top-level module, title, [[label, code], ...]], ...]`, highlights first.
pub fn examples_json() -> String {
    let highlights = HIGHLIGHTS.iter().map(|s| ("start", s));
    let sections = highlights.chain(modules().flat_map(|m| m.examples.iter().map(|s| (modules::path(m).split('.').next().unwrap(), s))));
    let rows: Vec<_> = sections.map(|(module, (title, rows))| (module, title, rows.iter().filter(|(_, code)| !code.is_empty()).collect::<Vec<_>>())).collect();
    serde_json::to_string(&rows).unwrap()
}

#[cfg(any(target_arch = "wasm32", test))]
/// The tour as JSON `[[chapter, [[text, code], ...]], ...]`.
pub fn tour_json() -> String {
    serde_json::to_string(crate::guide::TOUR).unwrap()
}

/// The vocabulary, for editor completion, hover docs and highlighting, as JSON:
/// `{fns: [[name, sig, desc, module], ...], units: [[names, desc], ...], consts: [name, ...], modules: [[path, about], ...],
/// keywords: [[keyword, form, desc], ...]}`.
pub fn vocab_json() -> String {
    let fns: Vec<_> = modules().flat_map(|m| m.fns.iter().map(move |f| (f.name, f.sig, f.desc, modules::path(m)))).collect();
    let units: Vec<_> = units::rows().map(|r| (r.0, r.4)).collect();
    let consts: Vec<_> = modules().flat_map(|m| m.consts.iter().map(|c| c.0).chain(m.quantities.iter().map(|q| q.0))).collect();
    let mods: Vec<_> = ALL.iter().map(|(p, m)| (p, m.about)).collect();
    let keywords: Vec<_> = KEYWORDS.iter().flat_map(|k| k.0.iter().map(move |w| (w, k.1, k.2))).collect();
    serde_json::json!({ "fns": fns, "units": units, "consts": consts, "modules": mods, "keywords": keywords }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide::INTRO;
    use crate::modules::units::DIMS;

    #[test]
    fn docs_examples_run() {
        let fns: Vec<_> = modules().flat_map(|m| m.fns).collect();
        let guide =
            modules().flat_map(|m| m.guide.iter().chain(m.examples)).chain(HIGHLIGHTS).chain(SYNTAX).flat_map(|s| s.1).map(|r| &r.1).filter(|e| !e.is_empty());
        let recipes = ADVANCED.iter().flat_map(|t| t.1).map(|r| &r.2).chain(KEYWORDS.iter().flat_map(|k| k.3));
        let intro = INTRO.iter().filter(|s| !s.2).flat_map(|s| s.3).map(|r| &r.1);
        for ex in fns.iter().flat_map(|f| f.examples.iter()).chain(guide).chain(recipes).chain(intro) {
            assert!(!eval(ex).starts_with("error:"), "{ex}: {}", eval(ex));
        }
        let found = |t| help(Some(t), false).is_ok();
        for (path, m) in ALL.iter() {
            assert!(found(path));
            assert!(found(m.name));
        }
        for (kind, _) in DIMS {
            assert!(found(kind));
        }
        for row in units::rows() {
            let name = row.0.split(' ').next().unwrap();
            assert!(!help(Some(name), false).unwrap().contains("error:"), "help({name:?})");
        }
        assert!(help(Some("upper"), true).unwrap().contains(Role::Name.code()));
        assert!(!help(Some("km"), false).unwrap().contains('\x1b'));
        // Shell examples would touch files if run, so they only have to desugar.
        let intro = INTRO.iter().filter(|s| s.2).flat_map(|s| s.3).map(|r| r.1);
        for ex in SHELL.iter().chain(SHELL_INTRO.iter().map(|p| &p.1)).flat_map(|s| s.1).map(|r| r.1).filter(|e| !e.is_empty()).chain(intro) {
            assert!(crate::parser::desugar(ex).is_ok(), "{ex}");
        }
        for f in modules().flat_map(|m| m.fns).filter(|f| f.flags.is_some()) {
            assert!(f.command.split(' ').next().is_some_and(|n| f.is(n)) && crate::parser::desugar(f.command).is_ok(), "{}: {}", f.name, f.command);
        }
        assert!(found("syntax"));
        assert!(found("advanced"));
        for (_, _, src) in ADVANCED_UNRUN.1 {
            assert!(crate::parser::parse(src).is_ok(), "{src}");
        }
        for (title, uses, _) in ADVANCED.iter().flat_map(|t| t.1).chain(ADVANCED_UNRUN.1) {
            for path in uses.split(" · ") {
                assert!(ALL.iter().any(|(p, _)| p == path), "{title}: no module {path}");
            }
        }
        assert!(found("quantity"));
        assert!(!found("nope"));
        assert!(help(Some("upper"), false).unwrap().contains("upper(s: str)"));
    }

    #[test]
    fn shadowed_topics_show_every_page() {
        let table = help(Some("table"), false).unwrap();
        assert!(table.contains("table(") && table.contains("functions taking a table first"), "{table}");
        let units = help(Some("units"), false).unwrap();
        assert_eq!(units.matches(r#"help("length") or help("km")"#).count(), 1, "{units}");
        let inch = help(Some("in"), false).unwrap();
        assert!(inch.contains("keyword for, in") && inch.contains("inch"), "{inch}");
    }

    #[test]
    fn keywords_have_pages() {
        for k in ["fn", "if", "else", "while", "for", "in", "to", "of", "mod", "return", "break", "continue", "true", "false", "nil"] {
            assert!(help(Some(k), false).is_ok_and(|p| p.contains("keyword ")), "{k}");
        }
        let syntax = help(Some("syntax"), false).unwrap();
        assert!(syntax.contains("break, continue"), "{syntax}");
        assert!(search_page("loop", false).unwrap().contains("while cond"));
    }

    #[test]
    fn examples_match_return_types() {
        use crate::ast::ExprKind::{Call, Field, Ident};
        for f in modules().flat_map(|m| m.fns) {
            let ret: Vec<_> = f.sig.split(" / ").filter_map(|g| g.rsplit_once(" -> ")).flat_map(|r| r.1.split('|')).collect();
            // Only examples whose outermost call is f, so `clipart().len` isn't held to clipart's type.
            for ex in f.examples {
                let ast = crate::parser::parse(ex).unwrap();
                let callee = match ast.as_slice() {
                    [e] => match &e.kind {
                        Call(c, _) => &c.kind,
                        k => k,
                    },
                    _ => continue,
                };
                if !matches!(callee, Ident(n) | Field(_, n) if n == f.name) {
                    continue;
                }
                let v = crate::run(&mut Interp::new(), ex).unwrap();
                assert!(ret.iter().any(|t| crate::signature::is_type(&v, t)), "{ex} is {}, not {}", v.type_name(), ret.join("|"));
            }
        }
    }

    #[test]
    fn search_finds_by_description_group_and_stem() {
        let names = |t| search(t).iter().map(|h| h.1.name).collect::<Vec<_>>();
        assert!(names("sorting").contains(&"sort"));
        assert!(names("Uppercase").contains(&"upper"));
        assert!(names("trig").contains(&"atan2"));
        assert!(names("hash").contains(&"md5"));
        assert!(names("zzz").is_empty());
    }

    #[test]
    fn search_marks_matches_and_groups() {
        let sorting = help(Some("sorting"), true).unwrap();
        assert!(sorting.contains(&format!("{}sort", Role::SearchMatch.code())), "{sorting}");
        let spread = crate::ansi::strip_ansi(&help(Some("spread"), false).unwrap());
        assert!(spread.starts_with("╭─ search \"spread\": ") && spread.contains("math.stats › spread"), "{spread}");
    }

    #[test]
    fn boxes_survive_wrapping() {
        for topic in ["examples", "advanced", "syntax", "text", "upper", "sorting"] {
            // Some boxes fit in 100 columns and some fall back to unboxed.
            let text = live(Some((false, 100)), || help(Some(topic), false)).unwrap();
            assert!(text.contains('╭'), "{topic}: no boxes at all");
            let mut open = false;
            for line in text.lines() {
                open = (open || line.starts_with('╭')) && !line.starts_with('╰');
                if open && !line.starts_with('╭') {
                    assert!(line.starts_with('│') && line.ends_with('│'), "{topic}: broken box row {line:?}");
                    assert!(line.chars().count() <= 100, "{topic}: box wider than the terminal: {line:?}");
                }
            }
        }
    }

    #[test]
    fn wrap_keeps_columns() {
        assert_eq!(wrap("  ab  one two three four five six seven", 30), "  ab  one two three four five\n      six seven");
        // Too little room past the column: it moves under the indent + 4, and code before it is never split.
        assert_eq!(wrap("  a_very_long_signature(x: str)  one two three", 40), "  a_very_long_signature(x: str)\n      one two three");
        assert_eq!(wrap("  f(x)  → aaa bbb ccc", 14), "  f(x)\n      → aaa\n        bbb\n        ccc");
        // Colors don't count toward the width.
        let red = |s: &str| format!("\x1b[31m{s}\x1b[0m");
        assert_eq!(
            wrap(&format!("  {}  one two three four five six seven", red("ab")), 30),
            format!("  {}  one two three four five\n      six seven", red("ab"))
        );
        assert_eq!(wrap("short", 30), "short");
    }

    #[test]
    fn links_change_nothing_visible() {
        for topic in [None, Some("upper"), Some("text"), Some("sorting"), Some("advanced")] {
            let linked = linked(true, || live(Some((true, 100)), || help(topic, true))).unwrap();
            let plain = live(Some((true, 100)), || help(topic, true)).unwrap();
            assert!(linked.contains("\x1b]8;;zil:"), "{topic:?}: no links");
            assert_eq!(crate::ansi::strip_ansi(&linked), crate::ansi::strip_ansi(&plain), "{topic:?}");
        }
        let upper = linked(true, || help(Some("upper"), true)).unwrap();
        assert!(upper.contains("zil:help/upper") && upper.contains("zil:run/%22hello%22.upper"), "{upper}");
    }

    #[test]
    fn near_suggests_typos() {
        assert_eq!(near("uper")[0], "upper");
        assert!(near("syntx").contains(&"syntax"));
        assert!(near("qqqqqq").is_empty());
    }
}
