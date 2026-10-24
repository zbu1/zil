//! The CLI's host: the real machine.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use std::io::{self, IsTerminal, Write};
use zil::host::{Host, Meta, Ran};

/// The machine zil runs on, with nothing held back.
pub struct NativeHost;

impl Host for NativeHost {
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
        Timestamp::now()
    }
    fn time_zone(&self) -> TimeZone {
        TimeZone::system()
    }
    fn seed(&self) -> u64 {
        fastrand::u64(..)
    }
    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn env_all(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn write(&self, path: &str, bytes: &[u8], append: bool) -> io::Result<()> {
        std::fs::OpenOptions::new().write(true).create(true).append(append).truncate(!append).open(path)?.write_all(bytes)
    }
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        std::fs::read_dir(dir)?.map(|e| Ok(e?.file_name().to_string_lossy().into_owned())).collect()
    }
    fn metadata(&self, path: &str) -> io::Result<Meta> {
        let md = std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path))?;
        let modified = Timestamp::try_from(md.modified()?).map_err(io::Error::other)?;
        Ok(Meta { is_dir: md.is_dir(), len: md.len(), modified })
    }
    fn remove(&self, path: &str) -> io::Result<()> {
        if std::path::Path::new(path).is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
    }
    fn remove_all(&self, path: &str) -> io::Result<()> {
        if std::path::Path::new(path).is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
    }
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        std::fs::copy(from, to).map(drop)
    }
    fn mkdir(&self, path: &str) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn cwd(&self) -> io::Result<String> {
        Ok(std::env::current_dir()?.to_string_lossy().into_owned())
    }
    fn cd(&self, dir: &str) -> io::Result<()> {
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
        let mut s = zil::Session::new(Rc::new(NativeHost));
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
        run(r#"mkdir(d + "/sub/deep"); write_file(d + "/a.txt", ["x", "y"]); append_file(d + "/a.txt", "z"); write_file(d + "/sub/deep/b.rs", "")"#);
        assert_eq!(run(r#"read_file(d + "/a.txt")"#), "x\ny\nz");
        run(r#"write_file(d + "/a.bin", b"\x7fELF"); append_file(d + "/a.bin", b"\xff")"#);
        assert_eq!(run(r#"read_bytes(d + "/a.bin")"#), r#"b"\x7fELF\xff""#);
        assert!(run(r#"read_file(d + "/a.bin")"#).contains("binary file; use `read_bytes`"));
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
    fn commands() {
        let dir = std::env::temp_dir().join(format!("zil-cmd-{}", std::process::id()));
        let d = dir.to_string_lossy().replace('\\', "/");
        let run = |s: &str| show(&format!("d = \"{d}\"\n{s}"));
        run(r#"mkdir "{d}/a" "{d}/b/c""#);
        run(r#"write_file(d + "/a/x.txt", "one fish\nTwo Fish\nred\n"); write_file(d + "/.hidden", "")"#);
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
        assert!(run(r#"ls -z {d}"#).contains("unknown flag `-z`"));
        run(r#"rm -r {d}"#);
        assert!(!dir.exists());
    }

    #[test]
    fn externals() {
        let dir = std::env::temp_dir().join(format!("zil-ext-{}", std::process::id()));
        let d = dir.to_string_lossy().replace('\\', "/");
        let run = |s: &str| show(&format!("d = \"{d}\"\n{s}"));
        run(r#"mkdir {d}; write_file(d + "/hi.txt", "hi\n")"#);
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
