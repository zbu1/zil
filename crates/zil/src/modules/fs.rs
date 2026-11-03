//! Files: reading and writing, directories as tables, the current directory.

use crate::host::Permission;
use crate::host::{Host, io_reason};
use crate::modules::{Fail, Module, units};
use crate::value::{Table, Value};
use regex::Regex;
use std::path::{Component, Path, PathBuf};

pub const MODULE: Module = Module {
    name: "fs",
    tags: &["files"],
    permissions: &[Permission::FilesRead, Permission::FilesWrite],
    about: "read and write files, and manage folders and the current directory",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Files and folders: reading and writing them and moving them around. Every function here reads \
a path as relative to the current directory unless it starts from the root."),
        ("writing", "\
`ls`, `rm`, `cp` and the other file functions also work as shell-style commands, like `rm -r build`, which \
help(\"shell\") explains. In an ordinary call, give the flags as a map: `rm(\"build\", {recursive: true})`."),
    ],
    #[rustfmt::skip]
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("read and write", &["read_file", "read_bytes", "write_file", "append_file", "save_session"]),
        ("look", &["exists", "is_dir", "ls", "glob", "file_size", "mtime"]),
        ("change", &["rm", "mv", "cp", "mkdir"]),
        ("paths", &["abspath", "cwd", "pwd", "cd"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{absolute, bytes, copy_tree, entries_table, glob as matches, io, io_at, parents, plan_table};
    use crate::host::{io_reason, text};
    use crate::interp::Interp;
    use crate::modules::Fail;
    use crate::signature::{As, Flags, Qty, Rest};
    use crate::value::{Table, Value};
    use jiff::Zoned;
    use std::path::Path;

    /// the contents of one or more files, joined into one string
    #[builtin(
        aliases = ["cat"],
        shown = [r#"read_file("notes.txt").lines.len"#, r#"read_file("data.json").from_json"#, r#"cat("a.txt", "b.txt")"#],
        see = ["write_file", "lines", "from_csv", "grep", "wc"],
        flags = [],
        command = "cat a.txt b.txt"
    )]
    fn read_file(path: &str, more: Rest<&str>, _f: Flags) -> Result<String, Fail> {
        let mut out = String::new();
        for (i, p) in [path].into_iter().chain(more.0).enumerate() {
            let bytes = crate::host::get().read(p).map_err(io_at(i, "read", p))?;
            out += &String::from_utf8(bytes).map_err(|_| Fail::Arg(i, format!("cannot read {p:?}: binary file; use `xxd` or `read_bytes`")))?;
        }
        Ok(out)
    }

    /// a file's contents, as bytes, for files that aren't text
    #[builtin(shown = [r#"read_bytes("a.out")[..4] == b"\x7fELF""#], see = ["read_file", "bytes", "hexdump"])]
    fn read_bytes(path: &str) -> Result<Vec<u8>, Fail> {
        crate::host::get().read(path).map_err(io("read", path))
    }

    fn write(v: &Value, path: &str, append: bool) -> Result<(), Fail> {
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
    #[builtin(shown = [r#"[1, 2, 3].write_file("out.txt")"#, r#"ls().write_file("files.csv")"#], see = ["read_file", "append_file"])]
    fn write_file(v: &Value, path: &str) -> Result<(), Fail> {
        write(v, path, false)
    }

    /// add v to the end of a file, creating it if needed
    ///
    /// A list goes one item per line
    #[builtin(shown = [r#"["started {now}"].append_file("log.txt")"#], see = ["write_file"])]
    fn append_file(v: &Value, path: &str) -> Result<(), Fail> {
        write(v, path, true)
    }

    /// write the session's variables, functions and units to a file as code that rebuilds them
    ///
    /// Run the file later to get them back. A value that can't be written as code, like a table, is left as a comment
    #[builtin(shown = [r#"save_session("defs.zil")"#], see = ["write_file"])]
    fn save_session(it: &mut Interp, path: &str) -> Result<(), Fail> {
        crate::host::get().write(path, crate::snapshot::prelude(it).as_bytes(), false).map_err(io("write", path))
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
    #[builtin(
        shown = ["ls()", r#"ls("src").where(|f| f.type == "file").name"#],
        see = ["glob", "sort_by"],
        flags = [("a", "all", "include names starting with a dot, which are hidden otherwise"), ("l", "long", "accepted out of habit: the table always has the details")],
        command = "ls -la src"
    )]
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
    #[builtin(
        shown = [r#"rm("out.txt")"#],
        see = ["mv", "mkdir"],
        flags = [("r", "recursive", "also directories and everything in them"), ("f", "force", "skip paths that don't exist instead of failing"), ("n", "dry_run", "list what would change, as a table of op, from and to, and change nothing")],
        command = "rm -rf build"
    )]
    fn rm(paths: Rest<&str>, f: Flags) -> Result<Option<As<Table>>, Fail> {
        // `rm -f *.tmp` matching nothing is fine, like in a shell.
        if paths.0.is_empty() && !f.has('f') {
            return Err(Fail::BadArgs);
        }
        let (host, mut plan) = (crate::host::get(), vec![]);
        for (i, p) in paths.0.into_iter().enumerate() {
            if f.has('n') {
                match host.metadata(p) {
                    Err(e) if f.has('f') && e.kind() == std::io::ErrorKind::NotFound => {}
                    done => plan.push(done.map(|_| ("remove", p.to_string(), String::new())).map_err(io_at(i, "remove", p))?),
                }
                continue;
            }
            match if f.has('r') { host.remove_all(p) } else { host.remove(p) } {
                Err(e) if f.has('f') && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) if !f.has('r') && host.metadata(p).is_ok_and(|m| m.is_dir) => {
                    let why = io_reason(&e);
                    return Err(Fail::Arg(i, format!("cannot remove {p:?}: {why}\nhelp: `rm -r` removes a directory and everything in it")));
                }
                done => done.map_err(io_at(i, "remove", p))?,
            }
        }
        Ok(f.has('n').then(|| plan_table(plan)))
    }

    /// Moves or copies every path but the last to the last, or into it when it's a directory.
    fn transfer(ps: Vec<&str>, copy: bool, f: &Flags) -> Result<Option<As<Table>>, Fail> {
        let (host, mut plan) = (crate::host::get(), vec![]);
        let (to, last) = (ps[ps.len() - 1], ps.len() - 1);
        let into = host.metadata(to).is_ok_and(|m| m.is_dir);
        if !into && last > 1 {
            return Err(Fail::Arg(last, format!("{to:?} is not a directory\nnote: with several paths, the last is the folder they go into")));
        }
        if !into && !f.has('n') {
            parents(to)?;
        }
        for (i, from) in ps[..last].iter().enumerate() {
            let dest = if into { Path::new(to).join(Path::new(from).file_name().unwrap_or_default()).to_string_lossy().into_owned() } else { to.into() };
            if f.has('n') {
                plan.push((if copy { "copy" } else { "move" }, from.to_string(), dest));
                continue;
            }
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
        Ok(f.has('n').then(|| plan_table(plan)))
    }

    /// move or rename files and directories
    ///
    /// If the last path is a directory, everything before it moves into it, so you can move several at once. Missing folders are created
    #[builtin(shown = [r#"mv("draft.txt", "final.txt")"#], see = ["cp", "rm"], flags = [("n", "dry_run", "list what would change, as a table of op, from and to, and change nothing")], command = "mv *.log logs")]
    fn mv(from: &str, to: &str, more: Rest<&str>, f: Flags) -> Result<Option<As<Table>>, Fail> {
        transfer([from, to].into_iter().chain(more.0).collect(), false, &f)
    }

    /// copy files
    ///
    /// If the last path is a directory, everything before it is copied into it. Missing folders are created
    #[builtin(shown = [r#"cp("data.csv", "backup/data.csv")"#], see = ["mv"], flags = [("r", "recursive", "copy directories too"), ("n", "dry_run", "list what would change, as a table of op, from and to, and change nothing")], command = "cp -r src backup")]
    fn cp(from: &str, to: &str, more: Rest<&str>, f: Flags) -> Result<Option<As<Table>>, Fail> {
        transfer([from, to].into_iter().chain(more.0).collect(), true, &f)
    }

    /// create directories, along with any missing folders above them
    #[builtin(
        shown = [r#"mkdir("out/2026/10")"#],
        see = ["rm"],
        flags = [("p", "parents", "accepted out of habit: missing folders above are always made"), ("n", "dry_run", "list what would change, as a table of op, from and to, and change nothing")],
        command = "mkdir -p out/logs"
    )]
    fn mkdir(path: &str, more: Rest<&str>, f: Flags) -> Result<Option<As<Table>>, Fail> {
        let mut plan = vec![];
        for (i, p) in [path].into_iter().chain(more.0).enumerate() {
            if f.has('n') {
                plan.push(("mkdir", String::new(), p.to_string()));
            } else {
                crate::host::get().mkdir(p).map_err(io_at(i, "create", p))?;
            }
        }
        Ok(f.has('n').then(|| plan_table(plan)))
    }

    /// the full path from the root of the file system, filling in the current directory
    ///
    /// The path doesn't have to exist
    #[builtin(shown = [r#"abspath("src")"#], see = ["cwd"])]
    fn abspath(path: &str) -> Result<String, Fail> {
        Ok(Path::new(&crate::host::get().cwd().map_err(io("resolve", path))?).join(path).to_string_lossy().into_owned())
    }

    /// the current directory, which relative paths start from
    #[builtin(shown = ["cwd()"], see = ["abspath", "cd"], flags = [], command = "cwd")]
    fn cwd(_f: Flags) -> Result<String, String> {
        crate::host::get().cwd().map_err(|e| io_reason(&e))
    }

    /// the current directory, the same as cwd() under the shell's name
    #[builtin(shown = ["pwd()"], see = ["cwd", "cd"], flags = [], command = "pwd")]
    fn pwd(f: Flags) -> Result<String, String> {
        cwd(f)
    }

    /// change the current directory and return the new one
    ///
    /// With no argument, go to your home directory, and with "-", go back to the previous one
    #[builtin(shown = [r#"cd("src")"#, r#"cd("..")"#, r#"cd("-")"#, "cd()"], see = ["cwd"], flags = [], command = "cd ../notes")]
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

fn bytes(n: u64) -> Value {
    Value::Qty(n as f64, units::terms("B").expect("bytes are a unit"))
}

/// What a dry run would do, one row per effect; an empty from or to shows as nil.
fn plan_table(plan: Vec<(&str, String, String)>) -> crate::signature::As<Table> {
    let side = |s: String| if s.is_empty() { Value::Nil } else { Value::str(s) };
    let rows = plan.into_iter().map(|(op, from, to)| vec![Value::str(op), side(from), side(to)]).collect();
    Value::table(Table { cols: ["op", "from", "to"].map(String::from).to_vec(), rows }).into()
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

#[cfg(test)]
mod tests {
    use crate::interp::tests::try_eval;

    #[test]
    fn cd() {
        assert!(try_eval(r#"cd("no/such/dir")"#).is_err());
        assert!(try_eval(r#"cd("-")"#).is_err_and(|e| e.msg.contains("no previous directory")));
    }

    #[test]
    fn dry_run() {
        let v = try_eval(r#"mkdir("a", "b", {dry_run: true})"#).unwrap();
        assert_eq!(
            v.to_string(),
            "op,from,to
mkdir,,a
mkdir,,b
"
        );
    }
}
