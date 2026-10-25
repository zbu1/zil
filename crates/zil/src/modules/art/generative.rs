//! Generative art: L-system turtle drawings and named fractals, elementary cellular automata, the Ulam spiral, starfields;
//! the Mandelbrot set, Conway's Game of Life and random mazes.

use super::Dots;
use crate::modules::{Call, Fail, Module};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

pub const MODULE: Module = Module {
    name: "generative",
    about: "pictures grown from simple rules: fractals, cellular automata, the Game of Life, mazes, starfields",
    #[rustfmt::skip]
    examples: &[
        ("grow things", &[
            ("a dragon curve", r#"fractal("dragon", 8, 30)"#),
            ("a tree in a box", r#"fractal("tree", 6, 24).boxed("round")"#),
            ("Sierpinski's triangle, two ways", r#"rule(90, 32, 16).beside(fractal("sierpinski", 4, 16), 4)"#),
            ("a plant from your own rules", r#"lsystem("X", {X: "F[-X][X]F[-X]+FX", F: "FF"}, 25, 4, 30)"#),
            ("the Mandelbrot set", "mandelbrot()"),
            ("a glider, 8 steps on", r#"life("glider", 8)"#),
            ("start from your own pattern", r#"life(".#.\n.#.\n.#.", 1)"#),
            ("a maze", "maze(12, 6)"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Dots, PRESETS, expand, toys, turtle};
    use crate::modules::Fail;
    use crate::signature::{As, Map};
    use crate::value::Value;
    use std::collections::HashMap;

    /// `n` if it's from `lo` to `hi`, else an error blaming argument `i`.
    fn int(i: usize, n: i64, lo: i64, hi: i64) -> Result<i64, Fail> {
        if (lo..=hi).contains(&n) { Ok(n) } else { Err(Fail::Arg(i, format!("expected an int from {lo} to {hi}"))) }
    }

    /// grow and draw an L-system
    ///
    /// Start from the axiom string and rewrite it n times, replacing each character that has a rule (a map from a character to its replacement). Then draw the result in braille dots, width characters wide, with a pen that starts facing up: F and G draw a step, f moves a step without drawing, + and - turn by angle degrees, | turns around, [ saves the pen's place and ] goes back to it. Other characters do nothing
    #[builtin(examples = [r#"lsystem("F", {F: "F+F-F-F+F"}, 90, 3, 30)"#], see = ["fractal"])]
    fn lsystem(axiom: &str, rules: &Map, angle: f64, n: i64, #[default(60)] width: i64) -> Result<String, Fail> {
        let mut table = HashMap::new();
        for (k, v) in rules.borrow().iter() {
            let mut cs = k.chars();
            let (Some(c), None) = (cs.next(), cs.next()) else { return Err(Fail::Arg(1, format!("rule keys are single characters, got {k:?}"))) };
            let Value::Str(v) = v else { return Err(Fail::Arg(1, format!("rule {k} must map to a string"))) };
            table.insert(c, v.to_string());
        }
        let program = expand(axiom, &table, int(3, n, 0, 30)? as usize)?;
        turtle(&program, angle, 90.0, int(4, width, 4, 500)? as usize)
    }

    /// draw a well-known fractal by name
    ///
    /// The names are "dragon", "koch", "hilbert", "sierpinski", "levy", "gosper", "tree", "fern" and "bush". n is how many times to rewrite, so higher is more detailed (each fractal has its own usual depth), and width is as for lsystem. With no name, the list of names
    #[builtin(examples = [r#"fractal("koch", 3, 30)"#], see = ["lsystem", "rule"])]
    fn fractal(name: &str, n: Option<i64>, #[default(60)] width: i64) -> Result<String, Fail> {
        let &(_, axiom, rules, angle, heading, depth) =
            PRESETS.iter().find(|p| p.0 == name).ok_or_else(|| Fail::Arg(0, format!("no fractal {name:?}; fractal() lists them")))?;
        let rules = rules.iter().map(|(c, r)| (*c, r.to_string())).collect();
        let program = expand(axiom, &rules, int(1, n.unwrap_or(depth), 0, 30)? as usize)?;
        turtle(&program, angle, heading, int(2, width, 4, 500)? as usize)
    }

    #[builtin(name = "fractal", examples = ["fractal()"])]
    fn fractals() -> Vec<Value> {
        PRESETS.iter().map(|p| Value::str(p.0)).collect()
    }

    /// run a cellular automaton
    ///
    /// A row of cells, each on or off, where each new row follows from the one above by the rule numbered n (0-255). It starts from a single live cell in a row width cells wide whose ends wrap around. Draws steps rows (half the width without steps), two rows per line of text
    #[builtin(examples = ["rule(30, 32)", "rule(90, 32, 16)"], see = ["life", "fractal"])]
    fn rule(n: i64, #[default(64)] width: i64, steps: Option<i64>) -> Result<String, Fail> {
        let n = int(0, n, 0, 255)? as u8;
        let w = int(1, width, 3, 1000)? as usize;
        let steps = int(2, steps.unwrap_or(w as i64 / 2), 1, 1000)? as usize;
        let mut row = vec![false; w];
        row[w / 2] = true;
        let mut rows = vec![row.clone()];
        for _ in 1..steps {
            row = (0..w).map(|i| n >> ((row[(i + w - 1) % w] as u8) << 2 | (row[i] as u8) << 1 | row[(i + 1) % w] as u8) & 1 == 1).collect();
            rows.push(row.clone());
        }
        let lines = rows.chunks(2).map(|pair| {
            let line = (0..w).map(|i| match (pair[0][i], pair.get(1).is_some_and(|b| b[i])) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                _ => ' ',
            });
            line.collect::<String>().trim_end().to_string()
        });
        Ok(lines.collect::<Vec<_>>().join("\n"))
    }

    /// an Ulam spiral: the numbers 1, 2, 3, ... wound outward from the center, with a dot for each prime
    ///
    /// The primes mysteriously line up along diagonals. size characters wide
    #[builtin(examples = ["spiral(20)"], see = ["is_prime"])]
    fn spiral(#[default(40)] size: i64) -> Result<String, Fail> {
        let d = int(0, size, 2, 200)? as usize * 2;
        let n = d * d;
        let mut composite = vec![false; n + 1];
        composite[0] = true;
        composite[1] = true;
        for i in 2..=n.isqrt() {
            if !composite[i] {
                (i * i..=n).step_by(i).for_each(|j| composite[j] = true);
            }
        }
        let mut dots = Dots::new(d, d);
        let (mut x, mut y, mut k) = ((d / 2) as i64, (d / 2) as i64, 1);
        let mut set = |x: i64, y: i64, k: usize| {
            if !composite[k] {
                dots.set(x, y);
            }
        };
        set(x, y, 1);
        // Runs of 1, 1, 2, 2, 3, 3… going right, up, left, down.
        'walk: for run in 1.. {
            for (dx, dy) in [(1, 0), (0, -1), (-1, 0), (0, 1)].into_iter().skip((run - 1) % 2 * 2).take(2) {
                for _ in 0..run {
                    (x, y, k) = (x + dx, y + dy, k + 1);
                    if k > n {
                        break 'walk;
                    }
                    set(x, y, k);
                }
            }
        }
        Ok(dots.render())
    }

    /// a random night sky
    ///
    /// Give the same seed to get the same sky again
    #[builtin(examples = ["starfield(30, 4, 7)"], see = ["negative"])]
    fn starfield(width: i64, height: i64, seed: Option<i64>) -> Result<String, Fail> {
        let (w, h) = (int(0, width, 1, 1000)?, int(1, height, 1, 1000)?);
        let mut rng = match seed {
            None => crate::host::rng(|r| r.fork()),
            Some(s) => fastrand::Rng::with_seed(s as u64),
        };
        let star = |r: f64| match r {
            r if r < 0.004 => '✦',
            r if r < 0.012 => '*',
            r if r < 0.02 => '+',
            r if r < 0.045 => '.',
            r if r < 0.07 => '·',
            _ => ' ',
        };
        let lines = (0..h).map(|_| (0..w).map(|_| star(rng.f64())).collect::<String>().trim_end().to_string());
        Ok(lines.collect::<Vec<_>>().join("\n"))
    }

    /// the Mandelbrot set, the famous fractal, drawn in ASCII characters
    #[builtin(examples = ["mandelbrot(40, 12)"], see = ["life"])]
    fn mandelbrot(#[default(72)] width: i64, #[default(24)] height: i64) -> Result<As<String>, Fail> {
        toys("mandelbrot", &[Value::int(width), Value::int(height)]).map(As::from)
    }

    /// run Conway's Game of Life for steps generations and draw the live cells
    ///
    /// On a grid, a live cell survives with 2 or 3 live neighbours, and an empty cell comes alive with exactly 3. start is a picture with # for live cells and . for empty ones, or a named pattern: "glider", "blinker", "pulsar" or "r_pentomino". The board grows as needed, unless wrap is true: then it stays start's size and cells leaving one edge come back on the other
    #[builtin(examples = [r#"life("blinker")"#, r#"life("glider", 4)"#], see = ["maze"])]
    fn life(start: &str, #[default(1)] steps: i64, #[default(false)] wrap: bool) -> Result<As<String>, Fail> {
        toys("life", &[Value::str(start), Value::int(steps), Value::Bool(wrap)]).map(As::from)
    }

    /// a random maze with exactly one path between any two points
    #[builtin(examples = ["maze(6, 3)"], see = ["life"])]
    fn maze(#[default(20)] width: i64, #[default(10)] height: i64) -> Result<As<String>, Fail> {
        toys("maze", &[Value::int(width), Value::int(height)]).map(As::from)
    }
}

/// Rewrite every character with its rule n times.
fn expand(axiom: &str, rules: &HashMap<char, String>, n: usize) -> Result<String, Fail> {
    let mut s = axiom.to_string();
    for _ in 0..n {
        s = s.chars().map(|c| rules.get(&c).map_or_else(|| c.to_string(), Clone::clone)).collect();
        if s.len() > 2_000_000 {
            return Err("that grows past 2 million characters; lower n".into());
        }
    }
    Ok(s)
}

/// Walk the turtle program, then fit its path into a braille box `width` characters wide.
fn turtle(program: &str, angle: f64, heading: f64, width: usize) -> Result<String, Fail> {
    let (mut pos, mut dir) = ((0.0f64, 0.0f64), heading);
    let mut saved = vec![];
    let mut segs = vec![];
    for c in program.chars() {
        match c {
            'F' | 'G' | 'f' => {
                let next = (pos.0 + dir.to_radians().cos(), pos.1 + dir.to_radians().sin());
                if c != 'f' {
                    segs.push((pos, next));
                }
                pos = next;
            }
            '+' => dir += angle,
            '-' => dir -= angle,
            '|' => dir += 180.0,
            '[' => saved.push((pos, dir)),
            ']' => (pos, dir) = saved.pop().unwrap_or((pos, dir)),
            _ => {}
        }
    }
    if segs.is_empty() {
        return Err("nothing to draw: the program has no F or G".into());
    }
    let pts = segs.iter().flat_map(|(a, b)| [a, b]);
    let (x0, x1, y0, y1) = pts.fold((f64::MAX, f64::MIN, f64::MAX, f64::MIN), |(x0, x1, y0, y1), p| (x0.min(p.0), x1.max(p.0), y0.min(p.1), y1.max(p.1)));
    // Dots are about square; cap the height at as many dots as the width.
    let w = (width * 2) as f64;
    let scale = ((w - 1.0) / (x1 - x0).max(1e-9)).min((w - 1.0) / (y1 - y0).max(1e-9));
    let mut dots = Dots::new(((x1 - x0) * scale) as usize + 1, ((y1 - y0) * scale) as usize + 1);
    let at = |p: &(f64, f64)| ((p.0 - x0) * scale, (y1 - p.1) * scale);
    for (a, b) in &segs {
        dots.line(at(a), at(b));
    }
    Ok(dots.render())
}

type Preset = (&'static str, &'static str, &'static [(char, &'static str)], f64, f64, i64);

/// Name, axiom, rules, turn angle, starting heading (degrees, 0 = right, 90 = up), default depth.
#[rustfmt::skip]
const PRESETS: &[Preset] = &[
    ("dragon", "FX", &[('X', "X+YF+"), ('Y', "-FX-Y")], 90.0, 0.0, 10),
    ("koch", "F--F--F", &[('F', "F+F--F+F")], 60.0, 0.0, 3),
    ("hilbert", "A", &[('A', "+BF-AFA-FB+"), ('B', "-AF+BFB+FA-")], 90.0, 0.0, 5),
    ("sierpinski", "F-G-G", &[('F', "F-G+F+G-F"), ('G', "GG")], 120.0, 180.0, 5),
    ("levy", "F", &[('F', "+F--F+")], 45.0, 0.0, 10),
    ("gosper", "F", &[('F', "F-G--G+F++FF+G-"), ('G', "+F-FF--G-F+G+G")], 60.0, 0.0, 3),
    ("tree", "X", &[('X', "F[+X][-X]"), ('F', "FF")], 30.0, 90.0, 7),
    ("fern", "X", &[('X', "F+[[X]-X]-F[-FX]+X"), ('F', "FF")], 25.0, 70.0, 5),
    ("bush", "F", &[('F', "FF+[+F-F-F]-[-F+F+F]")], 22.5, 90.0, 4),
];

/// The toys: mandelbrot, life and maze.
fn toys(name: &'static str, args: &[Value]) -> Call {
    use Value::*;
    let size = |i: usize, default: i64, max: i64| match args.get(i) {
        None => Ok(default as usize),
        Some(Int(n, _)) if (1..=max).contains(n) => Ok(*n as usize),
        Some(_) => Err(Fail::Arg(i, format!("expected an int from 1 to {max}"))),
    };
    Ok(match (name, args) {
        ("mandelbrot", [] | [Int(..)] | [Int(..), Int(..)]) => {
            let (w, h) = (size(0, 72, 500)?, size(1, 24, 500)?);
            const SHADES: &[u8] = b" .:-=+*#%@";
            let rows: Vec<String> = (0..h)
                .map(|y| {
                    (0..w)
                        .map(|x| {
                            let c = (-2.2 + 3.0 * x as f64 / w as f64, -1.2 + 2.4 * y as f64 / h as f64);
                            let (mut zr, mut zi, mut i) = (0.0f64, 0.0f64, 0);
                            while i < 50 && zr * zr + zi * zi <= 4.0 {
                                (zr, zi) = (zr * zr - zi * zi + c.0, 2.0 * zr * zi + c.1);
                                i += 1;
                            }
                            if i == 50 { '@' } else { SHADES[i * (SHADES.len() - 1) / 50] as char }
                        })
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect();
            Value::str(rows.join("\n"))
        }
        ("life", [Str(start), rest @ ..]) if rest.len() <= 2 => {
            let steps = match rest.first() {
                None => 1,
                Some(Int(n, _)) if (0..=10_000).contains(n) => *n,
                _ => return Err(Fail::Arg(1, "steps must be an int from 0 to 10000".into())),
            };
            let wrap = match rest.get(1) {
                None => false,
                Some(Bool(b)) => *b,
                _ => return Err(Fail::Arg(2, "wrap must be true or false".into())),
            };
            let pattern = PATTERNS.iter().find(|p| p.0 == &**start).map_or(&**start, |p| p.1);
            let mut live: HashSet<(i64, i64)> = HashSet::new();
            for (y, line) in pattern.lines().enumerate() {
                for (x, c) in line.chars().enumerate() {
                    match c {
                        '#' | '█' | 'O' | 'o' | '*' => _ = live.insert((x as i64, y as i64)),
                        '.' | ' ' | '_' => {}
                        _ => return Err(Fail::Arg(0, format!("unknown cell {c:?}: use # for alive and . for dead, or a pattern name"))),
                    }
                }
            }
            let size = (pattern.lines().map(|l| l.chars().count()).max().unwrap_or(0) as i64, pattern.lines().count() as i64);
            let board = wrap.then_some(size);
            for _ in 0..steps {
                live = step(&live, board);
                if live.len() > 100_000 {
                    return Err("too many live cells".into());
                }
            }
            if wrap {
                let rows = (0..size.1).map(|y| (0..size.0).map(|x| if live.contains(&(x, y)) { '#' } else { '.' }).collect::<String>());
                return Ok(Value::str(rows.collect::<Vec<_>>().join("\n")));
            }
            Value::str(draw(&live))
        }
        ("maze", [] | [Int(..)] | [Int(..), Int(..)]) => {
            let (w, h) = (size(0, 20, 200)?, size(1, 10, 200)?);
            // Grid of walls; cell (x, y) sits at (2x+1, 2y+1). Carve with an iterative depth-first search.
            let (gw, gh) = (2 * w + 1, 2 * h + 1);
            let mut open = vec![vec![false; gw]; gh];
            let mut stack = vec![(0usize, 0usize)];
            open[1][1] = true;
            while let Some(&(x, y)) = stack.last() {
                let mut next: Vec<(usize, usize)> = [(0, -1), (1, 0), (0, 1), (-1, 0)]
                    .iter()
                    .map(|(dx, dy)| (x as i64 + dx, y as i64 + dy))
                    .filter(|(nx, ny)| (0..w as i64).contains(nx) && (0..h as i64).contains(ny))
                    .map(|(nx, ny)| (nx as usize, ny as usize))
                    .filter(|(nx, ny)| !open[2 * ny + 1][2 * nx + 1])
                    .collect();
                if next.is_empty() {
                    stack.pop();
                    continue;
                }
                crate::host::rng(|r| r.shuffle(&mut next));
                let (nx, ny) = next[0];
                open[y + ny + 1][x + nx + 1] = true;
                open[2 * ny + 1][2 * nx + 1] = true;
                stack.push((nx, ny));
            }
            // Entrance top-left, exit bottom-right.
            open[0][1] = true;
            open[gh - 1][gw - 2] = true;
            Value::str(open.iter().map(|row| row.iter().map(|o| if *o { "  " } else { "██" }).collect::<String>()).collect::<Vec<_>>().join("\n"))
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// One generation; on a `board` of (width, height) the edges join.
fn step(live: &HashSet<(i64, i64)>, board: Option<(i64, i64)>) -> HashSet<(i64, i64)> {
    let mut counts = std::collections::HashMap::new();
    for (x, y) in live {
        for dx in -1..=1 {
            for dy in -1..=1 {
                if (dx, dy) != (0, 0) {
                    let c = board.map_or((x + dx, y + dy), |(w, h)| ((x + dx).rem_euclid(w), (y + dy).rem_euclid(h)));
                    *counts.entry(c).or_insert(0) += 1;
                }
            }
        }
    }
    counts.into_iter().filter(|(c, n)| *n == 3 || (*n == 2 && live.contains(c))).map(|(c, _)| c).collect()
}

/// The live cells' bounding box as rows of # and .; "(empty)" when everything died.
fn draw(live: &HashSet<(i64, i64)>) -> String {
    let (Some(x0), Some(x1), Some(y0), Some(y1)) =
        (live.iter().map(|c| c.0).min(), live.iter().map(|c| c.0).max(), live.iter().map(|c| c.1).min(), live.iter().map(|c| c.1).max())
    else {
        return "(empty)".into();
    };
    (y0..=y1).map(|y| (x0..=x1).map(|x| if live.contains(&(x, y)) { '#' } else { '.' }).collect::<String>()).collect::<Vec<_>>().join("\n")
}

const PATTERNS: &[(&str, &str)] = &[
    ("glider", ".#.\n..#\n###"),
    ("blinker", "###"),
    ("r_pentomino", ".##\n##.\n.#."),
    (
        "pulsar",
        "..###...###..\n.............\n#....#.#....#\n#....#.#....#\n#....#.#....#\n..###...###..\n.............\n..###...###..\n#....#.#....#\n#....#.#....#\n#....#.#....#\n.............\n..###...###..",
    ),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn generative() {
        assert_eq!(show(r#"lsystem("FF+F", {}, 90, 0, 4)"#).lines().count(), 2);
        assert_eq!(show(r#"lsystem("F", {}, 90, 0, 4)"#).lines().count(), 2);
        assert!(try_eval(r#"lsystem("+", {}, 90, 1)"#).is_err());
        assert!(try_eval(r#"lsystem("F", {FF: "F"}, 90, 1)"#).is_err());
        assert!(try_eval(r#"lsystem("F", {F: "FFFFFFFF"}, 90, 30)"#).is_err());
        for name in ["dragon", "koch", "hilbert", "sierpinski", "levy", "gosper", "tree", "fern", "bush"] {
            let art = show(&format!("fractal({name:?}, 3, 20)"));
            assert!(art.lines().all(|l| l.chars().count() <= 20), "{name} too wide:\n{art}");
            assert!(art.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)), "{name} drew nothing");
        }
        assert!(try_eval(r#"fractal("blob")"#).is_err());
        // Rule 90 from one cell is Pascal's triangle mod 2.
        assert_eq!(show("rule(90, 7, 4)"), "  ▄▀▄\n▄▀▄ ▄▀▄");
        assert_eq!(show("rule(0, 5, 2)"), "  ▀");
        assert!(try_eval("rule(256)").is_err());
        // 1 sits at the center; 2 and 3 (primes) are right and up-right of it.
        let s = show("spiral(2)");
        assert_eq!(s.lines().count(), 1);
        assert_eq!(show("starfield(10, 3, 1)"), show("starfield(10, 3, 1)"));
        assert_eq!(show("starfield(10, 3, 1)").split('\n').count(), 3);
    }

    #[test]
    fn toys() {
        let m = show("mandelbrot(40, 12)");
        assert_eq!(m.lines().count(), 12);
        assert!(m.contains('@'));
        assert_eq!(show(r#"life("blinker")"#), "#\n#\n#");
        assert_eq!(show(r#"life("blinker", 2)"#), "###");
        // A glider is itself again after 4 steps, one cell down and right.
        assert_eq!(show(r#"life("glider", 4)"#), show(r#"life("glider", 0)"#));
        assert_eq!(show(r#"life("pulsar", 3)"#), show(r#"life("pulsar", 0)"#));
        assert_eq!(show(r##"life("#", 1)"##), "(empty)");
        // On a wrapping 6×6 board a glider comes back where it started after 24 steps.
        let g = ".#....\n..#...\n###...\n......\n......\n......";
        assert_eq!(show(&format!("life({g:?}, 24, true)")), g);
        assert_eq!(show(&format!("life({g:?}, 1, true)")).lines().count(), 6);
        let mz = show("maze(5, 3)");
        assert_eq!(mz.lines().count(), 7);
        assert!(mz.lines().all(|l| l.chars().count() == 22));
        // A perfect maze on 15 cells has 14 passages: open cells = 15 + 14 + 2 doors.
        let open: usize = mz.lines().map(|l| l.chars().collect::<Vec<_>>().chunks(2).filter(|c| c[0] == ' ').count()).sum();
        assert_eq!(open, 15 + 14 + 2);
        assert!(try_eval(r#"life("x")"#).is_err());
        assert!(try_eval("maze(0)").is_err());
    }
}
