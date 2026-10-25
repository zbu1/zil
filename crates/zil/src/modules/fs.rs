//! Files: reading and writing, directories as tables, path pieces, JSON and CSV.

use crate::host::Host;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc, flags, units};
use crate::value::{self, Table, Value};
use indexmap::IndexMap;
use regex::Regex;
use std::path::{Component, Path, PathBuf};

pub const MODULE: Module = Module {
    name: "fs",
    about: "read and write files, manage folders, take paths apart, and convert JSON and CSV",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Files and folders: reading and writing them, moving them around, and taking paths apart. Every function here reads \
a path as relative to the current directory unless it starts from the root."),
        ("writing", "\
`ls`, `rm`, `cp` and the other file functions also work as shell-style commands, like `rm -r build`, which \
help(\"shell\") explains. In an ordinary call, give the flags as a map: `rm(\"build\", {r: true})`."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("fs", &[
            ("read JSON and add up a list in it", r#""\{\"a\": [1, 2]}".from_json.a.sum"#),
            ("read CSV and add up a column", "\"name,n\\nx,1\\ny,2\".from_csv.n.sum"),
            ("a file name without, and then only, its extension", r#"["a/b.tar.gz".stem, "a/b.tar.gz".ext]"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("read and write", &["read_file", "read_bytes", "cat", "write_file", "append_file"]),
        ("look", &["exists", "is_dir", "ls", "glob", "file_size", "mtime"]),
        ("change", &["rm", "mv", "cp", "mkdir"]),
        ("paths", &["path_join", "basename", "dirname", "ext", "stem", "abspath", "cwd", "pwd", "cd"]),
        ("data", &["from_json", "to_json", "from_csv", "to_csv"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("read_file", "read_file(path: str) -> str", "a file's contents, as a string", &[], &["write_file", "lines", "from_csv"]).shown(&[r#"read_file("notes.txt").lines.len"#, r#"read_file("data.json").from_json"#]),
    doc("read_bytes", "read_bytes(path: str) -> bytes", "a file's contents, as bytes, for files that aren't text", &[], &["read_file", "bytes", "hexdump"]).shown(&[r#"read_bytes("a.out")[..4] == b"\x7fELF""#]),
    doc("cat", "cat(path: str, ...) -> str", "the contents of one or more files, joined into one string", &[], &["read_file", "grep", "wc"]).shown(&[r#"cat("notes.txt").lines.len"#, "cat a.txt b.txt"]),
    doc("write_file", "write_file(path: str, v: any) -> nil", "write v to a file, replacing what was there", &[], &["read_file", "append_file"]).shown(&[r#"write_file("out.txt", [1, 2, 3])"#, r#"write_file("files.csv", ls())"#])
        .detail("Values are written as text, a list one item per line and a table as CSV, while bytes are written exactly"),
    doc("append_file", "append_file(path: str, v: any) -> nil", "add v to the end of a file, creating it if needed", &[], &["write_file"]).shown(&[r#"append_file("log.txt", ["started {now}"])"#])
        .detail("A list goes one item per line"),
    doc("exists", "exists(path: str) -> bool", "whether a file or directory exists", &[], &["is_dir"]).shown(&[r#"exists("Cargo.toml")"#]),
    doc("is_dir", "is_dir(path: str) -> bool", "whether path is a directory (folder)", &[], &["exists"]).shown(&[r#"is_dir("src")"#]),
    doc("ls", "ls(dir?: str) -> table", "what's in a directory (the current one by default), as a table of name, type, size and modified", &[], &["glob", "sort_by"]).shown(&["ls()", r#"ls("src").where(|f| f.type == "file").name"#, "ls -a"])
        .detail("Names starting with a dot are hidden unless you add -a"),
    doc("glob", "glob(pat: str) -> table", "every path that matches the pattern pat, as a table like ls gives", &[], &["ls"]).shown(&[r#"glob("**/*.rs").size.sum"#, r#"glob("*.csv").name"#])
        .detail("* matches any part of a name, ? any one character, and ** any number of folders, so **/*.rs finds .rs files at any depth"),
    doc("file_size", "file_size(path: str) -> quantity", "a file's size, as a quantity in bytes", &[], &["mtime", "human_bytes"]).shown(&[r#"file_size("Cargo.lock") to KiB"#]),
    doc("mtime", "mtime(path: str) -> date", "when a file was last changed, as a date", &[], &["file_size"]).shown(&[r#"now - mtime("Cargo.lock") to h"#]),
    doc("rm", "rm(path: str, ...) -> nil", "delete files and empty directories", &[], &["mv", "mkdir"]).shown(&[r#"rm("out.txt")"#, "rm *.tmp", "rm -r build"])
        .detail("With -r, also directories and everything in them. With -f, paths that don't exist are skipped instead of being an error"),
    doc("mv", "mv(from: str, to: str, ...) -> nil", "move or rename files and directories", &[], &["cp", "rm"]).shown(&[r#"mv("draft.txt", "final.txt")"#, "mv *.log logs"])
        .detail("If the last path is a directory, everything before it moves into it, so you can move several at once. Missing folders are created"),
    doc("cp", "cp(from: str, to: str, ...) -> nil", "copy files", &[], &["mv"]).shown(&[r#"cp("data.csv", "backup/data.csv")"#, "cp -r src backup"])
        .detail("If the last path is a directory, everything before it is copied into it. With -r, directories are copied too. Missing folders are created"),
    doc("mkdir", "mkdir(path: str, ...) -> nil", "create directories, along with any missing folders above them", &[], &["rm"]).shown(&[r#"mkdir("out/2026/10")"#, "mkdir a b"]),
    doc("path_join", "path_join(a: str, b: str, ...) -> str", "join pieces into one path, with / or \\ between them as your system uses", &[r#"path_join("a", "b.txt")"#], &["dirname", "basename"]),
    doc("basename", "basename(path: str) -> str|nil", "the last part of a path, usually the file name, or nil", &[r#""a/b/c.txt".basename"#], &["dirname", "stem"]),
    doc("dirname", "dirname(path: str) -> str|nil", "everything before the last part of a path, usually its folder, or nil", &[r#""a/b/c.txt".dirname"#], &["basename"]),
    doc("ext", "ext(path: str) -> str|nil", "the file's extension without the dot, like \"gz\", or nil if it has none", &[r#""a/b.tar.gz".ext"#, r#""README".ext"#], &["stem"]),
    doc("stem", "stem(path: str) -> str|nil", "the file name without its last extension, or nil", &[r#""a/b.tar.gz".stem"#], &["ext", "basename"]),
    doc("abspath", "abspath(path: str) -> str", "the full path from the root of the file system, filling in the current directory", &[], &["cwd"]).shown(&[r#"abspath("src")"#])
        .detail("The path doesn't have to exist"),
    doc("cwd", "cwd() -> str", "the current directory, which relative paths start from", &[], &["abspath", "cd"]).shown(&["cwd()"]),
    doc("pwd", "pwd() -> str", "the current directory, the same as cwd() under the shell's name", &[], &["cwd", "cd"]).shown(&["pwd"]),
    doc("cd", "cd(dir?: str) -> str", "change the current directory and return the new one", &[], &["cwd"]).shown(&[r#"cd("src")"#, r#"cd("..")"#, r#"cd("-")"#, "cd()"])
        .detail("With no argument, go to your home directory, and with \"-\", go back to the previous one"),
    doc("from_json", "from_json(s: str) -> any", "read JSON text into zil values: objects become maps, arrays lists, and null nil", &[r#""\{\"a\": [1, 2.5, null]}".from_json"#], &["to_json", "parse"]),
    doc("to_json", "to_json(v: any) -> str", "v as JSON text", &[r#"{a: [1, nil], b: 5 km}.to_json"#], &["from_json"])
        .detail("Values JSON has no type for, like quantities, fractions and dates, become strings"),
    doc("from_csv", "from_csv(s: str, header?: bool) -> table|list", "read CSV (comma-separated) text", &["\"name,n\\nx,1\".from_csv", "\"1,2\\n3,4\".from_csv(false)"], &["to_csv", "table"])
        .detail("The first row is taken as column names and the result is a table. Pass false if there's no such row, to get a list of rows. Numbers become numbers and empty cells nil"),
    doc("to_csv", "to_csv(rows: table|list) -> str", "CSV text from a table, a list of maps, or a list of lists", &[r#"[{a: 1, b: "x,y"}].to_csv"#, "[[1, 2], [3, 4]].to_csv"], &["from_csv"])
        .detail("Cells with commas or quotes are quoted"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let host = crate::host::get();
    let (args, f) = flags(name, args)?;
    Ok(match (name, args) {
        ("read_file", [Str(p)]) => {
            let bytes = host.read(p).map_err(io("read", p))?;
            Value::str(String::from_utf8(bytes).map_err(|_| Fail::Arg(0, format!("cannot read {p:?}: binary file; use `read_bytes`")))?)
        }
        ("read_bytes", [Str(p)]) => Value::bytes(host.read(p).map_err(io("read", p))?),
        ("write_file" | "append_file", [Str(p), Bytes(b)]) => {
            host.write(p, b, name == "append_file").map_err(io("write", p))?;
            Nil
        }
        ("write_file" | "append_file", [Str(p), v]) => {
            host.write(p, text(v).as_bytes(), name == "append_file").map_err(io("write", p))?;
            Nil
        }
        ("exists", [Str(p)]) => Bool(host.metadata(p).is_ok()),
        ("is_dir", [Str(p)]) => Bool(host.metadata(p).is_ok_and(|m| m.is_dir)),
        ("ls", [] | [Str(_)]) => {
            let dir = match args {
                [Str(d)] => &**d,
                _ => ".",
            };
            let mut names = host.list(dir).map_err(io("list", dir))?;
            names.retain(|n| f.contains('a') || !n.starts_with('.'));
            names.sort();
            let entries = names.into_iter().map(|n| {
                let path = Path::new(dir).join(&n).to_string_lossy().into_owned();
                (n, path)
            });
            entries_table(entries)?
        }
        ("glob", [Str(p)]) => entries_table(glob(p).map_err(|e| Fail::Arg(0, e))?.into_iter().map(|s| (s.clone(), s)))?,
        ("file_size", [Str(p)]) => bytes(host.metadata(p).map_err(io("read", p))?.len),
        ("mtime", [Str(p)]) => Value::date(host.metadata(p).map_err(io("read", p))?.modified.to_zoned(crate::host::tz())),
        ("cat", [_, ..]) => {
            let mut out = String::new();
            for (i, p) in paths(args)?.into_iter().enumerate() {
                let bytes = host.read(p).map_err(io_at(i, "read", p))?;
                out += &String::from_utf8(bytes).map_err(|_| Fail::Arg(i, format!("cannot read {p:?}: binary file; use `xxd` or `read_bytes`")))?;
            }
            Value::str(out)
        }
        // `rm -f *.tmp` matching nothing is fine, like in a shell.
        ("rm", _) if !args.is_empty() || f.contains('f') => {
            for (i, p) in paths(args)?.into_iter().enumerate() {
                match if f.contains('r') { host.remove_all(p) } else { host.remove(p) } {
                    Err(e) if f.contains('f') && e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) if !f.contains('r') && host.metadata(p).is_ok_and(|m| m.is_dir) => {
                        let why = io_reason(&e);
                        return Err(Fail::Arg(i, format!("cannot remove {p:?}: {why}\nhelp: `rm -r` removes a directory and everything in it")));
                    }
                    done => done.map_err(io_at(i, "remove", p))?,
                }
            }
            Nil
        }
        ("mv" | "cp", [_, _, ..]) => {
            let ps = paths(args)?;
            let (to, last) = (ps[ps.len() - 1], ps.len() - 1);
            let into = host.metadata(to).is_ok_and(|m| m.is_dir);
            if !into && last > 1 {
                return Err(Fail::Arg(last, format!("{to:?} is not a directory\nnote: with several paths, the last is the folder they go into")));
            }
            if !into {
                parents(to)?;
            }
            for (i, from) in ps[..last].iter().enumerate() {
                let dest = if into { Path::new(to).join(Path::new(from).file_name().unwrap_or_default()).to_string_lossy().into_owned() } else { to.into() };
                let done = match name {
                    "mv" => host.rename(from, &dest),
                    _ if !f.contains('r') && host.metadata(from).is_ok_and(|m| m.is_dir) => {
                        return Err(Fail::Arg(i, format!("cannot copy {from:?}: it's a directory\nhelp: `cp -r` copies a directory and everything in it")));
                    }
                    _ if absolute(&dest).starts_with(absolute(from)) => return Err(Fail::Arg(i, format!("cannot copy {from:?} into itself"))),
                    _ => copy_tree(&*host, from, &dest),
                };
                done.map_err(io_at(i, if name == "mv" { "move" } else { "copy" }, from))?;
            }
            Nil
        }
        ("mkdir", [_, ..]) => {
            for (i, p) in paths(args)?.into_iter().enumerate() {
                host.mkdir(p).map_err(io_at(i, "create", p))?;
            }
            Nil
        }
        ("path_join", [Str(_), ..]) => {
            let mut out = PathBuf::new();
            for (i, a) in args.iter().enumerate() {
                let Str(a) = a else { return Err(Fail::Arg(i, format!("expected str, got {}", a.type_name()))) };
                out.push(&**a);
            }
            Value::str(out.to_string_lossy())
        }
        ("basename" | "dirname" | "ext" | "stem", [Str(p)]) => {
            let p = Path::new(&**p);
            let part = match name {
                "basename" => p.file_name(),
                "dirname" => p.parent().map(|d| d.as_os_str()),
                "ext" => p.extension(),
                _ => p.file_stem(),
            };
            part.map_or(Nil, |s| Value::str(s.to_string_lossy()))
        }
        ("abspath", [Str(p)]) => Value::str(Path::new(&host.cwd().map_err(io("resolve", p))?).join(&**p).to_string_lossy()),
        ("cwd" | "pwd", []) => Value::str(host.cwd().map_err(|e| io_reason(&e))?),
        ("cd", [] | [Str(_)]) => {
            let dir = match args {
                [Str(p)] if &**p == "-" => crate::state::with(|s| s.prev_dir.borrow().clone()).ok_or_else(|| Fail::Msg("no previous directory".into()))?,
                [Str(p)] => p.to_string(),
                _ => host.env("USERPROFILE").or_else(|| host.env("HOME")).ok_or_else(|| Fail::Msg("no home directory set".into()))?,
            };
            let before = host.cwd().ok();
            host.cd(&dir).map_err(io("enter", &dir))?;
            crate::state::with(|s| *s.prev_dir.borrow_mut() = before);
            Value::str(host.cwd().map_err(|e| io_reason(&e))?)
        }
        ("from_json", [Str(s)]) => from_json(serde_json::from_str(s).map_err(|e| Fail::Arg(0, format!("invalid JSON: {e}")))?),
        ("to_json", [v]) => Value::str(to_json(v).to_string()),
        ("from_csv", [Str(s)]) => from_csv(s, true)?,
        ("from_csv", [Str(s), Bool(h)]) => from_csv(s, *h)?,
        ("to_csv", [Table(t)]) => Value::str(csv(&t.cols, &t.rows)),
        ("to_csv", [List(l)]) => {
            let l = l.borrow();
            match value::Table::from_maps(&l) {
                Ok(t) => Value::str(csv(&t.cols, &t.rows)),
                Err(e) if l.iter().all(|r| matches!(r, Map(_))) => return Err(Fail::Arg(0, e)),
                Err(_) if l.iter().all(|r| matches!(r, List(_))) => {
                    Value::str(l.iter().map(|r| if let List(r) = r { line(&r.borrow()) } else { unreachable!() }).collect::<String>())
                }
                Err(_) => return Err(Fail::Arg(0, "expected a list of maps or a list of lists".into())),
            }
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// An io error blaming the path, always the first argument: `cannot read "x": no such file or directory`.
/// Creates the folders `path` needs, so `mv`/`cp` can land in a new one.
fn parents(path: &str) -> Result<(), Fail> {
    match Path::new(path).parent() {
        Some(d) if !d.as_os_str().is_empty() => {
            crate::host::get().mkdir(&d.to_string_lossy()).map_err(|e| Fail::Arg(1, format!("cannot create {:?}: {}", d, io_reason(&e))))
        }
        _ => Ok(()),
    }
}

fn io<'a>(verb: &'a str, path: &'a str) -> impl FnOnce(std::io::Error) -> Fail + 'a {
    io_at(0, verb, path)
}

/// Like `io`, blaming argument `i`.
fn io_at<'a>(i: usize, verb: &'a str, path: &'a str) -> impl FnOnce(std::io::Error) -> Fail + 'a {
    move |e| Fail::Arg(i, format!("cannot {verb} {path:?}: {}", io_reason(&e)))
}

/// Every argument as a path, for commands that take several.
fn paths(args: &[Value]) -> Result<Vec<&str>, Fail> {
    fn path((i, a): (usize, &Value)) -> Result<&str, Fail> {
        match a {
            Value::Str(s) => Ok(s),
            a => Err(Fail::Arg(i, format!("expected str, got {}", a.type_name()))),
        }
    }
    args.iter().enumerate().map(path).collect()
}

/// `path` from the root, with `.` and `..` resolved by name, not by following links.
fn absolute(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in Path::new(&crate::host::get().cwd().unwrap_or_default()).join(path).components() {
        match c {
            Component::ParentDir => drop(out.pop()),
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// Copies a file, or a directory and everything in it.
// ponytail: follows symlinked directories, so a link back up the tree recurses until paths get too long; skip links if that bites.
fn copy_tree(host: &dyn Host, from: &str, to: &str) -> std::io::Result<()> {
    if !host.metadata(from)?.is_dir {
        return host.copy(from, to);
    }
    host.mkdir(to)?;
    for n in host.list(from)? {
        let at = |d: &str| Path::new(d).join(&n).to_string_lossy().into_owned();
        copy_tree(host, &at(from), &at(to))?;
    }
    Ok(())
}

/// What `write_file` writes: lists one item per line, so they round-trip with `.lines`.
pub fn text(v: &Value) -> String {
    match v {
        Value::List(l) => l.borrow().iter().map(|x| format!("{x}\n")).collect(),
        v => v.to_string(),
    }
}

/// An io error in lowercase with no OS code: `no such file or directory`.
pub fn io_reason(e: &std::io::Error) -> String {
    let s = e.to_string();
    let s = s.split(" (os error").next().unwrap_or(&s).trim_end_matches('.');
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_lowercase().chain(c).collect())
}

fn bytes(n: u64) -> Value {
    Value::Qty(n as f64, units::terms("B").expect("bytes are a unit"))
}

/// The `ls` table for (shown name, path) pairs.
fn entries_table(entries: impl Iterator<Item = (String, String)>) -> Result<Value, Fail> {
    let (host, tz) = (crate::host::get(), crate::host::tz());
    let mut rows = Vec::new();
    for (name, path) in entries {
        let md = host.metadata(&path).map_err(|e| format!("cannot read {name:?}: {}", io_reason(&e)))?;
        let kind = if md.is_dir { "dir" } else { "file" };
        // Directory sizes are filesystem bookkeeping (0 or 4096 B), not contents.
        let size = if md.is_dir { 0 } else { md.len };
        rows.push(vec![Value::str(name), Value::str(kind), bytes(size), Value::date(md.modified.to_zoned(tz.clone()))]);
    }
    Ok(Value::table(Table { cols: ["name", "type", "size", "modified"].map(String::from).to_vec(), rows }))
}

/// Paths matching `pat`, sorted within each directory: `*` and `?` stay within a name, `**` crosses directories.
// ponytail: `**` follows symlinked directories, so a link cycle recurses until the OS refuses; track visited dirs if that bites.
fn glob(pat: &str) -> Result<Vec<String>, String> {
    let pat = pat.replace('\\', "/");
    let parts: Vec<&str> = pat.split('/').collect();
    let fixed = parts.iter().take_while(|p| !p.contains(['*', '?'])).count();
    if fixed == parts.len() {
        return Ok(if crate::host::get().metadata(&pat).is_ok() { vec![pat] } else { vec![] });
    }
    let base = parts[..fixed].join("/");
    let (dir, prefix) = match fixed {
        0 => (".".to_string(), String::new()),
        _ if base.is_empty() => ("/".to_string(), "/".to_string()),
        _ => (base.clone(), format!("{base}/")),
    };
    let mut re = String::from("^");
    let mut cs = pat.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '*' if cs.peek() == Some(&'*') => {
                cs.next();
                re += if cs.next_if_eq(&'/').is_some() { "(?:.*/)?" } else { ".*" };
            }
            '*' => re += "[^/]*",
            '?' => re += "[^/]",
            c => re += &regex::escape(&c.to_string()),
        }
    }
    let re = Regex::new(&(re + "$")).map_err(|e| e.to_string())?;
    let max = if pat.contains("**") { usize::MAX } else { parts.len() - fixed };
    let mut out = Vec::new();
    walk(&*crate::host::get(), Path::new(&dir), &prefix, max, &re, &mut out);
    Ok(out)
}

fn walk(host: &dyn Host, dir: &Path, prefix: &str, depth: usize, re: &Regex, out: &mut Vec<String>) {
    let Ok(mut names) = host.list(&dir.to_string_lossy()) else { return };
    names.sort();
    for name in names {
        let path = format!("{prefix}{name}");
        if re.is_match(&path) {
            out.push(path.clone());
        }
        let sub = dir.join(&name);
        if depth > 1 && host.metadata(&sub.to_string_lossy()).is_ok_and(|m| m.is_dir) {
            walk(host, &sub, &format!("{path}/"), depth - 1, re, out);
        }
    }
}

pub fn from_json(j: serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Bool(b),
        J::Number(n) => n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::int),
        J::String(s) => Value::str(s),
        J::Array(a) => Value::list(a.into_iter().map(from_json).collect()),
        J::Object(o) => Value::map(o.into_iter().map(|(k, v)| (k, from_json(v))).collect::<IndexMap<_, _>>()),
    }
}

/// JSON for `v`; what JSON can't hold becomes its display string. Non-finite floats become null.
fn to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    let all = |vs: &mut dyn Iterator<Item = &Value>| J::Array(vs.map(to_json).collect());
    match v {
        Value::Nil => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(n, _) => J::from(*n),
        Value::Float(x) => serde_json::Number::from_f64(*x).map_or(J::Null, J::Number),
        Value::Frac(r, _) => J::String(r.to_string()),
        Value::Str(s) => J::String(s.to_string()),
        Value::List(l) => all(&mut l.borrow().iter()),
        Value::Set(s) => all(&mut s.borrow().iter()),
        Value::Map(m) => J::Object(m.borrow().iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
        Value::Table(t) => all(&mut t.maps().iter()),
        v => J::String(v.to_string()),
    }
}

fn from_csv(s: &str, header: bool) -> Result<Value, String> {
    let mut rows = parse_csv(s)?.into_iter().map(|r| r.into_iter().map(|f| cell(&f)).collect::<Vec<_>>());
    if !header {
        return Ok(Value::list(rows.map(Value::list).collect()));
    }
    let cols: Vec<String> = rows.next().unwrap_or_default().into_iter().map(|v| if let Value::Nil = v { String::new() } else { v.to_string() }).collect();
    let rows = rows.map(|mut r| {
        r.resize(cols.len(), Value::Nil);
        r
    });
    Ok(Value::table(Table { rows: rows.collect(), cols }))
}

/// A CSV field as a value: numbers become numbers, empty is nil.
fn cell(f: &str) -> Value {
    if f.is_empty() {
        Value::Nil
    } else if let Ok(n) = f.parse::<i64>() {
        Value::int(n)
    } else if let Ok(x) = f.parse::<f64>().map_err(drop).and_then(|x| if f.bytes().any(|b| b.is_ascii_digit()) { Ok(x) } else { Err(()) }) {
        Value::Float(x)
    } else {
        Value::str(f)
    }
}

/// RFC 4180 rows: `"`-quoted fields may hold commas, newlines and `""` for a quote.
fn parse_csv(s: &str) -> Result<Vec<Vec<String>>, String> {
    let (mut rows, mut row, mut field, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut cs = s.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '"' if quoted && cs.next_if_eq(&'"').is_some() => field.push('"'),
            '"' => quoted = !quoted,
            c if quoted => field.push(c),
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if quoted {
        return Err("unclosed quote".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

/// CSV with a header row; how a table displays as a string.
pub fn csv(cols: &[String], rows: &[Vec<Value>]) -> String {
    let header: Vec<Value> = cols.iter().map(|c| Value::str(c.as_str())).collect();
    std::iter::once(&header).chain(rows).map(|r| line(r)).collect()
}

fn line(r: &[Value]) -> String {
    let field = |v: &Value| {
        let s = if let Value::Nil = v { String::new() } else { v.to_string() };
        if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
    };
    r.iter().map(field).collect::<Vec<_>>().join(",") + "\n"
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn data() {
        assert_eq!(show(r#""\{\"a\": [1, 2.5, null], \"b\": \{}}".from_json"#), "{a: [1, 2.5, nil], b: {}}");
        assert_eq!(show(r#"{a: [1, nil], b: 5 km, c: "q\""}.to_json"#), r#"{"a":[1,null],"b":"5 km","c":"q\""}"#);
        let csv = r#""name,n\n\"x, \"\"y\"\"\",1\nz,\n""#;
        assert_eq!(show(&format!("{csv}.from_csv.to_csv == {csv}")), "true");
        assert_eq!(show(&format!("{csv}.from_csv.n")), "[1, nil]");
        assert_eq!(show(r#""a,1.5\nnan,inf".from_csv(false)"#), r#"[["a", 1.5], ["nan", "inf"]]"#);
        assert!(try_eval(r#""\"a".from_csv"#).is_err());
        assert!(try_eval(r#"cd("no/such/dir")"#).is_err());
        assert!(try_eval(r#"cd("-")"#).is_err_and(|e| e.msg.contains("no previous directory")));
        assert_eq!(show(r#"["a/b.tar.gz".stem, "a/b.tar.gz".ext, "a/b".dirname, "x".ext, "/".basename]"#), r#"["b.tar", "gz", "a", nil, nil]"#);
    }
}
