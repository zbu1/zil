//! Saved REPL sessions: each name's inputs in one file under the cache dir, for `zil`, `-c`, `--new`, `--list`,
//! `--rename` and `--delete`.

use std::io::Write;
use std::path::PathBuf;
use zil::cache_dir;

/// The unnamed session plain `zil` continues.
pub const LAST: &str = "last";

const ADJECTIVES: &[&str] = &[
    "amber", "bold", "brisk", "calm", "clever", "cosmic", "crisp", "dusty", "eager", "fuzzy", "gentle", "golden", "happy", "hidden", "jolly", "keen", "lively",
    "lucky", "mellow", "misty", "nimble", "noble", "plucky", "quiet", "rapid", "rusty", "silver", "sly", "snowy", "sunny", "swift", "tidy", "vivid", "wild",
    "witty", "zesty",
];
const NOUNS: &[&str] = &[
    "badger", "beacon", "comet", "cedar", "falcon", "fern", "fox", "gecko", "harbor", "heron", "lantern", "lynx", "maple", "meadow", "otter", "owl", "panda",
    "pebble", "pine", "quartz", "raven", "river", "robin", "sparrow", "spruce", "summit", "thistle", "tiger", "tulip", "walrus", "willow", "wren",
];

/// Whether `name` can name a session.
pub fn valid(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Where session `name` is saved.
pub fn path(name: &str) -> Option<PathBuf> {
    cache_dir().map(|d| d.join("sessions").join(format!("{name}.zil")))
}

/// The inputs in a session file, each ended by a NUL since one input can span lines.
pub fn inputs(saved: &str) -> impl Iterator<Item = &str> {
    saved.split('\0').filter(|s| !s.is_empty())
}

/// `--new`: moves the last session aside under a made-up name and returns it; nothing to do if it saved nothing.
pub fn archive() -> Result<Option<String>, String> {
    let Some(from) = path(LAST) else { return Ok(None) };
    if std::fs::read_to_string(&from).ok().is_none_or(|s| inputs(&s).next().is_none()) {
        return Ok(None);
    }
    for i in 0.. {
        let pair = format!("{}-{}", fastrand::choice(ADJECTIVES).unwrap(), fastrand::choice(NOUNS).unwrap());
        // Once most pairs are taken, a number keeps it unique.
        let name = if i < 100 { pair } else { format!("{pair}-{i}") };
        let to = path(&name).unwrap();
        if !to.exists() {
            return std::fs::rename(&from, &to).map(|_| Some(name)).map_err(|e| format!("can't move the last session aside: {e}"));
        }
    }
    unreachable!()
}

/// `--list`: every saved session, newest first, with how many inputs it replays and when it last changed.
pub fn list() -> Result<(), String> {
    let dir = cache_dir().ok_or("no cache directory")?.join("sessions");
    let mut rows = Vec::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = e.path();
        let (Some(name), Some("zil")) = (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|s| s.to_str())) else { continue };
        let modified = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| jiff::Timestamp::try_from(t).ok()).unwrap_or_default();
        let n = std::fs::read_to_string(&p).map_or(0, |s| inputs(&s).count());
        rows.push((name.to_string(), n, modified));
    }
    if rows.is_empty() {
        println!("no saved sessions");
        return Ok(());
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    let w = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let tz = jiff::tz::TimeZone::system();
    for (name, n, modified) in rows {
        let when = modified.to_zoned(tz.clone()).strftime("%Y-%m-%d %H:%M");
        let note = if name == LAST { "  (plain zil continues this)" } else { "" };
        println!("{name:w$}  {when}  {n:>4} input{}{note}", if n == 1 { "" } else { "s" });
    }
    Ok(())
}

/// `--rename`: gives saved session `from` the name `to`, which mustn't be taken.
pub fn rename(from: &str, to: &str) -> Result<(), String> {
    let (src, dst) = (saved(from)?, path(to).filter(|_| valid(to)).ok_or("session names use only letters, digits, - and _")?);
    if dst.exists() {
        return Err(format!("there's already a session named {to}"));
    }
    std::fs::rename(src, dst).map_err(|e| e.to_string())
}

/// `--delete`: removes saved session `name` once confirmed.
pub fn delete(name: &str) -> Result<(), String> {
    let p = saved(name)?;
    if confirm(&format!("delete session {name}?")) {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The file of session `name`, which must exist.
fn saved(name: &str) -> Result<PathBuf, String> {
    path(name).filter(|p| valid(name) && p.exists()).ok_or(format!("no saved session named {name}; zil --list shows them"))
}

/// Asks `question` and waits for y or n on stdin; anything but yes is no.
pub fn confirm(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}
