# zil

[![crates.io](https://img.shields.io/crates/v/zil.svg)](https://crates.io/crates/zil)
[![docs](https://img.shields.io/badge/docs-reference-blue)](https://zil.alexander-bui.com/)
[![sandbox](https://img.shields.io/badge/try_it-sandbox-orange)](https://zil.alexander-bui.com/play/)
[![license](https://img.shields.io/badge/license-MIT-green)](LICENSE.md)

zil is a terse calculator and scripting language for units, dates and much more.

The idea is to pack as much power as possible into as little syntax as possible. There are no imports, no type annotations and no boilerplate.
Units attach to numbers just like you'd write them on paper. `to` converts anything into anything it can. Methods chain left to right, so
you can read a line in the same order it runs. If you can guess how something should be written, it probably works. If it doesn't, the
error message will tell you what to try instead.

Type what you mean and get a real answer:

```zil
100 km / 2 h to mph                      # → 31.0686 mph
1.8 m to ft in                           # → 5 ft 10.8661 in
date("2026-12-25 18:30") to "Asia/Tokyo" # → 2026-12-26 09:30:00 +09:00
0.1 + 0.2 == 0.3                         # → true
2 ** 100                                 # → 1267650600228229401496703205376
"a1b22c333".nums.sum                     # → 356
```

## Sandbox

You can try zil in the browser at [zil.alexander-bui.com](https://zil.alexander-bui.com/) without installing anything. 

## Features

- **Units.** Quantities carry their units through arithmetic. `4.7 GB / 50 Mbps` gives a time, and adding meters to seconds is an error. `to`
  converts between compatible units.
- **Exact numbers.** Fractions stay fractions and integers grow as needed. Floats are only used when you ask for them.
- **Large standard library.** Dates, regex, hashing, statistics, calculus, colors, IP addresses and ASCII art are built in. There is nothing to
  import.
- **Error messages.** Errors point to the location in the source and often suggest a fix.
- **Built-in help.** `help(upper)` shows examples for a function. `help("km")` lists units. Most of the documentation lives here, so
  `help` is the best place to start exploring.

## Examples

### Units and currencies

```zil
350 F to C                      # → 176.667 C
4.7 GB / 50 Mbps to min         # → 12.5333 min
9.81 m/s^2 * 70 kg to N         # → 686.7 N
(5 m ± 0.2 m) * (3 m ± 0.1 m)   # → 15 ± 0.781025 m^2
100 USD to EUR                  # → €88.87
```

### Dates and times

```zil
date("2026-12-25") - date("2026-10-07") to d   # → 79 d
now + 90 d                                     # → 2027-01-05 20:39:42 -06:00
date("2026-10-07").weekday                     # → Wednesday
```

### Math

```zil
1/3 + 1/6 to frac              # → 1/2
factorial(30)                  # → 265252859812191058636308480000000
is_prime(2 ** 61 - 1)          # → true
csqrt(-4)                      # → 2i
integrate(|x| x ** 2, 0, 3)    # → 9
255 to hex                     # → 0xff
```

### Scripting

zil has variables, functions, loops, maps, string interpolation, and most other things you will find in scripting languages, all available by default.

```zil
fib = fn(n) {
  if n < 2 { return n }
  fib(n - 1) + fib(n - 2)
}
print((1..11).map(fib))        # [1, 1, 2, 3, 5, 8, 13, 21, 34, 55]

scores = {ana: 91, ben: 78}
for name in scores {
  grade = if scores[name] >= 90 { "A" } else { "B" }
  print("{name.capitalize}: {grade}")
}

"the quick brown fox".split.map(|w| w.capitalize).join(" ")   # → The Quick Brown Fox
[3, 1, 4, 1, 5, 9] |> sort |> map(|x| x * 2)                  # → [2, 2, 6, 8, 10, 18]
"hello".sha256[..12]                                          # → 2cf24dba5fb0
```

### Files and scripts

Files, folders, CSV and JSON are all built in, so a zil script can stand in for a shell script.

```zil
# the biggest source files
glob("src/**/*.rs").sort_by_desc("size").take(3)

# rename every .jpeg to .jpg
for f in glob("photos/*.jpeg") {
  mv(f.name, path_join(dirname(f.name), stem(f.name) + ".jpg"))
}

# total each item in a CSV and save the result as JSON
sales = read_file("sales.csv").from_csv
totals = sales.group_by(|r| r.item).map_values(|rs| rs.map(|r| r.qty * r.price).sum)
write_file("totals.json", totals.to_json)   # {"widget":12.5,"gadget":10}

# read JSON
pkg = read_file("package.json").from_json
print("{pkg.name} v{pkg.version}")

# keep a log; run with `zil log.zil 72.4`
append_file("weight.csv", "{today.format("%Y-%m-%d")},{args()[0]}")
```

### Fun

```zil
cowsay(excuse())      # a new excuse every time
mandelbrot(40, 12)
```

```
 ________________________________________
/ A goose blocked the path and would not \                           ...:
\ negotiate.                             /                        ...:@@@..
 ----------------------------------------                      ..#+=@@@@@@@:#:.
   \                                                   .........#@@@@@@@@@@@@-.
    \                                                 ..:@@@@@:@@@@@@@@@@@@@@@-
         (__)                                 @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@-..
         (oo)                                         ..:@@@@@:@@@@@@@@@@@@@@@-
  /-------\/                                           .........#@@@@@@@@@@@@-.
 / |     ||                                                    ..#+=@@@@@@@:#:.
*  ||----||                                                       ...:@@@..
   ~~    ~~                                                          ...:
```

There's a lot more where that came from! The best way to learn what zil can do is the help menu: start the REPL, type `help`, and dig in. Every
function comes with live examples. The [`examples/`](examples) folder has more scripts to read and run.

## Install

Download a prebuilt binary from the [latest release](https://github.com/zbu1/zil/releases/latest):

| Platform              | Download                                                                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Windows x64 installer | [zil-x86_64-pc-windows-msvc.msi](https://github.com/zbu1/zil/releases/latest/download/zil-x86_64-pc-windows-msvc.msi) |
| Windows x64 portable  | [zil-x86_64-pc-windows-msvc-portable.zip](https://github.com/zbu1/zil/releases/latest/download/zil-x86_64-pc-windows-msvc-portable.zip) |
| Windows x64           | [zil-x86_64-pc-windows-msvc.zip](https://github.com/zbu1/zil/releases/latest/download/zil-x86_64-pc-windows-msvc.zip) |
| macOS (Apple Silicon) | [zil-aarch64-apple-darwin.tar.gz](https://github.com/zbu1/zil/releases/latest/download/zil-aarch64-apple-darwin.tar.gz) |
| macOS (Intel)         | [zil-x86_64-apple-darwin.tar.gz](https://github.com/zbu1/zil/releases/latest/download/zil-x86_64-apple-darwin.tar.gz) |
| Linux x64             | [zil-x86_64-unknown-linux-gnu.tar.gz](https://github.com/zbu1/zil/releases/latest/download/zil-x86_64-unknown-linux-gnu.tar.gz) |
| Linux ARM64           | [zil-aarch64-unknown-linux-gnu.tar.gz](https://github.com/zbu1/zil/releases/latest/download/zil-aarch64-unknown-linux-gnu.tar.gz) |

The Windows installer puts `zil` on your PATH and adds a Start menu shortcut to the REPL. The portable zip keeps its history and caches
in its own `zil-data` folder and writes nothing anywhere else, so it can run from a USB stick. Any zil binary does the same if you make a
`zil-data` folder next to it. Otherwise zil keeps them in `%LOCALAPPDATA%\zil` on Windows and `~/.cache/zil` elsewhere.

Or build it with Cargo:

```sh
cargo install zil-cli
```

To install the latest version from git:

```sh
cargo install --git https://github.com/zbu1/zil zil-cli
```

## Usage

```
zil                        start the REPL
zil -e <code> [args...]    evaluate code and print the result
zil <file.zil> [args...]   run a script; args() lists the extra arguments
zil --version              print the version
```

The REPL keeps history, accepts multi-line input, and stores the last result in `_`. Piped input is available as `input`:

```sh
echo "hi there" | zil -e 'input.upper'   # → HI THERE
```

## Documentation

Most of the documentation is in the help menu: run `help` in the REPL for an overview, or `help(<name>)` for any function or unit.

The full reference is available online at [zil.alexander-bui.com](https://zil.alexander-bui.com/). To build it locally, see below.

## Building

```sh
git clone https://github.com/zbu1/zil
cd zil
cargo build --release   # binary at target/release/zil
cargo test
```

The site is [Astro](https://astro.build) with [Starlight](https://starlight.astro.build): the reference at the root, generated from the module tree,
and the sandbox at `/play/`. To run it locally, install the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version
in Cargo.lock, then run `pnpm install && pnpm build:docs && pnpm wasm && pnpm dev`.

## Data sources

Country and city data in `crates/zil/src/modules/geo/` comes from [GeoNames](https://www.geonames.org/) under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/),
trimmed to capitals and cities with more than 500,000 people.

Currency rates are provided by [Exchange Rate API](https://www.exchangerate-api.com).

## License

[MIT](LICENSE.md)
