//! The world outside the script: arguments, environment, shell commands, HTTP.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;
use indexmap::IndexMap;

/// Whether features that go online on their own (currency rates, translation, zip codes) may do so.
pub fn network_allowed() -> bool {
    crate::state::with(|s| s.network.get())
}

pub const MODULE: Module = Module {
    name: "sys",
    about: "script arguments, environment variables, shell commands, clipboard, HTTP GET",
    #[rustfmt::skip]
    examples: &[
        ("sys", &[
            ("arguments after the script", "args()"),
            ("is a variable set?", r#"env("ZIL_SURELY_UNSET") == nil"#),
            ("how many env vars", "env().len > 0"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("scripts", &[
            ("zil script.zil a b  →  args() is [\"a\", \"b\"]", ""),
            ("sh fails on a non-zero exit; run never does", ""),
            ("currency rates, translation and zip codes stay offline until allow_network_access()", ""),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("args", "args()", "command-line arguments after the script (or after `-e code`), as strings", &["args()"], &["env"]),
    doc("env", "env() / env(name: str)", "an environment variable, or nil; with no name, all of them as a map", &[r#"env("ZIL_SURELY_UNSET")"#], &["args"]),
    doc("exit", "exit(code?: int)", "stop the program with an exit code (default 0)", &[], &["args"]).shown(&["exit(1)"]),
    doc("sh", "sh(cmd: str)", "run a shell command (sh -c, or cmd /C on Windows) and return its output, trailing newline trimmed; errors on a non-zero exit", &[], &["run"])
        .shown(&[r#"sh("git rev-parse --short HEAD")"#, r#"sh("ls").lines.len"#]),
    doc("exec", "exec(argv: list, ...)", "run a program, piping each argv into the next, and return its output as a str (bytes if not UTF-8), trailing newline trimmed; errors on a non-zero exit. A map last holds options: stdin (a value fed in), stderr (a file for it), tty (output to the terminal; returns the exit code). `!prog args` desugars to it", &[], &["sh"])
        .shown(&[r#"exec(["git", "log", "--oneline"], ["head", "-3"])"#, "!objdump -d a.out | grep call"]),
    doc("run", "run(cmd: str)", "run a shell command and return {code, out, err}; never fails on the exit code", &[], &["sh"]).shown(&[r#"run("git status").code"#]),
    doc("copy", "copy(v: any)", "put a value on the system clipboard, as print would show it, and return it", &[], &["paste"]).shown(&[r#"sh("git rev-parse HEAD") |> copy"#]),
    doc("paste", "paste()", "the text on the system clipboard", &[], &["copy"]).shown(&["paste().lines.len"]),
    doc("fetch", "fetch(url: str)", "HTTP GET a URL and return the body as a string; errors on a non-2xx status", &[], &["from_json"])
        .shown(&[r#"fetch("https://api.github.com/repos/rust-lang/rust").from_json.stargazers_count"#]),
    doc("allow_network_access", "allow_network_access()", "let currency rates, translation and zip codes go online for the rest of the session; until then they only use cached data", &[], &["fetch"])
        .shown(&["allow_network_access()"]),
];

fn call(it: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("args", []) => Value::list(it.args.iter().map(|a| Value::str(a.as_str())).collect()),
        ("env", []) => Value::map(crate::host::get().env_all().into_iter().map(|(k, v)| (k, Value::str(v))).collect()),
        ("env", [Str(k)]) => crate::host::get().env(k).map_or(Nil, Value::str),
        ("exit", []) => return Err(crate::host::get().exit(0).into()),
        ("exit", [Int(n, _)]) => return Err(crate::host::get().exit(*n as i32).into()),
        ("exec", _) => exec(args)?,
        ("sh" | "run", [Str(cmd)]) => {
            let out = crate::host::get().run(cmd).map_err(|e| format!("cannot run {cmd:?}: {}", super::fs::io_reason(&e)))?;
            let text = |s: &[u8]| String::from_utf8_lossy(s).trim_end_matches(['\n', '\r']).to_string();
            let (out_s, err_s, code) = (text(&out.out), text(&out.err), out.code);
            if name == "run" {
                let m: IndexMap<_, _> = [("code", Value::int(code.into())), ("out", Value::str(out_s)), ("err", Value::str(err_s))]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
                return Ok(Value::map(m));
            }
            if code != 0 {
                return Err(format!("{cmd:?} exited with {code}{}", if err_s.is_empty() { String::new() } else { format!("\n{err_s}") }).into());
            }
            Value::str(out_s)
        }
        ("copy", [v]) => {
            let text = if let Table(t) = v { super::data::tables::grid(t, false) } else { v.to_string() };
            crate::host::get().copy_text(&text).map_err(|e| format!("cannot copy: {}", super::fs::io_reason(&e)))?;
            v.clone()
        }
        ("paste", []) => Value::str(crate::host::get().paste_text().map_err(|e| format!("cannot paste: {}", super::fs::io_reason(&e)))?),
        ("fetch", [Str(url)]) => Value::str(get(url, &[]).map_err(|e| format!("{url}: {e}"))?),
        ("allow_network_access", []) => {
            crate::state::with(|s| s.network.set(true));
            crate::modules::units::retry_rates();
            Nil
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// `exec(["ls", "-l"], ["sort"], {stdin: v, tty: true, stderr: "err.txt"})`, which `!ls -l | sort` desugars to.
fn exec(args: &[Value]) -> Call {
    let (cmds, opts) = match args.split_last() {
        Some((Value::Map(m), rest)) => (rest, m.borrow().clone()),
        _ => (args, IndexMap::new()),
    };
    if let Some(k) = opts.keys().find(|k| !["stdin", "stderr", "tty"].contains(&k.as_str())) {
        return Err(format!("unknown option `{k}`\nnote: exec takes stdin, stderr and tty").into());
    }
    // A list inside an argv is one argument per item, so `!rm {files}` passes each file.
    fn flat(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::List(l) => l.borrow().iter().for_each(|v| flat(v, out)),
            Value::Str(s) => out.push(s.to_string()),
            v => out.push(v.to_string()),
        }
    }
    let mut argvs = vec![];
    for (i, c) in cmds.iter().enumerate() {
        let Value::List(_) = c else { return Err(Fail::Arg(i, format!("expected a list like [\"ls\", \"-l\"], got {}", c.type_name()))) };
        let mut argv = vec![];
        flat(c, &mut argv);
        if argv.is_empty() {
            return Err(Fail::Arg(i, "a command needs a program".into()));
        }
        argvs.push(argv);
    }
    if argvs.is_empty() {
        return Err(Fail::BadArgs);
    }
    let tty = opts.get("tty").is_some_and(Value::truthy);
    let stderr = match opts.get("stderr") {
        None => None,
        Some(Value::Str(p)) => Some(p.clone()),
        Some(v) => return Err(format!("stderr is a file path, got {}", v.type_name()).into()),
    };
    let stdin = opts.get("stdin").map(|v| if let Value::Bytes(b) = v { b.to_vec() } else { super::fs::text(v).into_bytes() });
    let host = crate::host::get();
    let ran = host.exec(&argvs, stdin.as_deref(), tty, !tty || stderr.is_some()).map_err(|e| missing(&e))?;
    if let Some(p) = &stderr {
        host.write(p, &ran.err, false).map_err(|e| format!("cannot write {p:?}: {}", super::fs::io_reason(&e)))?;
    }
    let prog = &argvs[argvs.len() - 1][0];
    if tty {
        if ran.code != 0 {
            let (red, reset) = if crate::ansi::color_on() { ("\x1b[31m", crate::ansi::RESET) } else { ("", "") };
            host.note(&format!("{red}`{prog}` exited with {}{reset}", ran.code));
        }
        return Ok(Value::int(ran.code.into()));
    }
    if ran.code != 0 {
        let err = String::from_utf8_lossy(&ran.err);
        let err = err.trim_end();
        return Err(format!("`{prog}` exited with {}{}", ran.code, if err.is_empty() { String::new() } else { format!("\n{err}") }).into());
    }
    let mut out = ran.out;
    out.truncate(out.strip_suffix(b"\n").map_or(out.len(), |o| o.strip_suffix(b"\r").unwrap_or(o).len()));
    Ok(String::from_utf8(out).map_or_else(|e| Value::bytes(e.into_bytes()), Value::str))
}

/// Why `exec` couldn't start: a missing program gets near names on PATH, and a builtin's name says to drop the `!`.
fn missing(e: &std::io::Error) -> String {
    if e.kind() != std::io::ErrorKind::NotFound {
        return super::fs::io_reason(e);
    }
    let name = e.to_string();
    if crate::modules::FLAGS.iter().any(|f| f.0 == name) {
        return format!("no program `{name}`\nhelp: `{name}` is built in; drop the `!`");
    }
    let host = crate::host::get();
    let path = host.env("PATH").unwrap_or_default();
    let mut progs: Vec<String> = vec![];
    for dir in path.split(if cfg!(windows) { ';' } else { ':' }).filter(|d| !d.is_empty()) {
        for n in host.list(dir).unwrap_or_default() {
            progs.push(if cfg!(windows) { n.rsplit_once('.').map_or(n.clone(), |(stem, _)| stem.to_string()) } else { n });
        }
    }
    let near = crate::error::did_you_mean(&name, progs.iter().map(String::as_str));
    format!("no program `{name}`{near}\nhelp: to negate a value, write `!({name})`")
}

/// `key`'s entry in the host's cache `file`, else `fetch`ed and cached for next time; `fetch` checks `network_allowed` itself.
// ponytail: rereads and rewrites the whole cache file per call; keep it in memory if it grows large.
pub fn cached(file: &str, key: &str, fetch: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let host = crate::host::get();
    let mut cache: serde_json::Map<String, serde_json::Value> = host.cache_get(file).and_then(|(s, _)| serde_json::from_str(&s).ok()).unwrap_or_default();
    if let Some(hit) = cache.get(key).and_then(|v| v.as_str()) {
        return Ok(hit.into());
    }
    let out = fetch()?;
    cache.insert(key.into(), out.clone().into());
    host.cache_put(file, &serde_json::to_string(&cache).unwrap());
    Ok(out)
}

/// HTTP GET `url` with `query` parameters, returning the body.
pub fn get(url: &str, query: &[(&str, &str)]) -> Result<String, String> {
    crate::host::get().http_get(url, query)
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn sys() {
        assert_eq!(show("args()"), "[]");
        assert_eq!(show(r#"env("ZIL_SURELY_UNSET")"#), "nil");
        assert_eq!(show("allow_network_access()"), "nil");
    }
}
