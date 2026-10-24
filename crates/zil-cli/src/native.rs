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
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        Ok(Ran { code: out.status.code().unwrap_or(-1), out: text(&out.stdout), err: text(&out.stderr) })
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
}
