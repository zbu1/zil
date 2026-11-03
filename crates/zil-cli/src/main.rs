//! The `zil` command line: REPL, saved sessions, `-e`, scripts and `--docs`.

mod editor;
mod native;
mod pager;
mod repl;
mod sessions;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::rc::Rc;
use zil::host::Permission;
use zil::{Error, STACK, Session, Value, ansi, help};

/// Run `src` and exit with status 1 on error.
fn run_or_exit(session: &mut Session, name: &str, src: &str) -> Value {
    session.eval(src).unwrap_or_else(|e: Error| {
        report(name, src, &e);
        exit(1)
    })
}

const USAGE: &str = "usage: zil                        continue the last REPL session
       zil -n                    start a new session, keeping the last one under a made-up name
       zil -c <name>             continue the session named, or start it
       zil --list                list saved sessions
       zil --rename <old> <new>  rename a saved session
       zil --delete <name>       delete a saved session
       zil <folder>              start a REPL in a folder
       zil -e <code> [args...]   evaluate code and print the result
       zil <file.zil> [args...]  run a script; args() lists the extra arguments, and config.permissions = [...]
                                 lists what it may do outside itself, like \"files.read\" or \"network\"
       zil config                list the settings and where each came from
       zil config --init         write a commented config.toml to start from
       zil --update-rates        fetch the latest currency exchange rates now
       zil --list-units <word>   list the units whose name or description mentions a word
       zil --list-functions <word>  list the functions that do
       zil --version             print the version

Before any of these: --set <key>=<value> changes a setting (repeatable; ZIL_<KEY> env vars set them too),
--base <bin|oct|dec|hex|2-36> shows integer results in that base, --seed <n> fixes random numbers, --tz <zone> the time zone, and --now <date> stops the clock at that moment.
--config <file> reads settings from that file instead of config.toml in the zil config folder, and --no-config
from neither. config in zil lists the settings.
--deny <permissions> takes away permissions, like --deny processes,network; --sandbox takes away all of them and
stops each run after 5 seconds or 1 MiB of output, and --allow <permissions> gives some back; --allow files.read:<folder>
(or files.write) limits that permission to the folders given. deny = [...] in config.toml denies permissions too.";

fn main() {
    // The legacy console (e.g. a Start menu shortcut) shows ANSI colors as raw codes until asked not to.
    #[cfg(windows)]
    crossterm::ansi_support::supports_ansi();
    let t = std::thread::Builder::new().stack_size(STACK).spawn(real_main).expect("spawn interpreter thread");
    if t.join().is_err() {
        exit(101)
    }
}

fn real_main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let (mut host, sets, config, perms, sandbox, base) = flags(&mut args);
    let mut table = config.as_deref().map(read_config).unwrap_or_default();
    let bad = |e: &dyn std::fmt::Display| -> ! { config_error(config.as_deref(), e) };
    let deny = table.remove("deny").map(|d| d.as_array().cloned().unwrap_or_else(|| bad(&"deny lists permission names"))).unwrap_or_default();
    for p in deny {
        host.grant(p.as_str().unwrap_or_else(|| bad(&"deny lists permission names")), false).unwrap_or_else(|e| bad(&e));
    }
    for (spec, allow) in &perms {
        host.grant(spec, *allow).unwrap_or_else(|e| config_error(None, &format!("--{}: {e}", if *allow { "allow" } else { "deny" })));
    }
    let stay = matches!(args.as_slice(), [dir] if Path::new(dir).is_dir());
    if stay {
        let dir = args.remove(0);
        std::env::set_current_dir(&dir).unwrap_or_else(|e| config_error(None, &format!("{dir}: {e}")));
    }
    zil::host::set_default(Rc::new(host));
    let mut session = Session::new(zil::host::get());
    if sandbox {
        let l = session.limits();
        l.seconds = Some(5.0);
        l.output = 1 << 20;
    } else {
        session.limits().len = usize::MAX;
    }
    if let Some(b) = base {
        session.settings().base = b;
    }
    let mut from = HashMap::new();
    let preludes = config.as_deref().map(|p| load_config(&mut session, p, table, &mut from)).unwrap_or_default();
    apply_settings(&mut session, &sets, &mut from);
    let layered: Vec<String> = zil::KEYS.iter().map(|k| session.settings().get(k.name).map(|v| toml_value(&v)).unwrap_or_default()).collect();
    for p in preludes {
        let src = std::fs::read_to_string(&p).unwrap_or_else(|e| {
            eprintln!("zil: {}: {e}", p.display());
            exit(1)
        });
        run_or_exit(&mut session, &p.to_string_lossy(), &src);
    }
    match args.as_slice() {
        [] => repl::repl(session, sessions::LAST, None, stay),
        [flag] if flag == "-n" || flag == "--new" => {
            let archived = sessions::archive().unwrap_or_else(|e| {
                eprintln!("zil: {e}");
                exit(1)
            });
            repl::repl(session, sessions::LAST, archived.as_deref(), stay)
        }
        [flag, rest @ ..] if (flag == "-c" || flag == "--continue") && rest.len() <= 1 => {
            let name = rest.first().map_or(sessions::LAST, String::as_str);
            if !sessions::valid(name) {
                eprintln!("zil: session names use only letters, digits, - and _");
                exit(2)
            }
            repl::repl(session, name, None, stay)
        }
        [cmd] if cmd == "config" => {
            for (k, before) in zil::KEYS.iter().zip(layered) {
                let v = session.settings().get(k.name).map(|v| toml_value(&v)).unwrap_or_default();
                let src = if v != before { "a prelude" } else { from.get(k.name).map_or("default", String::as_str) };
                println!("{} = {v}  # {src}", k.name);
            }
            let host = zil::host::get();
            let denied: Vec<String> = Permission::ALL.into_iter().filter(|&p| !host.permits(p)).map(|p| format!("{:?}", p.name())).collect();
            println!("deny = [{}]  # {}", denied.join(", "), if denied.is_empty() { "default" } else { "the file and flags" });
        }
        [cmd, flag] if cmd == "config" && flag == "--init" => or_exit(init_config()),
        [flag] if flag == "--update-rates" => {
            let n = run_or_exit(&mut session, "<rates>", "refresh_rates()");
            println!("updated rates for {n} currencies");
        }
        [flag] if flag == "--list" => or_exit(sessions::list()),
        [flag, from, to] if flag == "--rename" => or_exit(sessions::rename(from, to)),
        [flag, name] if flag == "--delete" => or_exit(sessions::delete(name)),
        [flag, code, rest @ ..] if flag == "-e" => {
            session.set_args(rest.to_vec());
            match help::live(repl::help_term(code), || run_or_exit(&mut session, "<-e>", code)) {
                Value::Nil => {}
                v => println!("{}", zil::repl::display(&session, &v, ansi::color_on())),
            }
        }
        [flag, word] if flag == "--list-units" || flag == "--list-functions" => {
            session.set_args(vec![word.clone()]);
            let code = if flag == "--list-units" {
                r#"all_units().filter(|u| "{u.name} {u.full} {u.desc} {u.aliases.join(" ")}".lower().contains(args()[0].lower())).map(|u| {name: u.name, kind: u.kind, desc: u.desc})"#
            } else {
                "search(args()[0])"
            };
            let v = run_or_exit(&mut session, "<list>", code);
            println!("{}", zil::repl::display(&session, &v, ansi::color_on()));
        }
        [flag, dir] if flag == "--docs" => write_docs(dir.as_ref()).unwrap_or_else(|e| {
            eprintln!("zil: {dir}: {e}");
            exit(1)
        }),
        [flag] if flag == "-h" || flag == "--help" => println!("{USAGE}"),
        [flag] if flag == "-V" || flag == "--version" => println!("zil {}", env!("CARGO_PKG_VERSION")),
        [path, rest @ ..] if !path.starts_with('-') => {
            session.set_args(rest.to_vec());
            session.as_script();
            let src = std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("zil: {path}: {e}");
                exit(1)
            });
            run_or_exit(&mut session, path, &src);
        }
        _ => {
            eprintln!("{USAGE}");
            exit(2)
        }
    }
}

/// Takes the leading `--seed`, `--tz`, `--now`, `--set`, `--config`, `--no-config`, `--allow`, `--deny` and `--sandbox`
/// flags off `args`: the host they pin, the `--set` values, the config file to read, the permissions to allow (`true`)
/// or deny, in order, and whether to sandbox.
#[allow(clippy::type_complexity)]
fn flags(args: &mut Vec<String>) -> (native::NativeHost, Vec<String>, Option<PathBuf>, Vec<(String, bool)>, bool, Option<u32>) {
    let bad = |flag: &str, e: &dyn std::fmt::Display| -> ! {
        eprintln!("zil: {flag}: {e}");
        exit(2)
    };
    let (mut host, mut sets, mut now) = (native::NativeHost::default(), vec![], None);
    let (mut config, mut perms, mut sandbox, mut base) = (config_path().filter(|p| p.is_file()), vec![], false, None);
    loop {
        match args.as_slice() {
            [flag, ..] if flag == "--no-config" || flag == "--sandbox" => {
                if flag == "--no-config" {
                    config = None;
                } else {
                    sandbox = true;
                    perms.extend(Permission::ALL.map(|p| (p.name().to_string(), false)));
                }
                args.remove(0);
                continue;
            }
            [flag, v, ..] if ["--seed", "--tz", "--now", "--set", "--config", "--allow", "--deny", "--base"].contains(&flag.as_str()) => match flag.as_str() {
                "--allow" | "--deny" => perms.extend(v.split(',').map(|p| (p.trim().to_string(), flag == "--allow"))),
                "--seed" => host.seed = Some(v.parse().unwrap_or_else(|e| bad(flag, &e))),
                "--tz" => host.tz = Some(jiff::tz::TimeZone::get(v).unwrap_or_else(|e| bad(flag, &e))),
                "--now" => now = Some(v.clone()),
                "--config" => config = Some(v.into()),
                "--base" => {
                    base = Some(match v.as_str() {
                        "bin" => 2,
                        "oct" => 8,
                        "dec" => 10,
                        "hex" => 16,
                        n => n.parse().ok().filter(|b| (2..=36).contains(b)).unwrap_or_else(|| bad(flag, &"is bin, oct, dec, hex or a number from 2 to 36")),
                    })
                }
                _ => sets.push(v.clone()),
            },
            _ => break,
        }
        args.drain(..2);
    }
    // A plain date and time is on the --tz clock.
    host.now = now.map(|v| {
        let tz = host.tz.clone().unwrap_or_else(jiff::tz::TimeZone::system);
        v.parse::<jiff::Timestamp>()
            .or_else(|_| {
                let d = v.parse::<jiff::civil::DateTime>().or_else(|_| v.parse::<jiff::civil::Date>().map(|d| d.at(0, 0, 0, 0)))?;
                Ok::<_, jiff::Error>(d.to_zoned(tz)?.timestamp())
            })
            .unwrap_or_else(|_| bad("--now", &format!("{v:?} is not a date like 2026-01-01T09:30")))
    });
    (host, sets, config, perms, sandbox, base)
}

/// `config.toml`: in `zil-data` beside the portable build, else in the user's config folder.
fn config_path() -> Option<PathBuf> {
    let portable = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("zil-data"))).filter(|d| d.is_dir());
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    let dir = portable.or_else(|| Some(env("XDG_CONFIG_HOME").or_else(|| env("APPDATA")).or_else(|| Some(env("HOME")?.join(".config")))?.join("zil")))?;
    Some(dir.join("config.toml"))
}

/// Exits with status 2 over a bad config file `path`, or a bad flag without one.
fn config_error(path: Option<&Path>, e: &dyn std::fmt::Display) -> ! {
    match path {
        Some(p) => eprintln!("zil: {}: {e}", p.display()),
        None => eprintln!("zil: {e}"),
    }
    exit(2)
}

fn read_config(path: &Path) -> toml::Table {
    let bad = |e: &dyn std::fmt::Display| -> ! { config_error(Some(path), e) };
    std::fs::read_to_string(path).unwrap_or_else(|e| bad(&e)).parse().unwrap_or_else(|e| bad(&e))
}

/// Applies the settings in `table`, read from config file `path`, and returns its `prelude` files, relative to its
/// folder; a bad file exits with status 2.
fn load_config(session: &mut Session, path: &Path, table: toml::Table, from: &mut HashMap<String, String>) -> Vec<PathBuf> {
    let bad = |e: &dyn std::fmt::Display| -> ! { config_error(Some(path), e) };
    let mut preludes = vec![];
    for (key, v) in table {
        let v = match v {
            toml::Value::Array(files) if key == "prelude" => {
                let dir = path.parent().unwrap_or(Path::new(""));
                preludes = files.iter().map(|f| f.as_str().map(|f| dir.join(f)).unwrap_or_else(|| bad(&"prelude lists file names"))).collect();
                continue;
            }
            toml::Value::String(s) => Value::str(s),
            toml::Value::Integer(n) => Value::int(n),
            toml::Value::Float(x) => Value::Float(x),
            toml::Value::Boolean(b) => Value::Bool(b),
            _ => bad(&format!("{key} takes a string, number or bool")),
        };
        session.settings().set(&key, &v).unwrap_or_else(|e| bad(&e));
        from.insert(key, path.display().to_string());
    }
    preludes
}

/// `ZIL_<KEY>` env vars, then the `--set key=value` flags; a bad one exits with status 2.
fn apply_settings(session: &mut Session, sets: &[String], from: &mut HashMap<String, String>) {
    let mut set = |key: &str, v: &str, src: &str| {
        let v = v.parse().map_or_else(|_| Value::str(v), Value::int);
        if let Err(e) = session.settings().set(key, &v) {
            eprintln!("zil: {src}: {e}");
            exit(2)
        }
        from.insert(key.to_string(), src.to_string());
    };
    for k in zil::KEYS {
        let var = format!("ZIL_{}", k.name.to_uppercase());
        if let Ok(v) = std::env::var(&var) {
            set(k.name, &v, &var);
        }
    }
    for kv in sets {
        let Some((key, v)) = kv.split_once('=') else {
            eprintln!("zil: --set takes key=value");
            exit(2)
        };
        set(key, v, "--set");
    }
}

/// `v` as a TOML value.
fn toml_value(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("{:?}", s.to_string()),
        v => v.to_string(),
    }
}

/// Writes `config.toml` with every setting commented out at its default, unless one is already there.
fn init_config() -> Result<(), String> {
    let path = config_path().ok_or("no config folder: set XDG_CONFIG_HOME, APPDATA or HOME")?;
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let defaults = zil::Settings::default();
    let mut out = String::from(
        "# zil settings. `zil config` lists them with where each value came from.

# .zil files to run at startup, relative to this folder
# prelude = [\"mine.zil\"]

# Permissions zil never gets, unless --allow gives them back
# deny = [\"processes\", \"network\"]
",
    );
    for k in zil::KEYS {
        out += &format!(
            "
# {}
# {} = {}
",
            k.doc,
            k.name,
            defaults.get(k.name).map(|v| toml_value(&v)).unwrap_or_default()
        );
    }
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(""))).and_then(|_| std::fs::write(&path, out)).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Exits with status 1 on a session command's error.
fn or_exit(r: Result<(), String>) {
    if let Err(e) = r {
        eprintln!("zil: {e}");
        exit(1)
    }
}

/// Writes the reference pages (`help::docs::pages`) into `dir`.
fn write_docs(dir: &std::path::Path) -> std::io::Result<()> {
    for (name, page) in help::docs::pages(jiff::Zoned::now().date()) {
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(file, page)?;
    }
    Ok(())
}

/// Prints `e` to stderr, colored when stderr is a terminal.
fn report(name: &str, src: &str, e: &Error) {
    use std::io::IsTerminal;
    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    eprint!("{}", e.render(name, src, color));
}
