//! Files: reading and writing, directories as tables, path pieces, JSON and CSV.

use crate::host::Host;
use crate::modules::{Fail, Module, units};
use crate::value::{Table, Value};
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
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("read and write", &["read_file", "read_bytes", "cat", "write_file", "append_file"]),
        ("look", &["exists", "is_dir", "ls", "glob", "file_size", "mtime"]),
        ("change", &["rm", "mv", "cp", "mkdir"]),
        ("paths", &["path_join", "basename", "dirname", "ext", "stem", "abspath", "cwd", "pwd", "cd"]),
        ("data", &["from_json", "to_json", "from_csv", "to_csv"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{
        absolute, bytes, copy_tree, csv, entries_table, from_csv as read_csv, from_json as json_value, glob as matches, io, io_at, io_reason, line, parents,
        text, to_json as json,
    };
    use crate::modules::Fail;
    use crate::signature::{As, Flags, List, Or, Qty, Rest};
    use crate::value::{Table, Value};
    use jiff::Zoned;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    /// a file's contents, as a string
    #[builtin(shown = [r#"read_file("notes.txt").lines.len"#, r#"read_file("data.json").from_json"#], see = ["write_file", "lines", "from_csv"])]
    fn read_file(path: &str) -> Result<String, Fail> {
        let bytes = crate::host::get().read(path).map_err(io("read", path))?;
        String::from_utf8(bytes).map_err(|_| Fail::Arg(0, format!("cannot read {path:?}: binary file; use `read_bytes`")))
    }

    /// a file's contents, as bytes, for files that aren't text
    #[builtin(shown = [r#"read_bytes("a.out")[..4] == b"\x7fELF""#], see = ["read_file", "bytes", "hexdump"])]
    fn read_bytes(path: &str) -> Result<Vec<u8>, Fail> {
        crate::host::get().read(path).map_err(io("read", path))
    }

    /// the contents of one or more files, joined into one string
    #[builtin(shown = [r#"cat("notes.txt").lines.len"#, "cat a.txt b.txt"], see = ["read_file", "grep", "wc"], flags = "")]
    fn cat(path: &str, more: Rest<&str>, _f: Flags) -> Result<String, Fail> {
        let mut out = String::new();
        for (i, p) in [path].into_iter().chain(more.0).enumerate() {
            let bytes = crate::host::get().read(p).map_err(io_at(i, "read", p))?;
            out += &String::from_utf8(bytes).map_err(|_| Fail::Arg(i, format!("cannot read {p:?}: binary file; use `xxd` or `read_bytes`")))?;
        }
        Ok(out)
    }

    fn write(path: &str, v: &Value, append: bool) -> Result<(), Fail> {
        let host = crate::host::get();
        match v {
            Value::Bytes(b) => host.write(path, b, append),
            v => host.write(path, text(v).as_bytes(), append),
        }
        .map_err(io("write", path))
    }

    /// write v to a file, replacing what was there
    ///
    /// Values are written as text, a list one item per line and a table as CSV, while bytes are written exactly
    #[builtin(shown = [r#"write_file("out.txt", [1, 2, 3])"#, r#"write_file("files.csv", ls())"#], see = ["read_file", "append_file"])]
    fn write_file(path: &str, v: &Value) -> Result<(), Fail> {
        write(path, v, false)
    }

    /// add v to the end of a file, creating it if needed
    ///
    /// A list goes one item per line
    #[builtin(shown = [r#"append_file("log.txt", ["started {now}"])"#], see = ["write_file"])]
    fn append_file(path: &str, v: &Value) -> Result<(), Fail> {
        write(path, v, true)
    }

    /// whether a file or directory exists
    #[builtin(shown = [r#"exists("Cargo.toml")"#], see = ["is_dir"])]
    fn exists(path: &str) -> bool {
        crate::host::get().metadata(path).is_ok()
    }

    /// whether path is a directory (folder)
    #[builtin(shown = [r#"is_dir("src")"#], see = ["exists"])]
    fn is_dir(path: &str) -> bool {
        crate::host::get().metadata(path).is_ok_and(|m| m.is_dir)
    }

    /// what's in a directory, as a table of name, type, size and modified
    ///
    /// Names starting with a dot are hidden unless you add -a
    #[builtin(shown = ["ls()", r#"ls("src").where(|f| f.type == "file").name"#, "ls -a"], see = ["glob", "sort_by"], flags = "al")]
    fn ls(#[default(".")] dir: &str, f: Flags) -> Result<As<Table>, Fail> {
        let mut names = crate::host::get().list(dir).map_err(io("list", dir))?;
        names.retain(|n| f.has('a') || !n.starts_with('.'));
        names.sort();
        let entries = names.into_iter().map(|n| {
            let path = Path::new(dir).join(&n).to_string_lossy().into_owned();
            (n, path)
        });
        Ok(entries_table(entries)?.into())
    }

    /// every path that matches the pattern pat, as a table like ls gives
    ///
    /// * matches any part of a name, ? any one character, and ** any number of folders, so **/*.rs finds .rs files at any depth
    #[builtin(shown = [r#"glob("**/*.rs").size.sum"#, r#"glob("*.csv").name"#], see = ["ls"])]
    fn glob(pat: &str) -> Result<As<Table>, Fail> {
        Ok(entries_table(matches(pat).map_err(|e| Fail::Arg(0, e))?.into_iter().map(|s| (s.clone(), s)))?.into())
    }

    /// a file's size, as a quantity in bytes
    #[builtin(shown = [r#"file_size("Cargo.lock") to KiB"#], see = ["mtime", "human_bytes"])]
    fn file_size(path: &str) -> Result<As<Qty>, Fail> {
        Ok(bytes(crate::host::get().metadata(path).map_err(io("read", path))?.len).into())
    }

    /// when a file was last changed, as a date
    #[builtin(shown = [r#"now - mtime("Cargo.lock") to h"#], see = ["file_size"])]
    fn mtime(path: &str) -> Result<Zoned, Fail> {
        Ok(crate::host::get().metadata(path).map_err(io("read", path))?.modified.to_zoned(crate::host::tz()))
    }

    /// delete files and empty directories
    ///
    /// With -r, also directories and everything in them. With -f, paths that don't exist are skipped instead of being an error
    #[builtin(shown = [r#"rm("out.txt")"#, "rm *.tmp", "rm -r build"], see = ["mv", "mkdir"], flags = "rf")]
    fn rm(paths: Rest<&str>, f: Flags) -> Result<(), Fail> {
        // `rm -f *.tmp` matching nothing is fine, like in a shell.
        if paths.0.is_empty() && !f.has('f') {
            return Err(Fail::BadArgs);
        }
        let host = crate::host::get();
        for (i, p) in paths.0.into_iter().enumerate() {
            match if f.has('r') { host.remove_all(p) } else { host.remove(p) } {
                Err(e) if f.has('f') && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) if !f.has('r') && host.metadata(p).is_ok_and(|m| m.is_dir) => {
                    let why = io_reason(&e);
                    return Err(Fail::Arg(i, format!("cannot remove {p:?}: {why}\nhelp: `rm -r` removes a directory and everything in it")));
                }
                done => done.map_err(io_at(i, "remove", p))?,
            }
        }
        Ok(())
    }

    /// Moves or copies every path but the last to the last, or into it when it's a directory.
    fn transfer(ps: Vec<&str>, copy: bool, f: &Flags) -> Result<(), Fail> {
        let host = crate::host::get();
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
            let done = match copy {
                false => host.rename(from, &dest),
                _ if !f.has('r') && host.metadata(from).is_ok_and(|m| m.is_dir) => {
                    return Err(Fail::Arg(i, format!("cannot copy {from:?}: it's a directory\nhelp: `cp -r` copies a directory and everything in it")));
                }
                _ if absolute(&dest).starts_with(absolute(from)) => return Err(Fail::Arg(i, format!("cannot copy {from:?} into itself"))),
                _ => copy_tree(&*host, from, &dest),
            };
            done.map_err(io_at(i, if copy { "copy" } else { "move" }, from))?;
        }
        Ok(())
    }

    /// move or rename files and directories
    ///
    /// If the last path is a directory, everything before it moves into it, so you can move several at once. Missing folders are created
    #[builtin(shown = [r#"mv("draft.txt", "final.txt")"#, "mv *.log logs"], see = ["cp", "rm"], flags = "")]
    fn mv(from: &str, to: &str, more: Rest<&str>, f: Flags) -> Result<(), Fail> {
        transfer([from, to].into_iter().chain(more.0).collect(), false, &f)
    }

    /// copy files
    ///
    /// If the last path is a directory, everything before it is copied into it. With -r, directories are copied too. Missing folders are created
    #[builtin(shown = [r#"cp("data.csv", "backup/data.csv")"#, "cp -r src backup"], see = ["mv"], flags = "r")]
    fn cp(from: &str, to: &str, more: Rest<&str>, f: Flags) -> Result<(), Fail> {
        transfer([from, to].into_iter().chain(more.0).collect(), true, &f)
    }

    /// create directories, along with any missing folders above them
    #[builtin(shown = [r#"mkdir("out/2026/10")"#, "mkdir a b"], see = ["rm"], flags = "p")]
    fn mkdir(path: &str, more: Rest<&str>, _f: Flags) -> Result<(), Fail> {
        for (i, p) in [path].into_iter().chain(more.0).enumerate() {
            crate::host::get().mkdir(p).map_err(io_at(i, "create", p))?;
        }
        Ok(())
    }

    /// join pieces into one path, with / or \ between them as your system uses
    #[builtin(examples = [r#"path_join("a", "b.txt")"#], see = ["dirname", "basename"])]
    fn path_join(a: &str, more: Rest<&str>) -> String {
        let mut out = PathBuf::from(a);
        for p in more.0 {
            out.push(p);
        }
        out.to_string_lossy().into_owned()
    }

    fn part(p: Option<&std::ffi::OsStr>) -> Option<String> {
        p.map(|s| s.to_string_lossy().into_owned())
    }

    /// the last part of a path, usually the file name, or nil
    #[builtin(examples = [r#""a/b/c.txt".basename"#], see = ["dirname", "stem"])]
    fn basename(path: &str) -> Option<String> {
        part(Path::new(path).file_name())
    }

    /// everything before the last part of a path, usually its folder, or nil
    #[builtin(examples = [r#""a/b/c.txt".dirname"#], see = ["basename"])]
    fn dirname(path: &str) -> Option<String> {
        part(Path::new(path).parent().map(|d| d.as_os_str()))
    }

    /// the file's extension without the dot, like "gz", or nil if it has none
    #[builtin(examples = [r#""a/b.tar.gz".ext"#, r#""README".ext"#], see = ["stem"])]
    fn ext(path: &str) -> Option<String> {
        part(Path::new(path).extension())
    }

    /// the file name without its last extension, or nil
    #[builtin(examples = [r#""a/b.tar.gz".stem"#], see = ["ext", "basename"])]
    fn stem(path: &str) -> Option<String> {
        part(Path::new(path).file_stem())
    }

    /// the full path from the root of the file system, filling in the current directory
    ///
    /// The path doesn't have to exist
    #[builtin(shown = [r#"abspath("src")"#], see = ["cwd"])]
    fn abspath(path: &str) -> Result<String, Fail> {
        Ok(Path::new(&crate::host::get().cwd().map_err(io("resolve", path))?).join(path).to_string_lossy().into_owned())
    }

    /// the current directory, which relative paths start from
    #[builtin(shown = ["cwd()"], see = ["abspath", "cd"])]
    fn cwd() -> Result<String, String> {
        crate::host::get().cwd().map_err(|e| io_reason(&e))
    }

    /// the current directory, the same as cwd() under the shell's name
    #[builtin(shown = ["pwd"], see = ["cwd", "cd"], flags = "")]
    fn pwd(_f: Flags) -> Result<String, String> {
        cwd()
    }

    /// change the current directory and return the new one
    ///
    /// With no argument, go to your home directory, and with "-", go back to the previous one
    #[builtin(shown = [r#"cd("src")"#, r#"cd("..")"#, r#"cd("-")"#, "cd()"], see = ["cwd"], flags = "")]
    fn cd(dir: Option<&str>, _f: Flags) -> Result<String, Fail> {
        let host = crate::host::get();
        let dir = match dir {
            Some("-") => crate::state::with(|s| s.prev_dir.borrow().clone()).ok_or_else(|| Fail::Msg("no previous directory".into()))?,
            Some(p) => p.to_string(),
            None => host.env("USERPROFILE").or_else(|| host.env("HOME")).ok_or_else(|| Fail::Msg("no home directory set".into()))?,
        };
        let before = host.cwd().ok();
        host.cd(&dir).map_err(io("enter", &dir))?;
        crate::state::with(|s| *s.prev_dir.borrow_mut() = before);
        Ok(host.cwd().map_err(|e| io_reason(&e))?)
    }

    /// read JSON text into zil values: objects become maps, arrays lists, and null nil
    #[builtin(examples = [r#""\{\"a\": [1, 2.5, null]}".from_json"#], see = ["to_json", "parse"])]
    fn from_json(s: &str) -> Result<Value, Fail> {
        Ok(json_value(serde_json::from_str(s).map_err(|e| Fail::Arg(0, format!("invalid JSON: {e}")))?))
    }

    /// v as JSON text
    ///
    /// Values JSON has no type for, like quantities, fractions and dates, become strings
    #[builtin(examples = [r#"{a: [1, nil], b: 5 km}.to_json"#], see = ["from_json"])]
    fn to_json(v: &Value) -> String {
        json(v).to_string()
    }

    /// read CSV (comma-separated) text
    ///
    /// The first row is taken as column names and the result is a table. Pass false if there's no such row, to get a list of rows. Numbers become numbers and empty cells nil
    #[builtin(examples = ["\"name,n\\nx,1\".from_csv", "\"1,2\\n3,4\".from_csv(false)"], see = ["to_csv", "table"])]
    fn from_csv(s: &str, #[default(true)] header: bool) -> Result<As<Or<Table, Vec<Value>>>, String> {
        read_csv(s, header).map(As::from)
    }

    /// CSV text from a table, a list of maps, or a list of lists
    ///
    /// Cells with commas or quotes are quoted
    #[builtin(examples = [r#"[{a: 1, b: "x,y"}].to_csv"#, "[[1, 2], [3, 4]].to_csv"], see = ["from_csv"])]
    fn to_csv(rows: Or<&Rc<Table>, &List>) -> Result<String, Fail> {
        let l = match rows {
            Or::A(t) => return Ok(csv(&t.cols, &t.rows)),
            Or::B(l) => l.borrow(),
        };
        match Table::from_maps(&l) {
            Ok(t) => Ok(csv(&t.cols, &t.rows)),
            Err(e) if l.iter().all(|r| matches!(r, Value::Map(_))) => Err(Fail::Arg(0, e)),
            Err(_) if l.iter().all(|r| matches!(r, Value::List(_))) => {
                Ok(l.iter().map(|r| if let Value::List(r) = r { line(&r.borrow()) } else { unreachable!() }).collect())
            }
            Err(_) => Err(Fail::Arg(0, "expected a list of maps or a list of lists".into())),
        }
    }
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
