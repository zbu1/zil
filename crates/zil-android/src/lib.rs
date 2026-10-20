//! The Android app's native side: one REPL session on its own thread, fed input by `Zil` in `android/` through JNI.
//! Output, the clipboard and "done" go back as calls on that same Kotlin object.
#![cfg(target_os = "android")]
// JNI finds the entry points by their Java names.
#![allow(non_snake_case)]

use jiff::Timestamp;
use jiff::tz::TimeZone;
use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::sys::{jboolean, jint};
use jni::{JNIEnv, JavaVM};
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use zil::ansi::{RESET, Role};
use zil::host::{Host, Meta};
use zil::repl::{self, Shown};
use zil::{Session, help};

/// Work for the session thread, once `start` has made it.
static INPUT: OnceLock<Sender<Job>> = OnceLock::new();

enum Job {
    Run(String),
    /// `repl::complete`'s hits for `word`, given the text before it, sent back on the channel.
    Complete(String, String, Sender<Vec<String>>),
    /// How many characters fit across the log, which help fits its boxes to.
    Columns(usize),
}
/// The session's Ctrl-C, for the Stop button.
static INTERRUPT: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// The phone: output and the clipboard go through the Kotlin `Zil` object; files live in the app's private storage,
/// which is also the working directory. No processes or environment.
struct AndroidHost {
    vm: JavaVM,
    ui: GlobalRef,
    cache: PathBuf,
}

impl AndroidHost {
    /// Calls `name` with JNI signature `sig` on the Kotlin object, returning its result if it's a string.
    fn call(&self, name: &str, sig: &str, arg: Option<&str>) -> io::Result<Option<String>> {
        let mut env = self.vm.get_env().map_err(io::Error::other)?;
        let out = env.with_local_frame(8, |env| -> jni::errors::Result<Option<String>> {
            let s = arg.map(|a| env.new_string(a)).transpose()?;
            let args: Vec<JValue> = s.iter().map(|s| JValue::Object(s)).collect();
            let r = env.call_method(&self.ui, name, sig, &args)?;
            if sig.ends_with('V') {
                return Ok(None);
            }
            let r = JString::from(r.l()?);
            Ok(Some(env.get_string(&r)?.into()))
        });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        out.map_err(io::Error::other)
    }
}

impl Host for AndroidHost {
    fn print(&self, text: &str) {
        let _ = self.call("print", "(Ljava/lang/String;)V", Some(text));
    }
    fn note(&self, text: &str) {
        self.print(text)
    }
    fn sleep(&self, secs: f64) {
        std::thread::sleep(Duration::from_secs_f64(secs))
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
    fn copy_text(&self, text: &str) -> io::Result<()> {
        self.call("copy", "(Ljava/lang/String;)V", Some(text)).map(drop)
    }
    fn paste_text(&self) -> io::Result<String> {
        Ok(self.call("paste", "()Ljava/lang/String;", None)?.unwrap_or_default())
    }
    fn http_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, String> {
        ureq::get(url).query_pairs(query.iter().copied()).call().and_then(|mut r| r.body_mut().read_to_string()).map_err(|e| e.to_string())
    }
    fn cache_get(&self, key: &str) -> Option<(String, Timestamp)> {
        let path = self.cache.join(key);
        let at = Timestamp::try_from(path.metadata().ok()?.modified().ok()?).ok()?;
        Some((std::fs::read_to_string(path).ok()?, at))
    }
    fn cache_put(&self, key: &str, value: &str) {
        let _ = std::fs::create_dir_all(&self.cache);
        let _ = std::fs::write(self.cache.join(key), value);
    }
    fn exit(&self, _: i32) -> String {
        "there's no process to exit in the app".into()
    }
}

/// The session thread: greets once the log's width is known, then runs each input as the terminal REPL would, saying
/// `done` after each.
fn serve(host: AndroidHost, input: Receiver<Job>) {
    let _ = host.vm.attach_current_thread_permanently();
    let host = Rc::new(host);
    zil::host::set_default(host.clone());
    let mut session = Session::new(zil::host::get());
    let _ = INTERRUPT.set(Arc::clone(&session.limits().interrupt));
    let (mut n, mut columns) = (0, 0);
    for job in input {
        let src = match job {
            Job::Run(src) => src,
            Job::Complete(before, word, reply) => {
                let _ = reply.send(repl::complete(&session, &before, &word));
                continue;
            }
            Job::Columns(c) => {
                if columns == 0 {
                    let tip = help::live(Some((true, c)), || help::tip(true));
                    host.print(&format!("zil {} {}· help for docs{RESET}\n{tip}\n", env!("CARGO_PKG_VERSION"), Role::Dim.code()));
                }
                columns = c;
                continue;
            }
        };
        session.limits().interrupt.store(false, Ordering::Relaxed);
        let shown = catch_unwind(AssertUnwindSafe(|| eval(&mut session, &mut n, &src, columns))).unwrap_or_else(|_| "zil crashed on that input".into());
        if !shown.is_empty() {
            host.print(shown.trim_end());
        }
        let _ = host.call("done", "()V", None);
    }
}

/// One REPL input's shown result or error, in color, with help fit to `columns`; what it printed has already gone out.
fn eval(session: &mut Session, n: &mut usize, src: &str, columns: usize) -> String {
    let src = &*repl::help_sugar(src);
    // The app clears its own history and log, and confirms `clear session` first.
    match src.trim() {
        "clear results" => {
            repl::forget(session, n);
            return String::new();
        }
        "clear session" => {
            repl::reset(session, n);
            return String::new();
        }
        _ => {}
    }
    if let Some(topic) = repl::help_shorthand(src) {
        return help::live(Some((true, columns)), || help::help(topic, true)).unwrap_or_else(|e| e);
    }
    match help::live(repl::help_call(src).then_some((true, columns)), || session.eval(src)) {
        Ok(v) => match repl::record(session, n, src, &v) {
            Shown::Hidden => String::new(),
            Shown::Bare => v.to_string(),
            Shown::Numbered(k) => repl::show(k, &v, true),
            Shown::Named(k, pairs) => repl::show_named(k, &pairs, true),
            Shown::Defined(sig) => format!("{}{sig}{RESET}", Role::Dim.code()),
        },
        Err(e) => e.render("<repl>", src, true),
    }
}

fn string(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

/// Starts the session thread, once per process: `files` becomes the working directory, `cache` holds currency rates.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_start(mut env: JNIEnv, this: JObject, files: JString, cache: JString) {
    let (files, cache) = (string(&mut env, &files), string(&mut env, &cache));
    let (Ok(vm), Ok(ui)) = (env.get_java_vm(), env.new_global_ref(&this)) else { return };
    let (tx, rx) = channel();
    if INPUT.set(tx).is_err() {
        return;
    }
    let _ = std::env::set_current_dir(files);
    let host = AndroidHost { vm, ui, cache: cache.into() };
    std::thread::Builder::new().name("zil".into()).stack_size(zil::STACK).spawn(move || serve(host, rx)).expect("spawn interpreter thread");
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_submit(mut env: JNIEnv, _: JObject, src: JString) {
    if let Some(tx) = INPUT.get() {
        let _ = tx.send(Job::Run(string(&mut env, &src)));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_stop(_: JNIEnv, _: JObject) {
    if let Some(i) = INTERRUPT.get() {
        i.store(true, Ordering::Relaxed);
    }
}

/// How many characters fit across the log; the first call also shows the greeting.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_columns(_: JNIEnv, _: JObject, n: jint) {
    if let Some(tx) = INPUT.get() {
        let _ = tx.send(Job::Columns(n.max(1) as usize));
    }
}

/// Whether Enter should continue `src` on a new line instead of running it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_unclosed(mut env: JNIEnv, _: JObject, src: JString) -> jboolean {
    repl::unclosed(&string(&mut env, &src)) as jboolean
}

/// `src` with numbers, strings and keywords colored, as the terminal REPL colors the line being typed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_highlight<'a>(mut env: JNIEnv<'a>, _: JObject, src: JString) -> JString<'a> {
    let src = string(&mut env, &src);
    env.new_string(zil::ansi::highlight(&src)).unwrap_or_default()
}

/// Completions for `word` after `before`, newline-separated; none if the session is busy running something.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_zbu1_zil_Zil_complete<'a>(mut env: JNIEnv<'a>, _: JObject, before: JString, word: JString) -> JString<'a> {
    let (before, word) = (string(&mut env, &before), string(&mut env, &word));
    let (tx, rx) = channel();
    let hits = INPUT.get().and_then(|i| i.send(Job::Complete(before, word, tx)).ok()).and_then(|_| rx.recv_timeout(Duration::from_millis(100)).ok());
    env.new_string(hits.unwrap_or_default().join("\n")).unwrap_or_default()
}
