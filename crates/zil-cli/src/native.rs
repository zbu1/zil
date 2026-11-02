//! The CLI's host: the real machine.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use zil::host::{Host, Meta, Permission, Ran};

/// The machine zil runs on, with nothing held back but `denied` and the folders in `scopes`; `--now`, `--tz` and
/// `--seed` pin its clock, zone and randomness.
#[derive(Default)]
pub struct NativeHost {
    pub now: Option<Timestamp>,
    pub tz: Option<TimeZone>,
    pub seed: Option<u64>,
    pub denied: Vec<Permission>,
    /// Folders `files.read` or `files.write` is limited to; a permission with none reaches everywhere.
    pub scopes: Vec<(Permission, PathBuf)>,
}

impl NativeHost {
    /// Allows or denies `spec`: a permission name, or for `allow` a files permission and a folder it's limited to, like
    /// `files.read:./data` (folders add up).
    pub fn grant(&mut self, spec: &str, allow: bool) -> Result<(), String> {
        let (name, dir) = spec.split_once(':').map_or((spec, None), |(n, d)| (n, Some(d)));
        let p: Permission = name.trim().parse()?;
        self.denied.retain(|&d| d != p);
        match dir {
            None => {
                self.scopes.retain(|(s, _)| *s != p);
                if !allow {
                    self.denied.push(p)
                }
            }
            Some(_) if !allow => return Err(format!("{spec}: only --allow takes a folder")),
            Some(_) if !matches!(p, Permission::FilesRead | Permission::FilesWrite) => {
                return Err(format!("{spec}: only files.read and files.write take a folder"));
            }
            Some(d) => self.scopes.push((p, std::fs::canonicalize(d).map_err(|e| format!("{d}: {e}"))?)),
        }
        Ok(())
    }

    /// Errors unless `path` is in one of `p`'s folders, when it has any.
    fn inside(&self, p: Permission, path: &str) -> io::Result<()> {
        let mut dirs = self.scopes.iter().filter(|(s, _)| *s == p).map(|(_, d)| d).peekable();
        if dirs.peek().is_none() || resolve(Path::new(path)).is_some_and(|full| dirs.any(|d| full.starts_with(d))) {
            return Ok(());
        }
        Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("it's outside the folders --allow gives {p}")))
    }
}

/// `path` with links and `..` resolved, for one that may not exist yet: its nearest existing ancestor, resolved, plus
/// the rest; `None` if the rest has a `..`.
fn resolve(path: &Path) -> Option<PathBuf> {
    let at = if path.as_os_str().is_empty() { Path::new(".") } else { path };
    match std::fs::canonicalize(at) {
        Ok(full) => Some(full),
        Err(_) => Some(resolve(path.parent()?)?.join(path.file_name()?)),
    }
}

impl Host for NativeHost {
    fn permits(&self, p: Permission) -> bool {
        !self.denied.contains(&p)
    }
    fn print(&self, text: &str) {
        println!("{text}")
    }
    fn page(&self, text: &str) {
        crate::pager::page(text)
    }
    fn edit(&self, text: &str) -> io::Result<String> {
        crate::editor::edit(text)
    }
    fn note(&self, text: &str) {
        eprintln!("{text}")
    }
    fn terminal(&self) -> Option<(usize, usize)> {
        let (w, h) = terminal_size::terminal_size()?;
        io::stdout().is_terminal().then_some((w.0 as usize, h.0 as usize))
    }
    fn sleep(&self, secs: f64) {
        std::thread::sleep(std::time::Duration::from_secs_f64(secs))
    }
    fn stdin(&self) -> io::Result<String> {
        io::read_to_string(io::stdin())
    }
    fn now(&self) -> Timestamp {
        self.now.unwrap_or_else(Timestamp::now)
    }
    fn time_zone(&self) -> TimeZone {
        self.tz.clone().unwrap_or_else(TimeZone::system)
    }
    fn seed(&self) -> u64 {
        self.seed.unwrap_or_else(|| fastrand::u64(..))
    }
    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn env_all(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.inside(Permission::FilesRead, path)?;
        std::fs::read(path)
    }
    fn write(&self, path: &str, bytes: &[u8], append: bool) -> io::Result<()> {
        self.inside(Permission::FilesWrite, path)?;
        std::fs::OpenOptions::new().write(true).create(true).append(append).truncate(!append).open(path)?.write_all(bytes)
    }
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        self.inside(Permission::FilesRead, dir)?;
        std::fs::read_dir(dir)?.map(|e| Ok(e?.file_name().to_string_lossy().into_owned())).collect()
    }
    fn metadata(&self, path: &str) -> io::Result<Meta> {
        self.inside(Permission::FilesRead, path)?;
        let md = std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path))?;
        let modified = Timestamp::try_from(md.modified()?).map_err(io::Error::other)?;
        Ok(Meta { is_dir: md.is_dir(), len: md.len(), modified })
    }
    fn remove(&self, path: &str) -> io::Result<()> {
        self.inside(Permission::FilesWrite, path)?;
        if std::path::Path::new(path).is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
    }
    fn remove_all(&self, path: &str) -> io::Result<()> {
        self.inside(Permission::FilesWrite, path)?;
        if std::path::Path::new(path).is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
    }
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.inside(Permission::FilesWrite, from)?;
        self.inside(Permission::FilesWrite, to)?;
        std::fs::rename(from, to)
    }
    fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        self.inside(Permission::FilesRead, from)?;
        self.inside(Permission::FilesWrite, to)?;
        std::fs::copy(from, to).map(drop)
    }
    fn mkdir(&self, path: &str) -> io::Result<()> {
        self.inside(Permission::FilesWrite, path)?;
        std::fs::create_dir_all(path)
    }
    fn cwd(&self) -> io::Result<String> {
        Ok(std::env::current_dir()?.to_string_lossy().into_owned())
    }
    fn cd(&self, dir: &str) -> io::Result<()> {
        self.inside(Permission::FilesRead, dir)?;
        std::env::set_current_dir(dir)
    }
    fn run(&self, cmd: &str) -> io::Result<Ran> {
        let (sh, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
        let out = std::process::Command::new(sh).args([flag, cmd]).output()?;
        Ok(Ran { code: out.status.code().unwrap_or(-1), out: out.stdout, err: out.stderr })
    }
    fn exec(&self, cmds: &[Vec<String>], stdin: Option<&[u8]>, tty: bool, err: bool) -> io::Result<Ran> {
        use std::process::{Child, Command, Stdio};
        let (mut prev, mut kids): (Option<Stdio>, Vec<Child>) = (stdin.map(|_| Stdio::piped()), vec![]);
        for (i, argv) in cmds.iter().enumerate() {
            let last = i == cmds.len() - 1;
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]).stdin(prev.take().unwrap_or(Stdio::inherit()));
            if !last || !tty {
                c.stdout(Stdio::piped());
            }
            if last && err {
                c.stderr(Stdio::piped());
            }
            let mut kid = match c.spawn() {
                Ok(k) => k,
                Err(e) => {
                    for mut k in kids {
                        let _ = (k.kill(), k.wait());
                    }
                    return Err(if e.kind() == io::ErrorKind::NotFound { io::Error::new(e.kind(), argv[0].clone()) } else { e });
                }
            };
            if !last {
                prev = kid.stdout.take().map(Stdio::from);
            }
            kids.push(kid);
        }
        // Fed from a thread, so a program that writes before it reads everything can't deadlock against us.
        let feed = stdin.map(|b| {
            let (mut w, b) = (kids[0].stdin.take().unwrap(), b.to_vec());
            std::thread::spawn(move || drop(w.write_all(&b)))
        });
        let out = kids.pop().unwrap().wait_with_output()?;
        for mut k in kids {
            k.wait()?;
        }
        feed.map(|t| t.join());
        Ok(Ran { code: out.status.code().unwrap_or(-1), out: out.stdout, err: out.stderr })
    }
    fn open(&self, path: &str) -> io::Result<()> {
        use std::process::Command;
        let mut c = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        } else {
            Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
        };
        c.arg(path).spawn().map(drop)
    }
    fn copy_text(&self, text: &str) -> io::Result<()> {
        // clip.exe reads BOM-less input in the console code page (mangling non-ASCII) and keeps a BOM in the copied text.
        if cfg!(windows) {
            let ps = "[Console]::InputEncoding = [Text.Encoding]::UTF8; Set-Clipboard -Value ([Console]::In.ReadToEnd())";
            clip_tool(&[&["powershell", "-NoProfile", "-NonInteractive", "-Command", ps]], text.as_bytes()).map(drop)
        } else if cfg!(target_os = "macos") {
            clip_tool(&[&["pbcopy"]], text.as_bytes()).map(drop)
        } else {
            clip_tool(&[&["wl-copy"], &["xclip", "-selection", "clipboard"], &["xsel", "--clipboard", "--input"]], text.as_bytes()).map(drop)
        }
    }
    fn paste_text(&self) -> io::Result<String> {
        let out = if cfg!(windows) {
            let ps = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; [Console]::Out.Write((Get-Clipboard -Raw))";
            clip_tool(&[&["powershell", "-NoProfile", "-NonInteractive", "-Command", ps]], b"")?
        } else if cfg!(target_os = "macos") {
            clip_tool(&[&["pbpaste"]], b"")?
        } else {
            clip_tool(&[&["wl-paste", "--no-newline"], &["xclip", "-selection", "clipboard", "-o"], &["xsel", "--clipboard", "--output"]], b"")?
        };
        Ok(String::from_utf8_lossy(&out).replace("\r\n", "\n"))
    }
    fn http_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, String> {
        ureq::get(url).query_pairs(query.iter().copied()).call().and_then(|mut r| r.body_mut().read_to_string()).map_err(|e| e.to_string())
    }
    fn cache_get(&self, key: &str) -> Option<(String, Timestamp)> {
        let path = zil::cache_dir()?.join(key);
        let at = Timestamp::try_from(path.metadata().ok()?.modified().ok()?).ok()?;
        Some((std::fs::read_to_string(path).ok()?, at))
    }
    fn cache_put(&self, key: &str, value: &str) {
        if let Some(dir) = zil::cache_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join(key), value);
        }
    }
    fn exit(&self, code: i32) -> String {
        std::process::exit(code)
    }
}

/// Runs the first of `tools` that exists with `input` on its stdin, returning its stdout.
fn clip_tool(tools: &[&[&str]], input: &[u8]) -> io::Result<Vec<u8>> {
    use std::process::{Command, Stdio};
    for tool in tools {
        let Ok(mut child) = Command::new(tool[0]).args(&tool[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
            continue;
        };
        child.stdin.take().unwrap().write_all(input)?;
        let out = child.wait_with_output()?;
        return if out.status.success() { Ok(out.stdout) } else { Err(io::Error::other(format!("{} failed", tool[0]))) };
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "no clipboard tool found (install wl-clipboard, xclip or xsel)"))
}

#[cfg(test)]
mod tests {
    use super::NativeHost;
    use std::rc::Rc;

    fn show(src: &str) -> String {
        let mut s = zil::Session::new(Rc::new(NativeHost::default()));
        s.eval(src).map(|v| v.to_string()).unwrap_or_else(|e| e.msg)
    }

    #[test]
    fn sys() {
        assert_eq!(show(r#"sh("echo hi")"#), "hi");
        assert_eq!(show(r#"run("exit 3").code"#), "3");
        assert!(show(r#"sh("exit 3")"#).starts_with("sh:"));
    }

    #[test]
    fn files() {
        let dir = std::env::temp_dir().join(format!("zil-fs-{}", std::process::id()));
        let d = dir.to_string_lossy().replace('\\', "/");
        let run = |s: &str| show(&format!("d = \"{d}\"; {s}"));
        run(r#"mkdir(d + "/sub/deep"); write_file(["x", "y"], d + "/a.txt"); append_file("z", d + "/a.txt"); write_file("", d + "/sub/deep/b.rs")"#);
        assert_eq!(run(r#"read_file(d + "/a.txt")"#), "x\ny\nz");
        run(r#"write_file(b"\x7fELF", d + "/a.bin"); append_file(b"\xff", d + "/a.bin")"#);
        assert_eq!(run(r#"read_bytes(d + "/a.bin")"#), r#"b"\x7fELF\xff""#);
        assert!(run(r#"read_file(d + "/a.bin")"#).contains("binary file; use `xxd` or `read_bytes`"));
        run(r#"rm(d + "/a.bin")"#);
        assert_eq!(run(r#"[exists(d + "/a.txt"), is_dir(d + "/sub"), file_size(d + "/a.txt"), type(mtime(d + "/a.txt"))]"#), r#"[true, true, 5 B, "date"]"#);
        assert_eq!(run("ls(d).select(\"name\", \"type\") to list"), r#"[{name: "a.txt", type: "file"}, {name: "sub", type: "dir"}]"#);
        assert_eq!(run(r#"glob(d + "/**/*.rs").name.map(|p| p.basename)"#), r#"["b.rs"]"#);
        assert_eq!(run(r#"glob(d + "/*").len"#), "2");
        assert_eq!(run(r#"glob(d + "/s?b/*").name.map(|p| p.basename)"#), r#"["deep"]"#);
        run(r#"cp(d + "/a.txt", d + "/c.txt"); mv(d + "/c.txt", d + "/e.txt"); rm(d + "/a.txt")"#);
        assert_eq!(run("ls(d).name"), r#"["e.txt", "sub"]"#);
        assert!(show(&format!(r#"rm("{d}/sub")"#)).starts_with("rm:"), "rm refuses a non-empty directory");
        run(r#"mv(d + "/e.txt", d + "/new/deeper/e.txt")"#);
        assert_eq!(run(r#"ls(d + "/new/deeper").name"#), r#"["e.txt"]"#);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scopes() {
        let dir = std::env::temp_dir().join(format!("zil-scope-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("in")).unwrap();
        let mut host = NativeHost::default();
        host.grant(&format!("files.write:{}", dir.join("in").display()), true).unwrap();
        let d = dir.to_string_lossy().replace('\\', "/");
        let mut s = zil::Session::new(Rc::new(host));
        let mut run = |src: &str| s.eval(&format!("d = \"{d}\"\n{src}")).map(|v| v.to_string()).unwrap_or_else(|e| e.msg);
        assert_eq!(run(r#"mkdir(d + "/in/new"); write_file("x", d + "/in/new/a.txt"); read_file(d + "/in/new/a.txt")"#), "x");
        assert!(run(r#"write_file("x", d + "/out.txt")"#).ends_with("it's outside the folders --allow gives files.write"));
        assert!(run(r#"write_file("x", d + "/in/no/../../out.txt")"#).contains("outside"));
        assert!(run(r#"mv(d + "/in/new/a.txt", d + "/a.txt")"#).contains("outside"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commands() {
        let dir = std::env::temp_dir().join(format!("zil-cmd-{}", std::process::id()));
        let d = dir.to_string_lossy().replace('\\', "/");
        let run = |s: &str| show(&format!("d = \"{d}\"\n{s}"));
        run(r#"mkdir "{d}/a" "{d}/b/c""#);
        run(r#"write_file("one fish\nTwo Fish\nred\n", d + "/a/x.txt"); write_file("", d + "/.hidden")"#);
        assert_eq!(run(r#"ls {d} |> len"#), "2");
        assert_eq!(run(r#"ls -la {d} |> len"#), "3");
        assert_eq!(run(r#"grep -ic fish "{d}/a/x.txt""#), "2");
        assert_eq!(run(r#"grep -v fish "{d}/a/x.txt""#), r#"["Two Fish", "red"]"#);
        assert_eq!(run(r#"wc "{d}/a/x.txt""#), "{lines: 3, words: 5, bytes: 22}");
        assert_eq!(run(r#"wc -l "{d}/a/x.txt" "{d}/a/x.txt""#), "6");
        assert_eq!(run(r#"cat "{d}/a/x.txt" "{d}/a/x.txt" |> len"#), "44");
        assert!(run(r#"cp "{d}/a" "{d}/b""#).contains("it's a directory"));
        assert!(run(r#"cp -r "{d}/a" "{d}/a/in""#).contains("into itself"));
        run(r#"cp -r "{d}/a" "{d}/b""#);
        assert_eq!(run(r#"exists(d + "/b/a/x.txt")"#), "true");
        run(r#"mv "{d}/b/a/x.txt" "{d}/.hidden" "{d}/b/c""#);
        assert_eq!(run(r#"ls -a "{d}/b/c" |> len"#), "2");
        assert!(run(r#"mv "{d}/a/x.txt" "{d}/a/x.txt" "{d}/nope""#).contains("not a directory"));
        assert!(run(r#"rm "{d}/b""#).starts_with("rm: cannot remove"));
        assert_eq!(run(r#"rm -f "{d}/missing""#), "nil");
        assert!(run(r#"ls -z {d}"#).contains("unknown flag `z`"));
        run(r#"rm -r {d}"#);
        assert!(!dir.exists());
    }

    #[test]
    fn externals() {
        let dir = std::env::temp_dir().join(format!("zil-ext-{}", std::process::id()));
        let d = dir.to_string_lossy().replace('\\', "/");
        let run = |s: &str| show(&format!("d = \"{d}\"\n{s}"));
        run(r#"mkdir {d}; write_file("hi\n", d + "/hi.txt")"#);
        assert!(run("x = !git --version").starts_with("git version"));
        assert_eq!(run("x = !git --version | grep version |> len"), "1");
        assert_eq!(run(r#"x = !git hash-object --stdin < "{d}/hi.txt""#), "45b983be36b73c0788dc9cbcb76cbb80fc7bb057");
        assert_eq!(run("x = !git --version | git hash-object --stdin |> len"), "40", "an OS pipe between programs");
        assert_eq!(run(r#"exec(["git", "hash-object", "--stdin"], {stdin: "hi\n"})"#), "45b983be36b73c0788dc9cbcb76cbb80fc7bb057");
        assert_eq!(run("!git --version"), "0", "a bare statement goes to the terminal and returns the exit code");
        run(r#"!git --version > "{d}/v.txt""#);
        assert!(run(r#"read_file(d + "/v.txt")"#).starts_with("git version"));
        assert!(run(r#"x = !git nosuchcmd 2> "{d}/e.txt""#).contains("`git` exited with 1"));
        assert!(run(r#"read_file(d + "/e.txt")"#).contains("nosuchcmd"));
        assert!(run("x = !zil_surely_missing").contains("no program `zil_surely_missing`"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
