//! Everything a session does to the world outside it goes through its `Host`: output, input, the clock, randomness,
//! the environment, files, processes, the network, the cache and exiting. A method the host doesn't implement is
//! denied, so `NullHost` is a session that touches nothing. Fresh interpreters take the running session's host.

use crate::state;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use std::cell::Cell;
use std::fmt;
use std::io;
use std::rc::Rc;

/// What a session may do outside itself. Output, the clock and randomness need none: the host supplies them, frozen or
/// seeded in a sandbox.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    FilesRead,
    FilesWrite,
    Network,
    Processes,
    Env,
    Cache,
    Stdin,
    Clipboard,
    Exit,
}

impl Permission {
    pub const ALL: [Permission; 9] = {
        use Permission::*;
        [FilesRead, FilesWrite, Network, Processes, Env, Cache, Stdin, Clipboard, Exit]
    };

    pub fn name(self) -> &'static str {
        use Permission::*;
        match self {
            FilesRead => "files.read",
            FilesWrite => "files.write",
            Network => "network",
            Processes => "processes",
            Env => "env",
            Cache => "cache",
            Stdin => "stdin",
            Clipboard => "clipboard",
            Exit => "exit",
        }
    }

    fn bit(self) -> u16 {
        1 << self as u16
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Permission {
    type Err = String;
    fn from_str(s: &str) -> Result<Permission, String> {
        let names = Permission::ALL.map(Permission::name);
        Permission::ALL.into_iter().find(|p| p.name() == s).ok_or_else(|| format!("no permission `{s}`{}", crate::error::did_you_mean(s, names)))
    }
}

/// A set of permissions, as bits.
pub(crate) fn mask(ps: impl IntoIterator<Item = Permission>) -> u16 {
    ps.into_iter().fold(0, |m, p| m | p.bit())
}

/// What a file or directory looks like, for `ls`, `file_size` and friends.
pub struct Meta {
    pub is_dir: bool,
    pub len: u64,
    pub modified: Timestamp,
}

/// A finished program: its exit code (-1 if killed) and output.
pub struct Ran {
    pub code: i32,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
}

fn denied<T>(what: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{what} is disabled in this session")))
}

pub trait Host {
    /// Whether the host lets sessions use `p` at all; a session can only narrow this. Methods the host doesn't
    /// implement are denied either way.
    fn permits(&self, _p: Permission) -> bool {
        true
    }
    /// A line of program output, from `print` and friends.
    fn print(&self, _text: &str) {}
    /// Long output from `page`, which a terminal can scroll through.
    fn page(&self, text: &str) {
        self.print(text)
    }
    /// A side note for the user that isn't program output, like a stale cache warning.
    fn note(&self, _text: &str) {}
    /// Columns and rows when output goes to a terminal that understands ANSI escapes.
    fn terminal(&self) -> Option<(usize, usize)> {
        None
    }
    /// Lets the user edit `text` full-screen, for `edit`; returns what they saved.
    fn edit(&self, _text: &str) -> io::Result<String> {
        denied("editing")
    }
    /// Pauses between `animate` frames.
    fn sleep(&self, _secs: f64) {}
    /// All of stdin, for `input`.
    fn stdin(&self) -> io::Result<String> {
        denied("reading input")
    }
    fn now(&self) -> Timestamp {
        Timestamp::UNIX_EPOCH
    }
    fn time_zone(&self) -> TimeZone {
        TimeZone::UTC
    }
    /// Seeds the session's random numbers once, when it starts.
    fn seed(&self) -> u64 {
        0
    }
    fn env(&self, _name: &str) -> Option<String> {
        None
    }
    fn env_all(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn read(&self, _path: &str) -> io::Result<Vec<u8>> {
        denied("file access")
    }
    fn write(&self, _path: &str, _bytes: &[u8], _append: bool) -> io::Result<()> {
        denied("file access")
    }
    /// The names in a directory, in any order.
    fn list(&self, _dir: &str) -> io::Result<Vec<String>> {
        denied("file access")
    }
    /// Follows symlinks, but describes a broken one itself.
    fn metadata(&self, _path: &str) -> io::Result<Meta> {
        denied("file access")
    }
    /// A file or an empty directory.
    fn remove(&self, _path: &str) -> io::Result<()> {
        denied("file access")
    }
    /// A file, or a directory and everything in it; a link is removed, never followed.
    fn remove_all(&self, _path: &str) -> io::Result<()> {
        denied("file access")
    }
    fn rename(&self, _from: &str, _to: &str) -> io::Result<()> {
        denied("file access")
    }
    fn copy(&self, _from: &str, _to: &str) -> io::Result<()> {
        denied("file access")
    }
    /// A directory and any missing parents.
    fn mkdir(&self, _path: &str) -> io::Result<()> {
        denied("file access")
    }
    fn cwd(&self) -> io::Result<String> {
        denied("file access")
    }
    fn cd(&self, _dir: &str) -> io::Result<()> {
        denied("file access")
    }
    /// Runs `cmd` in the system shell.
    fn run(&self, _cmd: &str) -> io::Result<Ran> {
        denied("running commands")
    }
    /// Runs `cmds`, each `[program, args...]`, as a pipeline: the first reads `stdin` (else the terminal) and each feeds
    /// the next through an OS pipe. With `tty` the last writes to the terminal, else its output is captured; `err`
    /// captures its stderr too. A missing program is a `NotFound` error whose message is its name.
    fn exec(&self, _cmds: &[Vec<String>], _stdin: Option<&[u8]>, _tty: bool, _err: bool) -> io::Result<Ran> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "external programs need desktop zil"))
    }
    /// Puts `text` on the system clipboard.
    fn copy_text(&self, _text: &str) -> io::Result<()> {
        denied("clipboard access")
    }
    /// What's on the system clipboard, as text.
    fn paste_text(&self) -> io::Result<String> {
        denied("clipboard access")
    }
    /// HTTP GET `url` with `query` parameters, returning the body.
    fn http_get(&self, _url: &str, _query: &[(&str, &str)]) -> Result<String, String> {
        Err("network access is disabled in this session".into())
    }
    /// A cache entry and when it was put there.
    fn cache_get(&self, _key: &str) -> Option<(String, Timestamp)> {
        None
    }
    fn cache_put(&self, _key: &str, _value: &str) {}
    /// Ends the process; returns only if it can't, with why.
    fn exit(&self, _code: i32) -> String {
        "exit is disabled in this session".into()
    }
}

/// Denies everything: no output, a clock stuck at 1970 UTC, and the same random numbers every time.
pub struct NullHost;
impl Host for NullHost {
    fn permits(&self, _p: Permission) -> bool {
        false
    }
}

/// The host a session calls: its own, behind the session's permissions.
pub(crate) struct Gate {
    host: Rc<dyn Host>,
    /// What the embedder allows; `config.permissions` can't go past it or the host's own.
    pub policy: Cell<u16>,
    /// What running code may use now: the policy, or what `config.permissions` declared.
    pub grants: Cell<u16>,
}

impl Gate {
    pub fn new(host: Rc<dyn Host>) -> Gate {
        let all = mask(Permission::ALL);
        Gate { host, policy: Cell::new(all), grants: Cell::new(all) }
    }

    /// Whether `config.permissions` may grant `p`.
    pub fn allows(&self, p: Permission) -> bool {
        self.policy.get() & p.bit() != 0 && self.host.permits(p)
    }

    pub fn check(&self, p: Permission) -> Result<(), String> {
        if !self.allows(p) {
            Err(format!("{p} is denied in this session"))
        } else if self.grants.get() & p.bit() == 0 {
            Err(format!("{p} isn't in config.permissions"))
        } else {
            Ok(())
        }
    }

    fn need(&self, p: Permission) -> io::Result<()> {
        self.check(p).map_err(|m| io::Error::new(io::ErrorKind::PermissionDenied, m))
    }

    /// The granted permissions, in `Permission::ALL` order.
    pub fn granted(&self) -> Vec<Permission> {
        Permission::ALL.into_iter().filter(|&p| self.check(p).is_ok()).collect()
    }
}

impl Host for Gate {
    fn permits(&self, p: Permission) -> bool {
        self.check(p).is_ok()
    }
    fn print(&self, text: &str) {
        self.host.print(text)
    }
    fn page(&self, text: &str) {
        self.host.page(text)
    }
    fn note(&self, text: &str) {
        self.host.note(text)
    }
    fn terminal(&self) -> Option<(usize, usize)> {
        self.host.terminal()
    }
    fn edit(&self, text: &str) -> io::Result<String> {
        self.need(Permission::Stdin)?;
        self.host.edit(text)
    }
    fn sleep(&self, secs: f64) {
        self.host.sleep(secs)
    }
    fn stdin(&self) -> io::Result<String> {
        self.need(Permission::Stdin)?;
        self.host.stdin()
    }
    fn now(&self) -> Timestamp {
        self.host.now()
    }
    fn time_zone(&self) -> TimeZone {
        self.host.time_zone()
    }
    fn seed(&self) -> u64 {
        self.host.seed()
    }
    fn env(&self, name: &str) -> Option<String> {
        self.permits(Permission::Env).then(|| self.host.env(name)).flatten()
    }
    fn env_all(&self) -> Vec<(String, String)> {
        if self.permits(Permission::Env) { self.host.env_all() } else { vec![] }
    }
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.need(Permission::FilesRead)?;
        self.host.read(path)
    }
    fn write(&self, path: &str, bytes: &[u8], append: bool) -> io::Result<()> {
        self.need(Permission::FilesWrite)?;
        self.host.write(path, bytes, append)
    }
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        self.need(Permission::FilesRead)?;
        self.host.list(dir)
    }
    fn metadata(&self, path: &str) -> io::Result<Meta> {
        self.need(Permission::FilesRead)?;
        self.host.metadata(path)
    }
    fn remove(&self, path: &str) -> io::Result<()> {
        self.need(Permission::FilesWrite)?;
        self.host.remove(path)
    }
    fn remove_all(&self, path: &str) -> io::Result<()> {
        self.need(Permission::FilesWrite)?;
        self.host.remove_all(path)
    }
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.need(Permission::FilesWrite)?;
        self.host.rename(from, to)
    }
    fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        self.need(Permission::FilesRead)?;
        self.need(Permission::FilesWrite)?;
        self.host.copy(from, to)
    }
    fn mkdir(&self, path: &str) -> io::Result<()> {
        self.need(Permission::FilesWrite)?;
        self.host.mkdir(path)
    }
    fn cwd(&self) -> io::Result<String> {
        self.need(Permission::FilesRead)?;
        self.host.cwd()
    }
    fn cd(&self, dir: &str) -> io::Result<()> {
        self.need(Permission::FilesRead)?;
        self.host.cd(dir)
    }
    fn run(&self, cmd: &str) -> io::Result<Ran> {
        self.need(Permission::Processes)?;
        self.host.run(cmd)
    }
    fn exec(&self, cmds: &[Vec<String>], stdin: Option<&[u8]>, tty: bool, err: bool) -> io::Result<Ran> {
        self.need(Permission::Processes)?;
        self.host.exec(cmds, stdin, tty, err)
    }
    fn copy_text(&self, text: &str) -> io::Result<()> {
        self.need(Permission::Clipboard)?;
        self.host.copy_text(text)
    }
    fn paste_text(&self) -> io::Result<String> {
        self.need(Permission::Clipboard)?;
        self.host.paste_text()
    }
    fn http_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, String> {
        self.check(Permission::Network)?;
        self.host.http_get(url, query)
    }
    fn cache_get(&self, key: &str) -> Option<(String, Timestamp)> {
        self.host.cache_get(key)
    }
    fn cache_put(&self, key: &str, value: &str) {
        if self.permits(Permission::Cache) {
            self.host.cache_put(key, value)
        }
    }
    fn exit(&self, code: i32) -> String {
        match self.check(Permission::Exit) {
            Ok(()) => self.host.exit(code),
            Err(e) => e,
        }
    }
}

/// Errors unless the running session may use `p`, for builtins whose host call can't fail, like `env`.
pub fn need(p: Permission) -> Result<(), String> {
    state::with(|s| s.gate.check(p))
}

/// The host of a thread's first session: nothing until the embedder installs its own with `set_default`, as the CLI and
/// `zil-wasm` do. The tests get `TestHost`.
pub fn default() -> Rc<dyn Host> {
    #[cfg(test)]
    return Rc::new(TestHost);
    #[cfg(not(test))]
    return Rc::new(NullHost);
}

pub use crate::state::set_default_host as set_default;

/// The running session's host.
pub fn get() -> Rc<dyn Host> {
    state::with(|s| s.host.clone())
}

/// The time now, in the host's time zone.
pub fn now() -> Zoned {
    let h = get();
    h.now().to_zoned(h.time_zone())
}

pub fn timestamp() -> Timestamp {
    get().now()
}

pub fn tz() -> TimeZone {
    get().time_zone()
}

/// The running session's random numbers, seeded by its host.
pub fn rng<T>(f: impl FnOnce(&mut fastrand::Rng) -> T) -> T {
    state::with(|s| f(&mut s.rng.borrow_mut()))
}

/// What `write_file` writes: lists one item per line, so they round-trip with `.lines`.
pub fn text(v: &crate::value::Value) -> String {
    match v {
        crate::value::Value::List(l) => l.borrow().iter().map(|x| format!("{x}\n")).collect(),
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

/// `key`'s entry in the host's cache `file`, else `fetch`ed and cached for next time.
// ponytail: rereads and rewrites the whole cache file per call; keep it in memory if it grows large.
pub fn cached(file: &str, key: &str, fetch: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let host = get();
    let mut cache: serde_json::Map<String, serde_json::Value> = host.cache_get(file).and_then(|(s, _)| serde_json::from_str(&s).ok()).unwrap_or_default();
    if let Some(hit) = cache.get(key).and_then(|v| v.as_str()) {
        return Ok(hit.into());
    }
    let out = fetch()?;
    cache.insert(key.into(), out.clone().into());
    host.cache_put(file, &serde_json::Value::Object(cache).to_string());
    Ok(out)
}

/// The tests run at a fixed moment in Chicago, whatever the machine's clock and zone.
#[cfg(test)]
pub struct TestHost;

#[cfg(test)]
impl Host for TestHost {
    fn now(&self) -> Timestamp {
        "2026-10-06T14:30:00Z".parse().unwrap()
    }
    fn time_zone(&self) -> TimeZone {
        TimeZone::get("America/Chicago").unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::NullHost;
    use crate::interp::Interp;
    use crate::run;
    use std::rc::Rc;

    #[test]
    fn null_host_touches_nothing() {
        let mut it = Interp::with_host(Rc::new(NullHost));
        let mut show = |s| run(&mut it, s).map(|v| v.to_string()).unwrap_or_else(|e| e.msg);
        assert_eq!(show("now.year"), "1970");
        #[cfg(feature = "fs")]
        assert_eq!(show(r#"read_file("Cargo.toml")"#), r#"read_file: cannot read "Cargo.toml": files.read is denied in this session"#);
        #[cfg(feature = "sys")]
        {
            assert_eq!(show(r#"env("PATH")"#), "env: env is denied in this session");
            assert!(show(r#"sh("echo hi")"#).contains("processes is denied"));
            assert!(show(r#"copy("x")"#).contains("clipboard is denied"));
            assert!(show("paste()").contains("clipboard is denied"));
        }
        #[cfg(feature = "art")]
        assert!(show(r#"img("x.png")"#).contains("files.read is denied"));
        assert_eq!(show("config.permissions"), "[]");
        assert_eq!(show(r#"config.permissions = ["network"]"#), "network is denied in this session");
        let a = show("[rand(), rand()]");
        assert_eq!(run(&mut Interp::with_host(Rc::new(NullHost)), "[rand(), rand()]").unwrap().to_string(), a, "seed 0 repeats");
    }

    #[test]
    fn permissions() {
        let mut it = Interp::new();
        let mut show = |s| run(&mut it, s).map(|v| v.to_string()).unwrap_or_else(|e| e.msg);
        assert_eq!(show(r#"config.permissions = ["env", "network"]; config.permissions"#), r#"["network", "env"]"#);
        #[cfg(feature = "sys")]
        {
            assert_eq!(show(r#"env("ZIL_SURELY_UNSET")"#), "nil");
            assert_eq!(show(r#"sh("echo hi")"#), r#"sh: cannot run "echo hi": processes isn't in config.permissions"#);
            assert_eq!(show(r#"config.permissions = []; env("PATH")"#), "env: env isn't in config.permissions");
        }
        assert_eq!(show(r#"config.permissions = ["files.raed"]"#), "no permission `files.raed`");
        assert_eq!(show("config.permissions = 1"), r#"permissions is a list of names like "files.read", got `1`"#);
        let mut s = crate::Session::builder().host(Rc::new(super::TestHost)).permissions([super::Permission::Env]).build().unwrap();
        assert_eq!(s.eval(r#"config.permissions = ["env", "cache"]"#).unwrap_err().msg, "cache is denied in this session");
        assert!(s.eval(r#"config.permissions = []; config.permissions = ["env"]"#).is_ok(), "within the policy, a script can grant again");
        s.as_script();
        assert_eq!(s.eval("config.permissions").unwrap().to_string(), "[]");
    }
}
