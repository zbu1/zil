//! The reference as Starlight MDX pages for `zil --docs <dir>`, rendered from the same module tree as `help`, one page per module,
//! plus `sidebar.json`, which `docs/astro.config.mjs` reads. A build step for the site, not a user-facing command.
//! Example results are evaluated live, so the pages are build output: CI regenerates them on deploy.

#![allow(clippy::unwrap_used)] // every unwrap here is a write into a String, which never fails

use super::eval;
use crate::guide::{ADVANCED, ADVANCED_UNRUN, HIGHLIGHTS, INTRO, INTRO_LEAD, SHELL, SHELL_INTRO, SYNTAX};
use crate::modules::{self, ALL, Doc, Module, Section, TREE, modules, units};
use std::fmt::Write;

/// Prefix of the line holding an example's result; the zil grammar and `runnable.js` know it.
const RESULT: &str = "# → ";

/// (file name, contents) for every page; `date` goes in the note on generated results.
pub fn pages(date: jiff::civil::Date) -> Vec<(String, String)> {
    // Under docs/, so the reference is served at /docs/ and the landing page (src/pages/index.astro) has the root.
    let mut out = vec![
        ("docs/index.mdx".to_string(), index(date)),
        ("docs/intro.mdx".into(), intro(date)),
        ("docs/syntax.mdx".into(), syntax(date)),
        ("docs/advanced.mdx".into(), advanced(date)),
    ];
    for (path, m) in ALL.iter() {
        out.push((format!("docs/{}.mdx", path.replace('.', "/")), page(path, m, date)));
    }
    out.push(("sidebar.json".into(), sidebar()));
    out
}

/// A module's page URL: `math.trig` is `/docs/math/trig/`.
fn url(path: &str) -> String {
    format!("/docs/{}/", path.replace('.', "/"))
}

/// Frontmatter, the components pages use, and the note on generated results at the end.
fn layout(title: &str, about: &str, extra: &str, body: &str, date: jiff::civil::Date) -> String {
    format!(
        "---\ntitle: {}\ndescription: {}\n{extra}---\nimport {{ Aside, Badge, CardGrid, LinkCard }} from \"@astrojs/starlight/components\";\n\n{body}\n\
         <p class=\"generated\">Example results generated on {date}. Ones using `now`, `today` or randomness will differ when you run them.</p>\n",
        json(title),
        json(&about.replace('`', ""))
    )
}

fn index(date: jiff::civil::Date) -> String {
    let mut s = String::from(
        "<Aside type=\"tip\">Every example here runs in your browser. Edit one and press **Run**, or <kbd>Ctrl</kbd> <kbd>↵</kbd>. \
         Press <kbd>Ctrl</kbd> <kbd>K</kbd> to search.</Aside>\n\n",
    );
    sections(&mut s, HIGHLIGHTS);
    s.push_str("<CardGrid>\n");
    s.push_str("<LinkCard title=\"Introduction\" href=\"/docs/intro/\" description=\"Where to start, depending on what you're coming from\" />\n");
    s.push_str("<LinkCard title=\"Syntax\" href=\"/docs/syntax/\" description=\"Values, calls, pipes, lambdas and statements on one page\" />\n");
    s.push_str("<LinkCard title=\"Advanced examples\" href=\"/docs/advanced/\" description=\"Longer recipes that combine several modules\" />\n");
    s.push_str("</CardGrid>\n\n## Modules\n\n<CardGrid>\n");
    for (path, m) in ALL.iter().filter(|(p, _)| !p.contains('.')) {
        card(&mut s, path, m);
    }
    s.push_str("</CardGrid>\n");
    layout("Overview", "A terse calculator and scripting language for units, dates and much more.", "", &s, date)
}

/// A link to a module, with how many functions it and its submodules have, and the submodules' names.
fn card(s: &mut String, path: &str, m: &Module) {
    fn count(m: &Module) -> usize {
        m.fns.len() + m.children.iter().map(count).sum::<usize>()
    }
    let mut about = format!("{} · {} functions", m.about.replace('`', ""), count(m));
    if !m.children.is_empty() {
        write!(about, " · {}", m.children.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")).unwrap();
    }
    writeln!(s, "<LinkCard title={} href=\"{}\" description={} />", jsx(m.name), url(path), jsx(&about)).unwrap();
}

fn intro(date: jiff::civil::Date) -> String {
    // Backticked spans stay code; the rest is escaped.
    let text = |t: &str| t.split('`').enumerate().map(|(i, p)| if i % 2 == 1 { format!("`{p}`") } else { prose(p) }).collect::<String>();
    let mut s = format!("{}\n\n", text(INTRO_LEAD));
    for (heading, body, shell, rows) in INTRO {
        writeln!(s, "## {}\n\n{}\n", prose(heading), text(body)).unwrap();
        if rows.is_empty() {
            continue;
        }
        let mut code = String::new();
        for (label, ex) in *rows {
            writeln!(code, "# {label}").unwrap();
            if *shell {
                writeln!(code, "{ex}\n{RESULT}{}", crate::parser::desugar(ex).unwrap_or_else(|e| format!("error: {}", e.msg))).unwrap();
            } else {
                example(&mut code, ex);
            }
        }
        // Command lines aren't runnable blocks: `rm {old}` would run.
        if *shell {
            writeln!(s, "````text\n{}\n````\n", code.trim_end()).unwrap();
        } else {
            block(&mut s, &code);
        }
    }
    layout("Introduction", "Where to start, depending on what you're coming from.", "sidebar:\n  order: 0\n", &s, date)
}

fn syntax(date: jiff::civil::Date) -> String {
    let mut s = String::from("The language at a glance. `help(\"syntax\")` shows the same in the REPL.\n\n");
    // Its last section points here.
    let syntax: Vec<_> = SYNTAX.iter().filter(|(heading, _)| *heading != "more").copied().collect();
    sections(&mut s, &syntax);
    s.push_str("## Command mode\n\n");
    for (para, (title, rows)) in SHELL_INTRO {
        writeln!(s, "{}\n", inline(para)).unwrap();
        if !rows.is_empty() {
            command_lines(&mut s, &format!(" title={}", json(title)), rows);
        }
    }
    s.push_str("`help(\"shell\")` has the same in the REPL.\n\n### Commands\n\n");
    for f in modules().flat_map(|m| m.fns).filter(|f| f.flags.is_some()) {
        writeln!(s, "[`{}`]({}#{}) {}\n", f.name, home(f.name), anchor(f.name), inline(f.desc)).unwrap();
        command(&mut s, f);
    }
    for (heading, rows) in SHELL {
        writeln!(s, "### {}\n", prose(heading)).unwrap();
        command_lines(&mut s, "", rows);
    }
    layout("Syntax", "Values, calls, pipes, lambdas and statements on one page.", "sidebar:\n  order: 1\n", &s, date)
}

/// Recipes combining several modules: a heading each, the modules it uses, then the script with its result.
fn advanced(date: jiff::civil::Date) -> String {
    let mut s = String::from("Longer recipes that combine several modules.\n\n");
    let unrun = std::iter::once((ADVANCED_UNRUN, false));
    for ((theme, recipes), run) in ADVANCED.iter().map(|t| (*t, true)).chain(unrun) {
        writeln!(s, "## {}\n", prose(theme)).unwrap();
        for (title, uses, src) in recipes {
            writeln!(s, "### {}\n\n<Badge text={} />\n", prose(title), jsx(uses)).unwrap();
            let mut code = String::new();
            if run {
                example(&mut code, src);
            } else {
                writeln!(code, "{src}").unwrap();
            }
            block(&mut s, &code);
        }
    }
    layout("Advanced examples", "Longer recipes that combine several modules.", "", &s, date)
}

fn page(path: &str, m: &Module, date: jiff::civil::Date) -> String {
    let mut s = String::new();
    writeln!(s, "{}\n", inline(m.about)).unwrap();
    for (heading, text) in m.intro {
        if !heading.is_empty() {
            writeln!(s, "### {}\n", prose(heading)).unwrap();
        }
        for para in text.split("\n\n") {
            writeln!(s, "{}\n", inline(para)).unwrap();
        }
    }
    if !m.children.is_empty() {
        s.push_str("<CardGrid>\n");
        for c in m.children {
            card(&mut s, &format!("{path}.{}", c.name), c);
        }
        s.push_str("</CardGrid>\n\n");
    }
    if !m.fns.is_empty() {
        for f in m.fns {
            writeln!(s, "- [`{0}`](#{1}) {2}", f.name, anchor(f.name), inline(f.desc)).unwrap();
        }
        s.push('\n');
    }
    sections(&mut s, m.guide);
    let kinds = units::kinds(m.units);
    if !kinds.is_empty() {
        s.push_str("## Units\n\n| kind | units |\n| --- | --- |\n");
        for (kind, rows) in kinds {
            let names: Vec<_> = rows.iter().map(|r| prose(&r.0.replace(' ', ", "))).collect();
            writeln!(s, "| {kind} | {} |", names.join("; ")).unwrap();
        }
        s.push('\n');
    }
    if !m.fns.is_empty() {
        s.push_str("## Functions\n\n");
        for f in m.fns {
            function(&mut s, f);
        }
    }
    if !m.examples.is_empty() {
        s.push_str("## More examples\n\n");
        sections(&mut s, m.examples);
    }
    layout(path, m.about, "", &s, date)
}

fn function(s: &mut String, f: &Doc) {
    writeln!(s, "### {}\n\n<code class=\"sig\">{}</code>\n\n{}\n", f.name, jsx_text(f.sig), inline(f.desc)).unwrap();
    if !f.aliases.is_empty() {
        writeln!(s, "Also called {}.\n", f.aliases.iter().map(|a| format!("`{a}`")).collect::<Vec<_>>().join(", ")).unwrap();
    }
    if !f.detail.is_empty() {
        writeln!(s, "{}\n", inline(f.detail)).unwrap();
    }
    if !f.fields.is_empty() {
        s.push_str("Returns a map with:\n\n| key | type | meaning |\n|---|---|---|\n");
        for (key, ty, meaning) in f.fields {
            writeln!(s, "| `{key}` | {} | {} |", jsx_text(ty), inline(meaning)).unwrap();
        }
        s.push('\n');
    }
    if !f.examples.is_empty() || !f.shown.is_empty() {
        let mut code = String::new();
        for ex in f.examples {
            example(&mut code, ex);
        }
        for ex in f.shown {
            writeln!(code, "{ex}").unwrap();
        }
        block(s, &code);
    }
    if f.flags.is_some() {
        s.push_str("As a command:\n\n");
        command(s, f);
    }
    if !f.see.is_empty() {
        let links: Vec<_> = f.see.iter().map(|n| format!("[`{n}`]({}#{})", home(n), anchor(n))).collect();
        writeln!(s, "See also {}\n", links.join(", ")).unwrap();
    }
}

/// A command's flags and what each does, then its example line and the call it desugars to. Not a runnable block: `rm -rf build` would run.
fn command(s: &mut String, f: &Doc) {
    let flags = f.flags.unwrap_or(&[]);
    for (c, n, what) in flags {
        writeln!(s, "- `-{c}`, `--{}` {}", n.replace('_', "-"), inline(what)).unwrap();
    }
    if !flags.is_empty() {
        s.push('\n');
    }
    let call = crate::parser::desugar(f.command).unwrap_or_else(|e| format!("error: {}", e.msg));
    writeln!(s, "````text\n{}\n{RESULT}{call}\n````\n", f.command).unwrap();
}

/// Labelled command lines with the calls they desugar to, as one block with `meta` after its language. Not runnable:
/// `rm *.tmp` would run.
fn command_lines(s: &mut String, meta: &str, rows: &[(&str, &str)]) {
    writeln!(s, "````text{meta}").unwrap();
    for (label, ex) in rows {
        writeln!(s, "# {label}").unwrap();
        if !ex.is_empty() {
            writeln!(s, "{ex}\n{RESULT}{}", crate::parser::desugar(ex).unwrap_or_else(|e| format!("error: {}", e.msg))).unwrap();
        }
    }
    s.push_str("````\n\n");
}

/// The URL of the page documenting function `name`.
fn home(name: &str) -> String {
    modules().find(|m| m.fns.iter().any(|g| g.name == name)).map_or(String::new(), |m| url(modules::path(m)))
}

/// Each section as a heading and a code block of `# label`, example, result.
fn sections(s: &mut String, guide: &[Section]) {
    for (heading, rows) in guide {
        writeln!(s, "### {}\n", prose(heading)).unwrap();
        let mut code = String::new();
        for (label, ex) in *rows {
            writeln!(code, "# {label}").unwrap();
            if !ex.is_empty() {
                example(&mut code, ex);
            }
        }
        block(s, &code);
    }
}

fn example(s: &mut String, ex: &str) {
    writeln!(s, "{ex}").unwrap();
    let v = eval(ex);
    if v != ex {
        writeln!(s, "{RESULT}{}", v.replace('\n', &format!("\n{RESULT}"))).unwrap();
    }
}

/// A runnable code block; `runnable.js` adds the Run button.
fn block(s: &mut String, code: &str) {
    writeln!(s, "````zil\n{}\n````\n", code.trim_end()).unwrap();
}

/// The sidebar: the guides, then every module, a module with submodules as a collapsed group headed by its own page.
fn sidebar() -> String {
    fn items(ms: &[Module], prefix: &str) -> Vec<String> {
        ms.iter()
            .map(|m| {
                let path = if prefix.is_empty() { m.name.to_string() } else { format!("{prefix}.{}", m.name) };
                let slug = json(&format!("docs/{}", path.replace('.', "/")));
                if m.children.is_empty() {
                    return format!("{{\"slug\":{slug},\"label\":{}}}", json(m.name));
                }
                let mut inner = vec![format!("{{\"slug\":{slug},\"label\":\"overview\"}}")];
                inner.extend(items(m.children, &path));
                format!("{{\"label\":{},\"collapsed\":true,\"items\":[{}]}}", json(m.name), inner.join(","))
            })
            .collect()
    }
    let guides = r#"{"label":"Overview","link":"/docs/"},{"slug":"docs/intro"},{"slug":"docs/syntax"},{"slug":"docs/advanced"}"#;
    format!("[{guides},{{\"label\":\"Modules\",\"items\":[\n{}\n]}}]\n", items(TREE, "").join(",\n"))
}

fn json(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSX attribute holding `s` as a JS string, so quotes and braces need no thought.
fn jsx(s: &str) -> String {
    format!("{{{}}}", json(s))
}

/// Text inside a JSX element: braces and angle brackets as entities.
fn jsx_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('{', "&#123;").replace('}', "&#125;")
}

/// Plain text as MDX: markdown and JSX punctuation backslash-escaped.
fn prose(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\`*_[]<>{}|#".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Prose with `code` in backticks kept as code spans, which MDX leaves alone.
fn inline(s: &str) -> String {
    s.split('`').enumerate().map(|(i, part)| if i % 2 == 1 { format!("`{part}`") } else { prose(part) }).collect()
}

/// The id Starlight gives a heading (github-slugger): lowercase, spaces to `-`, other punctuation dropped.
fn anchor(s: &str) -> String {
    s.to_lowercase().chars().filter_map(|c| if c == ' ' { Some('-') } else { (c.is_alphanumeric() || c == '-' || c == '_').then_some(c) }).collect()
}
