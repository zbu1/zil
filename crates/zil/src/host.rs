//! Everything a session does to the world outside it goes through its `Host`: output, input, the clock, randomness,
//! the environment, files, processes, the network, the cache and exiting. A method the host doesn't implement is
//! denied, so `NullHost` is a session that touches nothing. Fresh interpreters take the running session's host.

use crate::state;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use std::io;
use std::rc::Rc;

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
impl Host for NullHost {}

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
        assert_eq!(show(r#"read_file("Cargo.toml")"#), r#"read_file: cannot read "Cargo.toml": file access is disabled in this session"#);
        assert_eq!(show(r#"env("PATH")"#), "nil");
        assert!(show(r#"sh("echo hi")"#).contains("running commands is disabled"));
        assert!(show(r#"img("x.png")"#).contains("file access is disabled"));
        assert!(show(r#"copy("x")"#).contains("clipboard access is disabled"));
        assert!(show("paste()").contains("clipboard access is disabled"));
        let a = show("[rand(), rand()]");
        assert_eq!(run(&mut Interp::with_host(Rc::new(NullHost)), "[rand(), rand()]").unwrap().to_string(), a, "seed 0 repeats");
    }
}
