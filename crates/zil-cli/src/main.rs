//! The `zil` command line: REPL, saved sessions, `-e`, scripts and `--docs`.

mod editor;
mod native;
mod pager;
mod repl;
mod sessions;

use std::process::exit;
use std::rc::Rc;
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
       zil -e <code> [args...]   evaluate code and print the result
       zil <file.zil> [args...]  run a script; args() lists the extra arguments
       zil --version             print the version

Before any of these: --set <key>=<value> changes a setting (repeatable; ZIL_<KEY> env vars set them too),
--seed <n> fixes random numbers, --tz <zone> the time zone, and --now <date> stops the clock at that moment.
config in zil lists the settings.";

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
    let (host, sets) = flags(&mut args);
    zil::host::set_default(Rc::new(host));
    let mut session = Session::new(zil::host::get());
    apply_settings(&mut session, &sets);
    match args.as_slice() {
        [] => repl::repl(session, sessions::LAST, None),
        [flag] if flag == "-n" || flag == "--new" => {
            let archived = sessions::archive().unwrap_or_else(|e| {
                eprintln!("zil: {e}");
                exit(1)
            });
            repl::repl(session, sessions::LAST, archived.as_deref())
        }
        [flag, rest @ ..] if (flag == "-c" || flag == "--continue") && rest.len() <= 1 => {
            let name = rest.first().map_or(sessions::LAST, String::as_str);
            if !sessions::valid(name) {
                eprintln!("zil: session names use only letters, digits, - and _");
                exit(2)
            }
            repl::repl(session, name, None)
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
        [flag, dir] if flag == "--docs" => write_docs(dir.as_ref()).unwrap_or_else(|e| {
            eprintln!("zil: {dir}: {e}");
            exit(1)
        }),
        [flag] if flag == "-h" || flag == "--help" => println!("{USAGE}"),
        [flag] if flag == "-V" || flag == "--version" => println!("zil {}", env!("CARGO_PKG_VERSION")),
        [path, rest @ ..] if !path.starts_with('-') => {
            session.set_args(rest.to_vec());
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

/// Takes the leading `--seed`, `--tz`, `--now` and `--set` flags off `args`: the host they pin, and the `--set` values.
fn flags(args: &mut Vec<String>) -> (native::NativeHost, Vec<String>) {
    let bad = |flag: &str, e: &dyn std::fmt::Display| -> ! {
        eprintln!("zil: {flag}: {e}");
        exit(2)
    };
    let (mut host, mut sets, mut now) = (native::NativeHost::default(), vec![], None);
    while let [flag, v, ..] = args.as_slice()
        && ["--seed", "--tz", "--now", "--set"].contains(&flag.as_str())
    {
        match flag.as_str() {
            "--seed" => host.seed = Some(v.parse().unwrap_or_else(|e| bad(flag, &e))),
            "--tz" => host.tz = Some(jiff::tz::TimeZone::get(v).unwrap_or_else(|e| bad(flag, &e))),
            "--now" => now = Some(v.clone()),
            _ => sets.push(v.clone()),
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
    (host, sets)
}

/// `ZIL_<KEY>` env vars, then the `--set key=value` flags; a bad one exits with status 2.
fn apply_settings(session: &mut Session, sets: &[String]) {
    let set = |key: &str, v: &str, from: &str| {
        let v = v.parse().map_or_else(|_| Value::str(v), Value::int);
        if let Err(e) = session.settings().set(key, &v) {
            eprintln!("zil: {from}: {e}");
            exit(2)
        }
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
