//! Written guides shared by `help` and the docs: the intro, the highlights, the syntax tour and the advanced recipes.

use crate::modules::Section;

/// Heads the docs' Introduction page.
pub const INTRO_LEAD: &str = "zil started as a calculator and grew into a scripting language and a shell. You don't need all of that on day one. \
    Read the section closest to how you work now, and come back to the others when you're curious.";

/// A section of the intro: heading, prose (`code` in backticks, paragraphs split by blank lines), whether the examples
/// are command lines (shown with what they desugar to, not run), and the examples.
pub type Intro = (&'static str, &'static str, bool, &'static [(&'static str, &'static str)]);

/// The docs' Introduction: where to start, by what you're used to.
#[rustfmt::skip]
pub const INTRO: &[Intro] = &[
    ("coming from a calculator or spreadsheet", "\
Type the math the way you'd write it on paper, units and all. zil keeps track of them, so `100 km / 2 h` comes out as a speed, \
and adding meters to seconds is an error instead of a wrong answer. Put `to` after anything to see it another way: other units, \
a fraction, a time zone.

Every answer is saved. `_` is the last one and `_1`, `_2` and so on are the numbered ones, so you can build on a result like you \
would with a calculator's memory. When you want more, like rounding, averages or dates, there's a function for it already loaded. \
`help` lists them, and putting `?` after anything explains it.", false, &[
        ("units come along", "100 km / 2 h"),
        ("to converts", "100 km / 2 h to mph"),
        ("into mixed units", "1.8 m to ft in"),
        ("percentages", "80 + 15%"),
        ("three of them, with tax", "19.99 * 3 * (1 + 8.25%)"),
        ("how long a download takes", "4.7 GB / 50 Mbps to min"),
        ("a column total", "[12.50, 8.25, 31].sum"),
        ("when a call lands in Tokyo", r#"date("2026-12-25 18:30") to "Tokyo""#),
    ]),
    ("coming from Python or JavaScript", "\
Most of what you know carries over. There are variables, functions, `if`, `for` and `while`, lists and maps, and string \
interpolation with `{}`. What's different is mostly about typing less. Nothing needs importing, since the whole library is \
always loaded. Any function can be called as a method, so `x.f(y)` means `f(x, y)` and you can write a chain of steps in the \
order they happen. Calls with no arguments drop the parens, and `|x| x * 2` is a lambda.

Numbers behave more like math than like floats. Decimals are exact, so `0.1 + 0.2 == 0.3` really is true, integers never \
overflow, and `1 / 3` stays a third instead of rounding. Units are part of the value too, so mixing up meters and feet is an \
error instead of a bug you find later.", false, &[
        ("x.f(y) is f(x, y)", r#""a-b-c".split("-")"#),
        ("no arguments, no parens", r#""hello".upper"#),
        ("lambdas", "[1, 2, 3].map(|x| x * 10)"),
        ("|> pipes a value into a function", "[3, 1, 2] |> sort"),
        ("functions", "double = fn(x) { x * 2 }; double(4)"),
        ("interpolation", r#""2 + 2 = {2 + 2}""#),
        ("exact decimals", "0.1 + 0.2 == 0.3"),
        ("fractions stay exact", "1 / 3 * 3 == 1"),
        ("no overflow", "2 ** 100"),
    ]),
    ("coming from bash", "\
zil has a command mode, so a lot of everyday shell use works as is. A line that starts with a command like `ls`, `cd` or \
`grep` is a shell command, and `!` runs any other program, like `!git status`. Pipes, globs and `>` redirection all work.

The difference is what comes back. zil's own commands return values, not text, so `ls` gives you a table of files you can \
filter or sum without parsing anything. `|>` hands the result back to zil so you can keep going, and for anything with real \
logic you get a proper language instead of `[[ ]]` and `$(( ))`.

A few habits don't carry over. Single quotes, `$VAR` and brace expansion are errors, and `help shell` shows what to write \
instead. Below, each line is shown with the zil call it turns into, which `desugar` shows for any line.", true, &[
        ("long listing, hidden files too", "ls -la src"),
        ("run any program", "!git status"),
        ("pipe into a program", "ls | sort"),
        ("hand the result back to zil", "ls src |> len"),
        ("only the big files", "ls src |> where(|f| f.size > 10 KB)"),
        ("a zil value as an argument", "rm {old}"),
    ]),
    ("where to go next", "\
`help examples` is a long page of one-liners grouped by module, and the quickest way to see what's in here. `help syntax` \
covers the whole language on one screen. `help advanced` has longer recipes that mix several modules. And `help` on its own \
lists every module, so `help time` or `help geo` shows everything in one.", false, &[]),
];

/// Things most languages can't do in one line: heads `help("examples")` and the docs index.
#[rustfmt::skip]
pub const HIGHLIGHTS: &[Section] = &[(
    "a taste",
    &[
        ("4th Thursday of November", r#"date(2026, 11, 1).nth_weekday(4, "thu")"#),
        ("cities are time zones", r#"date("2026-12-25 18:30") to "Sao Paulo""#),
        ("units combine and convert", "100 km / 2 h to mph"),
        ("cups to grams", r#"1 cup * density("flour") to g"#),
        ("an hourly wage as a yearly salary", "$25/h to USD/workyr"),
        ("measurements with error bars", "5 ± 0.1 m * 2"),
        ("exact decimals", "0.1 + 0.2 == 0.3"),
        ("factoring big ints", "factors(2 ** 32 + 1)"),
        ("calculus: the area under a sine curve", "integrate(sin, 0, pi)"),
        ("the distance from Paris to Tokyo", r#"great_circle("Paris", "Tokyo")"#),
        ("the closest spelling", r#""recieve".closest(["deceive", "receive", "recipe"])"#),
        ("is this IP address in that network?", r#"in_cidr("10.0.3.7", "10.0.0.0/22")"#),
        ("odds in plain words", "odds(2 ** -64)"),
        ("charts made of text", "(0..20).map(|x| sin(x / 3)).sparkline"),
        ("reading these: x.f(y) means f(x, y), |x| x * 2 is a small function of x, xs |> sum passes xs to sum, and # starts a comment", ""),
    ],
)];

/// `help("syntax")`: the language at a glance.
#[rustfmt::skip]
pub const SYNTAX: &[Section] = &[
    ("values", &[
        ("decimals are exact", "0.1 + 0.2"),
        ("dividing integers gives an exact fraction", "7 / 2"),
        ("// divides and rounds down", "7 // 2"),
        ("** is power, and ^ is xor, not power", "2 ** 10"),
        ("hex and binary numbers stay in their base", "0xff + 1"),
        ("units attach to numbers", "60 km/h to m/s"),
        ("a number touching a name multiplies, with no space: variables win over units, and a space always means the unit", "x = 3; [2x, 2pi, 2(x + 1), 3 m]"),
        ("a function and a space before a value calls it", "sqrt 16 + sin 90 deg"),
        ("percentages", "80 + 15%"),
        ("clock arithmetic: values that wrap around", "10 mod 12 + 5"),
        ("{...} in a string puts in a value", r#""2 + 2 = {2 + 2}""#),
        ("slices, where negative positions count from the end", r#""hello"[-3..]"#),
        ("r\"...\" is a regular expression", r#""a1b22".find_all(r"\d+")"#),
        ("b\"...\" is raw bytes", r#"b"\x7fELF"[1..].str"#),
        ("a..=b counts from a to b, a..b stops just before b", "1..=5"),
        ("maps hold values under names", "{a: 1, b: 2}.b"),
    ]),
    ("calls", &[
        ("x.f(y) means f(x, y)", r#""a-b".split("-")"#),
        ("calls with no arguments need no parentheses", r#""hi".upper"#),
        ("|> passes a value to a function", "[3, 1, 2] |> sort"),
        ("_ marks where the piped value goes", "3.14159 |> round(_, 2)"),
        ("|x| ... is a small function, a lambda", "[1, 2, 3].map(|x| x * 10)"),
        ("to converts a value to a unit, base, type or zone", "255 to hex"),
    ]),
    ("statements", &[
        ("assignment, with no declaring needed", "x = 5; x += 2; x"),
        ("if gives a value, so it can be used anywhere", r#"if 3 > 2 { "yes" } else { "no" }"#),
        ("for loops over a range, list or other collection", "t = 0; for i in 1..=4 { t += i }; t"),
        ("unpack a list into names, _ skips an item", "[a, [b, _]] = [1, [2, 3]]; a + b"),
        ("while", "n = 3; while n > 0 { n -= 1 }; n"),
        ("fn defines a function", "f = fn(x) { return x * 2 }; f(4)"),
        ("comparisons chain, as in math", "x = 5; 0 < x <= 10"),
        ("# starts a comment that runs to the end of the line, and a new line or a semicolon ends a statement", ""),
    ]),
    ("more", &[("the Syntax page on the docs website has the full reference, including which operators bind tighter", "")]),
];

/// A keyword's help page: the keywords it covers, their forms, a one-line description, live examples, related names.
pub type Keyword = (&'static [&'static str], &'static str, &'static str, &'static [&'static str], &'static [&'static str]);

/// `help if` etc.: one page per keyword, or per keywords that go together.
#[rustfmt::skip]
pub const KEYWORDS: &[Keyword] = &[
    (&["if", "else"], "if cond { ... } else if cond { ... } else { ... }",
        "runs the first block whose condition is true; it gives that block's value, or nil if none ran, so it works inside expressions",
        &[r#"if 3 > 2 { "yes" } else { "no" }"#, r#"x = 7; if x < 5 { "low" } else if x < 10 { "mid" } else { "high" }"#, r#"if 1 > 2 { "never" }"#,
            "(if true { 1 } else { 2 }) + 10"],
        &["while", "true"]),
    (&["while"], "while cond { ... }", "loops, running the block for as long as the condition is true; it gives nil",
        &["n = 1; while n < 100 { n *= 2 }; n", "n = 0; while true { n += 1; if n == 3 { break } }; n"],
        &["for", "break"]),
    (&["for", "in"], "for x in items { ... } / x in items",
        "for runs the block once per item of a range, list, string, set or map (its keys), and a list pattern unpacks each item; \
on its own, x in items tests membership. After a number with nothing following, in is inches",
        &["t = 0; for i in 1..=4 { t += i }; t", r#"t = ""; for c in "abc" { t += c.upper }; t"#, r#"t = []; for i, x in ["a", "b"].enumerate { t.push("{i}{x}") }; t"#,
            "3 in [1, 2, 3]", r#""ell" in "hello""#, r#""a" in {a: 1}"#, "5 in to cm"],
        &["while", "break", "enumerate"]),
    (&["break", "continue"], "break / continue", "break leaves the innermost loop; continue skips the rest of the block, on to its next round",
        &["s = 0; for i in 1..=10 { if i == 4 { break }; s += i }; s", "s = 0; for i in 1..=5 { if i % 2 == 0 { continue }; s += i }; s"],
        &["for", "while"]),
    (&["fn", "return"], "fn(x: type, y = default, *rest) -> type { ... } / return value",
        "defines a function; types, defaults, *rest and -> are all optional. It gives its block's last value, or return's value early. \
|x| ... is the short form",
        &["f = fn(x: int, y = 2) -> int { x * y }; f(3)", "f = fn(*xs) { xs.len }; f(1, 2, 3)", r#"f = fn(x) { if x < 0 { return "neg" }; "pos" }; f(-1)"#,
            "[1, 2, 3].map(|x| x * 10)"],
        &["if", "map"]),
    (&["to"], "x to target", "converts a value: to a unit, a base, a type, a time zone or another form",
        &["5 km to mi", "1.5 h to min", "255 to hex", r#""12" to int"#, "0.75 to frac"],
        &["units", "hex"]),
    (&["of"], "a of b", "multiplies, so percentages read the way they're said",
        &["20% of 50", "15% of 80 km"],
        &["to"]),
    (&["mod"], "a mod n", "a modular value: arithmetic with it wraps around n, like hours on a clock. For a plain remainder, use rem",
        &["10 mod 12 + 5", "-7 mod 3", "(3 mod 7) * 5"],
        &["rem"]),
    (&["true", "false"], "true / false", "the two bools: comparisons give them, and if and while test them",
        &["3 > 2", "true && !false", "[1, 2, 3].any(|x| x > 2)"],
        &["if", "nil"]),
    (&["nil"], "nil", "nothing: what a missing map key, an if with no branch taken, or a loop gives",
        &["{a: 1}.b", "if false { 1 }", "type(nil)"],
        &["if", "true"]),
];

/// Heads `help("shell")` and the docs' command mode section: paragraphs (`code` in backticks), each followed by a box of
/// command lines shown with the calls they desugar to, unless its rows are empty.
#[rustfmt::skip]
pub const SHELL_INTRO: &[(&str, Section)] = &[
    ("Command mode is for the file and program work you would otherwise switch to a terminal for.", ("", &[])),
    ("One part is the built-in shortcuts: zil functions for things commonly done in a shell.", ("built-in commands", &[
        ("list a folder, hidden files too", "ls -la src"),
        ("lines with err, ignoring case", "grep -i err app.log"),
        ("delete a folder and what's in it", "rm -rf build"),
        ("a file's contents", "cat a.txt"),
    ])),
    ("Also, regular shell commands can be run with `!` in front of them. The output is shown as it happens, and the result is \
the program's exit code: 0 for success.", ("programs", &[
        ("output shows as it happens", "!git status"),
        ("interactive programs work too", "!python"),
        ("assigned, the output is captured instead", "log = !git log --oneline -5"),
        ("two programs, joined by a system pipe", "!git log --oneline | head -3"),
    ])),
    ("A few operators change meaning too.", ("operators", &[
        ("| passes a result on, instead of bitwise or", "ls src | grep rs"),
        ("> writes a file, instead of greater than", "ls > files.txt"),
        ("< feeds a file to a program", "!sort < names.txt"),
        ("|> means the same as in zil, and goes back to it", "ls src |> len"),
    ])),
];

/// `help("shell")`: command mode by task, each example a command line shown with the call it desugars to (not run).
#[rustfmt::skip]
pub const SHELL: &[Section] = &[
    ("words", &[
        ("a command on its own runs with no arguments", "ls"),
        ("* matches any part of a name", "rm *.tmp"),
        ("put names with spaces in double quotes", r#"cat "my file.txt""#),
        ("flags are letters that combine, -la is -l and -a, or names: --all is -a", ""),
        ("grep, wc, xxd and strings read the files you name, instead of taking text", ""),
    ]),
    ("mixing in zil", &[
        ("{expr} passes a zil value as one argument", "rm {old}"),
        ("|> hands the result to a zil function", "ls src |> len"),
        ("with parentheses it's an ordinary call, not a command: files = ls(\"src\")", ""),
        ("a variable with a command's name wins: after cat = 5, cat foo is no longer the cat command", ""),
    ]),
    ("running programs", &[
        ("! runs any program, showing its output", "!git status"),
        ("on a line by itself, the result is the program's exit code: 0 for success", ""),
        ("assigned or piped, its output is captured", "log = !git log --oneline -5"),
        ("when output is captured, a program that fails is an error", ""),
        ("! right before ( is still \"not\": !(x) means not x", ""),
    ]),
    ("pipes", &[
        ("| feeds one command's result into the next", "ls src | grep rs"),
        ("after |, any other name is a program", "ls | sort"),
        ("between two programs, | is an ordinary system pipe", "!git log --oneline | head -3"),
    ]),
    ("redirection", &[
        ("> writes a file, >> adds to the end of one", "ls > files.txt"),
        ("< feeds a file to a program as its input", "!sort < names.txt"),
        ("2> sends a program's errors to a file", "!make 2> errors.txt"),
    ]),
    ("REPL only", &[
        ("help topic shows a help page, the same as help(\"topic\")", ""),
        ("? after a name shows its help: upper? is help upper, and now.moon_phase? is help moon_phase", ""),
    ]),
    ("coming from bash", &[
        ("'single quotes' aren't strings: write \"...\", or r\"...\" to keep backslashes as they are", ""),
        ("{a,b} brace expansion isn't supported: pass a list, {[\"a\", \"b\"]}", ""),
        ("$VAR isn't a variable, since $ is for money like $5: write {env(\"VAR\")}", ""),
        ("ls - l doesn't work: a flag's letters must touch its dash, -l", ""),
        ("ls | where(...) is an error, since after | a name means a program: use |> to hand the result to zil", ""),
        ("2>&1 isn't supported: send error output to a file with 2> f", ""),
    ]),
];

/// The sandbox's tour, by chapter: (prose with `code` in backticks, a REPL entry to run). Each entry runs in the same session, so
/// later steps can use earlier results; a step with no code is just read, and the last one opens Script mode.
#[rustfmt::skip]
#[cfg(any(target_arch = "wasm32", test))]
pub const TOUR: &[(&str, &[(&str, &str)])] = &[
    ("Calculator", &[
        ("zil is a calculator first. Numbers carry units, and `to` converts them.", "42 km / 65 mph to min"),
        ("Every result is saved: `_` is the last one, and `_1`, `_2`… are numbered ones.", "_ * 2"),
        ("Decimals are exact, so this really is true.", "0.1 + 0.2 == 0.3"),
        ("Integers never overflow.", "2 ** 100"),
    ]),
    ("Syntax", &[
        ("Variables need no declaring. A newline or `;` ends a statement, and a line shows its last value.", "x = 5; x += 2; x"),
        ("`x.f(y)` is just `f(x, y)`, so every function can be chained as a method.", r#""a-b-c".split("-")"#),
        ("Calls without arguments need no parens.", r#""hello world".upper"#),
        ("Braces inside a string run code.", r#""2 + 2 = {2 + 2}""#),
        ("`|x| ...` is a function, ready to pass to `map`, `filter`, `sort_by` and friends.", "[1, 2, 3].map(|x| x * 10)"),
        ("`|>` pipes a value into a function.", "scores = [4, 8, 15, 16, 23, 42]\nscores.filter(|x| x > 10) |> avg"),
        ("`fn` defines a longer function, and `if` is an expression with a value.", "sq = fn(x) { x * x }\nif sq(4) > 10 { \"big\" } else { \"small\" }"),
    ]),
    ("Finding things", &[
        ("Help is built in, and every example in it runs live. Click a name or an example in the output to follow it.", "help(sparkline)"),
        ("Don't know a function's name? Search by topic.", r#"help("sorting")"#),
        ("Type a value and a dot at the prompt, like `\"hi\".`, to list every method that takes it. Hover any name for its docs.", ""),
    ]),
    ("Sampler", &[
        ("Units mix and split.", "1.8 m to ft in"),
        ("Error bars carry through arithmetic.", "(5 m ± 0.2 m) * (3 m ± 0.1 m)"),
        ("Dates know time zones, and cities work as zones.", r#"date("2026-12-25 18:30") to "Tokyo""#),
        ("Currencies use live rates, fetched the first time two currencies meet.", "100 USD to EUR"),
        ("Text tools are built in, like pulling every number out of a string.", r#""a1b22c333".nums.sum"#),
        ("So is calculus.", "integrate(sin, 0, pi)"),
        ("And a world atlas.", r#"great_circle("Paris", "Tokyo")"#),
        ("Charts are text, so they work anywhere.", "(0..20).map(|x| sin(x / 3)).sparkline"),
    ]),
    ("Scripts", &[
        ("For longer programs, use Script mode: every line shows its value as you type, and Share copies a link to your code. The catalog on the left has plenty more to try.", ""),
    ]),
];

/// A worked example drawing on several modules: (what it does, the modules it uses, a script whose result is shown).
pub type Recipe = (&'static str, &'static str, &'static str);

/// `help("advanced")`: recipes, by theme.
#[rustfmt::skip]
pub const ADVANCED: &[(&str, &[Recipe])] = &[
    ("travel and sky", &[
        ("a flight from Seattle to Tokyo: time in the air and local arrival", "geo · units · time.zones", r#"leave = date("2026-12-20T13:00-08:00") to "Seattle"
flight = great_circle("Seattle", "Tokyo") / 900 km/h
{in_the_air: flight.parts, landing: leave + flight to "Tokyo"}"#),
        ("UTC hours when it's 9 to 5 in New York, London and Berlin at once", "time.zones · data.lists", r#"office = |t| 9 <= t.hour < 17
day = date("2026-10-12T00:00Z") to UTC
hours = (0..24).map(|h| day + h * 1 h)
hours.filter(|t| office(t to "New York") && office(t to "London") && office(t to "Berlin")).map(|t| t.format("%H:%M"))"#),
        ("hours of daylight on the 21st of each month, equator to arctic", "time.sky · geo · data.charts · art.frames", r#"places = ["Singapore", "Cairo", "Paris", "Oslo", "Reykjavik"]
rows = places.map(|p| (1..=12).map(|m| day_length(p, date(2026, m, 21)) / 1 h))
places.map(|p| p.pad(10)).join("\n").beside(rows.heatmap)"#),
        ("where the sun sets within the next hour", "geo · time.sky · time", r#"cities().filter(|c| now <= sunset(c.name, today) < now + 1 h).map(|c| c.name).take(6)"#),
        ("the fastest a ping from New York could ever be: there and back through fiber at 2/3 c", "geo · units.constants", r#"["London", "Tokyo", "Sydney"].map(|p| 2 * great_circle("New York", p) / (2/3 * c) to ms)"#),
        ("dig straight down from Madrid: where do you come out?", "geo", r#"m = city("Madrid")
out = nearest(-m.lat, m.lon - 180)
{city: out.name, country: out.country, apart: great_circle("Madrid", out.name) to km}"#),
        ("the further north, the shorter December days: latitude vs. daylight across every city", "geo · time.sky · math.stats", r#"cs = cities()
corr(cs.map(|c| c.lat), cs.map(|c| day_length(c.name, date(2026, 12, 21)) / 1 h))"#),
        ("the day Oslo gains daylight fastest, and how fast", "time.sky · math.stats · time", r#"start = date(2026, 1, 1)
gains = (0..365).map(|i| day_length("Oslo", start + i * 1 d)).deltas
{day: start + (gains.find(gains.max) + 1) * 1 d, gained: gains.max to min, fastest_loss: gains.min to min}"#),
    ]),
    ("money and science", &[
        ("a $400k mortgage from January 2027: payment, total interest, when principal overtakes interest, interest by year", "finance · time · data.lists · data.charts", r#"plan = amortize($400000, 6.5%/yr, 30 yr)
flip = plan.filter(|m| m.principal > m.interest).first
yearly = plan.chunks(12).map(|y| y.map(|m| m.interest).sum / $1)
{monthly: plan[0].payment, interest: plan.map(|m| m.interest).sum, crossover: date(2027, 1, 1) + (flip.n - 1) * 1 mo, by_year: yearly.sparkline}"#),
        ("a ski trip: who owes whom", "finance · data.maps", r#"paid = {ana: $640 + $85.50, ben: 3 * $42, cy: $0, dee: $310}
{each: share(paid.values.sum, 4)[0], settle: settle(paid)}"#),
        ("weigh the Earth with a pendulum, with errors carried through every step", "math.uncertainty · units.constants", r#"L = 1.000 ± 0.005 m
T = (20.07 ± 0.1 s) / 10    # timed 10 swings
g = 4 * pi ** 2 * L / T ** 2 to m/s^2
M = g * (6371 km) ** 2 / G to kg
{g: g, off_by: percent(g.value / g0 - 1, 2), earth: M}"#),
        ("a ball thrown at 20 m/s, 35° up: landing, peak and arc", "math.calculus · math.trig · data.charts", r#"v = 20; a = 35 deg; g = 9.81
y = |t| v * sin(a) * t - g / 2 * t ** 2
land = root(y, 0.1, 10)
peak = root(|t| deriv(y, t), 0, land)
{seconds: round(land, 2), meters: round(v * cos(a) * land, 1), peak: round(y(peak), 2), arc: (0..=30).map(|i| y(land * i / 30)).sparkline}"#),
        ("fractions closing in on pi: convergents of its continued fraction, and how far off each is", "math.numtheory · data.lists", r#"terms = pi.cfrac(5)
fold = |ts| ts.reverse.drop(1).reduce(ts.last, |acc, t| t + 1 / acc)
(1..=5).map(|n| fold(terms.take(n))).map(|c| [c to frac, sci(abs(c - pi), 1)])"#),
        ("a cookie recipe for 24, scaled to 60 and weighed instead of scooped", "units.kitchen · data.maps", r#"cups = {flour: 2.25 cup, sugar: 0.75 cup, butter: 1 cup, honey: 2 tbsp}
cups.entries.map(|[k, v]| [k, round(v * 60 / 24 * density(k) to g)]).from_entries"#),
        ("a Fermi estimate with honest error bars: piano tuners in Chicago", "math.uncertainty · units", r#"pianos = (2.7e6 ± 0.1e6) / (2.5 ± 0.5) * (5% ± 2%)    # people / household * share with a piano
per_tuner = (4 ± 1) / d * 250 d / yr
pianos / yr / per_tuner"#),
        ("three receipts, three unknown prices: coffee, bagel, juice", "math.linalg · units.money", r#"# rows: [coffees, bagels, juices] on each receipt
solve([[2, 1, 0], [1, 2, 1], [0, 1, 2]], [$9.50, $11.50, $8.50])"#),
    ]),
    ("text, data and dev", &[
        ("how far apart were the errors in a log, and what were they?", "text · time · math.stats · data.lists", r#"log = "08:59:58 INFO start
09:00:03 ERROR db timeout
09:00:41 INFO retry
09:02:15 ERROR db timeout
09:07:50 ERROR disk full"
errors = log.grep("ERROR")
times = errors.map(|l| date("2026-10-06 " + l.match(r"^\S+")))
{gaps: times.deltas.map(|d| d.parts), kinds: errors.map(|l| l.split("ERROR ")[1]).count_by(|k| k)}"#),
        ("crack a Caesar cipher: try every shift, keep the one with the most real words", "text.ciphers · text.layout · data", r#"secret = "wkh vhfuhw sdvvzrug lv vzruglvk"
common = ["the", "is", "and", "a", "of", "to", "secret", "password", "in"]
score = |s| s.words.filter(|w| w in common).len
(0..26).map(|k| secret.caesar(-k)).sort(score).last"#),
        ("a running log as CSV, with h:m:s times that become durations by a dot product", "fs · data.tables · math.linalg · units", r#"runs = "date,km,time\n2026-09-01,5,0:27:30\n2026-09-04,10,0:58:10\n2026-09-08,5,0:26:05\n2026-09-12,21.1,2:05:40".from_csv
dur = |t| dot(t.split(":").map(int), [1 h, 1 min, 1 s])
pretty(runs.map(|r| {date: r.date, km: r.km, per_km: dur(r.time) / r.km to min}).table.sort_by("per_km"))"#),
        ("carve a /22 into /24s and see which one each server lands in", "dev.net · data.tables", r#"servers = ["10.0.1.20", "10.0.3.7", "10.0.3.250", "10.0.0.9"]
pretty(subnets("10.0.0.0/22", 24).map(|b| {
  block: b, hosts: cidr(b).hosts, servers: servers.filter(|s| in_cidr(s, b)).join(" "),
}).table)"#),
        ("six hues, each as light as it can be while still readable on white", "dev.colors · data.lists", r#"readable = |h| (20..=70).map(|l| hsl(h, 75, l)).filter(|c| contrast(c, "white") >= 4.5).last
(0..6).map(|i| readable(i * 60)).map(|c| "{swatch(c)} {round(contrast(c, "white"), 2)}").join("\n")"#),
        ("when were this token and this ID minted, in Tokyo time?", "dev · dev.ids · time.zones", r#"token = jwt("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJpYXQiOjE1MTYyMzkwMjJ9.c2ln")
id = uuid_info("01890a5d-ac96-774b-bcce-b302099a8057")
[token.payload.iat, id.timestamp].map(|d| [d to "Tokyo", d.relative])"#),
        ("anagram groups: words that sort to the same letters", "text.layout · data.lists", r#"words = "listen silent enlist google tinsel inlets banana stone tones notes onset".words
words.group_by(|w| w.chars.sort.join).values.filter(|g| g.len > 1)"#),
        ("a spellchecker in one line: each word's closest match in a dictionary", "text.compare · text.layout", r#"dict = "the quick brown fox jumps over lazy dog".words
"teh qiuck bronw fox jumsp ovr the lazzy dgo".words.map(|w| w.closest(dict)).join(" ")"#),
        ("newest release per major version, by semver precedence rather than string order", "dev · data.lists · data.maps", r#"tags = ["1.9.3", "1.10.0", "2.0.0-rc.1", "2.0.0", "1.2.11", "2.1.0-beta"]
tags.group_by(|t| semver(t).major).map_values(|vs| vs.sort(semver).last)"#),
        ("how many files until two CRC-32s probably collide?", "math.random · math.calculus", r#"half = root(|k| birthday_paradox(round(k), 2 ** 32) - 0.5, 1, 1e6)
{even_odds_at: round(half), at_10k_files: odds(birthday_paradox(10000, 2 ** 32))}"#),
        ("proof of work: the first nonce whose SHA-256 starts with 000", "text.hash · math", r#"n = 0
while !"zil:{n}".sha256.starts_with("000") { n += 1 }
{nonce: n, hash: "zil:{n}".sha256[..16], expected_tries: 16 ** 3}"#),
    ]),
    ("puzzles and play", &[
        ("prime dates: days of 2027 whose YYYYMMDD is prime", "math.numtheory · time · data.lists", r#"days = (0..365).map(|i| date(2027, 1, 1) + i * 1 d)
primes = days.filter(|d| is_prime(int(d.format("%Y%m%d"))))
{count: primes.len, first: primes.take(3).map(|d| d.format("%b %d")), by_month: primes.count_by(|d| d.format("%b"))}"#),
        ("numbers that are palindromes in both decimal and binary", "math.bases · data.lists", r#"pal = |n| n.digits == n.digits.reverse
(1..1000).filter(|n| pal(n) && pal(n to bin))"#),
        ("which points are in the Mandelbrot set? escape step, or stays", "math.complex · data.maps", r#"escapes = fn(c) { z = 0; for n in 1..=50 { z = z * z + c; if abs(z) > 2 { return n } }; return "stays" }
[0, -1, 1, 1i, 0.3 + 0.5i, -0.75 + 0.1i].map(|c| [str(c), escapes(c)]).from_entries"#),
        ("the exact chance of a poker full house", "math.numtheory · math.random", r#"p = 13 * choose(4, 3) * 12 * choose(4, 2) / choose(52, 5)
[p to frac, odds(p)]"#),
    ]),
];

/// Recipes that touch files, the shell or the network: shown, never run (help would change things or hang offline).
#[rustfmt::skip]
pub const ADVANCED_UNRUN: (&str, &[Recipe]) = ("files, shell and network (shown, not run)", &[
    ("disk usage by file type, as a bar chart", "fs · data.lists · data.maps · data.charts", r#"files = glob("**/*").filter(|f| f.type == "file")
files.group_by(|f| f.name.ext).map_values(|fs| fs.map(|f| f.size).sum to KB).bars"#),
    ("which weekdays you commit on", "sys · time · data.lists · data.charts", r#"sh("git log --format=%ad --date=short").lines.count_by(|d| date(d).weekday).bars"#),
    ("PATH entries that point nowhere", "sys · fs", r#"sep = if env("OS") == "Windows_NT" { ";" } else { ":" }
env("PATH").split(sep).filter(|p| !exists(p))"#),
    ("files with the most TODOs", "fs · text · data.tables", r#"todos = glob("src/**/*.rs").map(|f| {file: f.name, todos: read_file(f.name).grep("TODO").len})
todos.filter(|r| r.todos > 0).table.sort_by_desc("todos")"#),
    ("sort Downloads into a folder per month", "fs · time", r#"for f in ls("Downloads").where(|f| f.type == "file") {
  dir = path_join("Downloads", f.modified.format("%Y-%m"))
  mkdir(dir)
  mv(path_join("Downloads", f.name), path_join(dir, f.name))
}"#),
    ("a weight log: zil weigh.zil 71.2 adds today's entry and charts them all", "sys · fs · time · data.charts", r#"if !exists("weight.csv") { write_file("weight.csv", "date,kg") }
append_file("weight.csv", "{today.format("%Y-%m-%d")},{args()[0]}")
read_file("weight.csv").from_csv.kg.sparkline"#),
    ("describe any CSV column: zil stats.zil data.csv price", "sys · fs · math.stats", r#"[file, col] = args()
read_file(file).from_csv.map(|r| r[col]).describe"#),
    ("GitHub stars, straight from the API", "sys · fs", r#"["rust-lang/rust", "zbu1/zil"].map(|r| fetch("https://api.github.com/repos/{r}").from_json.stargazers_count)"#),
    ("the weather in Oslo, in Fahrenheit", "sys · fs · units", r#"now_oslo = fetch("https://wttr.in/Oslo?format=j1").from_json.current_condition[0]
{temp: int(now_oslo.temp_C) * 1 C to F, feels_like: int(now_oslo.FeelsLikeC) * 1 C to F, sky: now_oslo.weatherDesc[0].value}"#),
    ("a 9-day trip at $150 a day, in local money (live rates)", "sys · units.money · geo", r#"{japan: $150 * 9 to "Japan", vietnam: $150 * 9 to "Vietnam", mexico: $150 * 9 to "Mexico"}"#),
    ("one phrase, three languages", "text.translation · sys", r#"["es", "ja", "de"].map(|l| "where is the train station?".translate(l))"#),
]);

#[cfg(test)]
mod tests {
    use super::TOUR;

    /// Every step runs in one session, as the sandbox's REPL would; the currency one only has to parse, since tests are offline.
    #[test]
    fn tour_runs() {
        let (mut interp, mut n) = (crate::Session::new(crate::host::get()), 0);
        for (text, src) in TOUR.iter().flat_map(|c| c.1) {
            assert_eq!(text.matches('`').count() % 2, 0, "{text}");
            if src.contains("USD to EUR") {
                assert!(crate::parser::parse(src).is_ok(), "{src}");
            } else if !src.is_empty() {
                let v = interp.eval(src).unwrap_or_else(|e| panic!("{src}: {}", e.msg));
                crate::repl::record(&mut interp, &mut n, src, &v);
            }
        }
        assert!(TOUR.last().unwrap().1.last().unwrap().1.is_empty(), "the last step opens Script mode");
    }
}
