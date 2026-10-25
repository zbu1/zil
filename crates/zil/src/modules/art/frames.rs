//! Frames and composition: boxes, speech bubbles, and art placed beside, above, over, flipped, turned or scaled.

use super::{grid, ungrid};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "frames",
    about: "arrange text pictures: boxes and speech bubbles, side by side, stacked, overlaid, flipped, rotated, scaled",
    #[rustfmt::skip]
    examples: &[
        ("compose", &[
            ("a caption under a picture", r#"clipart("owl").stack("hoo?", "center")"#),
            ("two friends", r#"clipart("cat").beside(clipart("dog"), 4)"#),
            ("a cat says", r#"say("deploy on friday?", "cat")"#),
            ("a picture frame", r#"clipart("house").boxed("double").boxed("shadow")"#),
            ("a sky of black stars on white", r#"starfield(20, 4, 1).negative"#),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("boxed", "boxed(s: str, style?: str) -> str", "draw a box around text", &[r#"boxed("hello")"#, r#""zil\ncalc".boxed("round")"#], &["say", "stack"])
        .detail("style is \"single\" (the default), \"double\", \"round\", \"heavy\", \"ascii\", \"stars\" or \"shadow\""),
    doc("say", "say(s: str, art?: str) -> str", "a clip art picture saying s in a speech bubble: the cow by default, or any clipart() name", &[r#"say("moo")"#, r#"say("pika", "pikachu")"#], &["think", "cowsay", "boxed"]),
    doc("cowsay", "cowsay(s: str, art?: str) -> str", "the same as say, named after the classic Unix program", &[r#"cowsay("moo")"#], &["say"]),
    doc("think", "think(s: str, art?: str) -> str", "like say, but in a thought bubble", &[r#"think("hmm", "ghost")"#], &["say"]),
    doc("beside", "beside(a: str, b: str, gap?: int) -> str", "put picture b to the right of picture a, tops lined up, with gap spaces between (1 by default)", &[r#""a\nb".beside("c\nd\ne")"#], &["stack", "overlay"]),
    doc("stack", "stack(a: str, b: str, align?: str) -> str", "put picture b under picture a, lined up \"left\" (the default), \"center\" or \"right\"", &[r#""wide one".stack("x", "right")"#], &["beside"]),
    doc("overlay", "overlay(a: str, b: str, x: int, y: int) -> str", "draw picture b on top of picture a, with b's top-left corner at column x and row y of a (counting from 0)", &[r#"boxed("     ").overlay("hi", 2, 1)"#], &["beside"])
        .detail("Spaces in b let a show through"),
    doc("mirror", "mirror(s: str, axis?: str) -> str", "flip a picture left to right (\"h\", the default) or upside down (\"v\")", &[r#""(=^.^=)/".mirror"#, r#""/\\\n\\/".mirror("v")"#], &["rotate", "flip"])
        .detail("Slashes and brackets are swapped so the shapes still look right"),
    doc("rotate", "rotate(s: str, degrees: int) -> str", "turn a picture clockwise by 90, 180 or 270 degrees", &[r#""abc\ndef".rotate(90)"#], &["mirror"]),
    doc("scale", "scale(s: str, n: int) -> str", "make a picture n times bigger by repeating each character n times across and n times down", &[r#""ab".scale(2)"#], &["banner"]),
    doc("negative", "negative(s: str) -> str", "swap ink and background: spaces become solid blocks and the other way round, and braille dots are inverted", &[r#""▀ █".negative"#], &["invert"]),
    doc("trim_art", "trim_art(s: str) -> str", "remove blank lines at the top and bottom, indentation every line shares, and spaces at the ends of lines", &[r#""\n   ab\n    c\n\n".trim_art"#], &["dedent"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let int = |i: usize, lo: i64, hi: i64| match args.get(i) {
        Some(Int(n, _)) if (lo..=hi).contains(n) => Ok(*n as usize),
        _ => Err(Fail::Arg(i, format!("expected an int from {lo} to {hi}"))),
    };
    Ok(match (name, args) {
        ("boxed", [Str(s)]) => Value::str(boxed(s, "single")?),
        ("boxed", [Str(s), Str(style)]) => Value::str(boxed(s, style).map_err(|e| Fail::Arg(1, e))?),
        ("say" | "cowsay" | "think", [Str(s), rest @ ..]) if rest.len() <= 1 => {
            let piece = match rest {
                [] => "cow",
                [Str(p)] => p,
                _ => return Err(Fail::BadArgs),
            };
            let art = super::piece(piece).ok_or_else(|| Fail::Arg(1, format!("no {piece:?} in the gallery; clipart() lists them")))?;
            // Line the first row up under the bubble's tail.
            let pad = " ".repeat(4usize.saturating_sub(art.len() - art.trim_start_matches(' ').len()));
            let art = art.lines().map(|l| format!("{pad}{l}")).collect::<Vec<_>>().join("\n");
            let tail = if name == "think" { 'o' } else { '\\' };
            Value::str(format!("{}\n   {tail}\n    {tail}\n{}", bubble(s, name == "think"), art))
        }
        ("beside", [Str(a), Str(b), rest @ ..]) if rest.len() <= 1 => {
            let gap = if rest.is_empty() { 1 } else { int(2, 0, 200)? };
            let (a, b) = (grid(a), grid(b));
            let wa = a.first().map_or(0, Vec::len);
            let rows = (0..a.len().max(b.len())).map(|r| {
                let mut row = a.get(r).cloned().unwrap_or_else(|| vec![' '; wa]);
                row.extend(std::iter::repeat_n(' ', gap));
                row.extend(b.get(r).into_iter().flatten());
                row
            });
            Value::str(ungrid(&rows.collect::<Vec<_>>()))
        }
        ("stack", [Str(a), Str(b), rest @ ..]) if rest.len() <= 1 => {
            let align = match rest {
                [] => "left",
                [Str(s)] if ["left", "center", "right"].contains(&&**s) => s,
                _ => return Err(Fail::Arg(2, "align is \"left\", \"center\" or \"right\"".into())),
            };
            let lines: Vec<&str> = a.lines().chain(b.lines()).collect();
            let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let placed = lines.iter().map(|l| match align {
                "left" => l.to_string(),
                "center" => format!("{}{l}", " ".repeat((w - l.chars().count()) / 2)),
                _ => format!("{l:>w$}"),
            });
            Value::str(placed.map(|l| l.trim_end().to_string()).collect::<Vec<_>>().join("\n"))
        }
        ("overlay", [Str(a), Str(b), Int(..), Int(..)]) => {
            let (x, y) = (int(2, 0, 1000)?, int(3, 0, 1000)?);
            let (mut a, b) = (grid(a), grid(b));
            let w = a.first().map_or(0, Vec::len).max(x + b.first().map_or(0, Vec::len));
            a.resize(a.len().max(y + b.len()), vec![]);
            for row in &mut a {
                row.resize(w, ' ');
            }
            for (r, row) in b.iter().enumerate() {
                for (c, ch) in row.iter().enumerate().filter(|(_, ch)| **ch != ' ') {
                    a[y + r][x + c] = *ch;
                }
            }
            Value::str(ungrid(&a))
        }
        ("mirror", [Str(s), rest @ ..]) if rest.len() <= 1 => {
            let mut g = grid(s);
            match rest {
                [] => horizontal(&mut g),
                [Str(a)] if a.as_ref() == "h" => horizontal(&mut g),
                [Str(a)] if a.as_ref() == "v" => vertical(&mut g),
                _ => return Err(Fail::Arg(1, "axis is \"h\" or \"v\"".into())),
            }
            Value::str(ungrid(&g))
        }
        ("rotate", [Str(s), Int(d, _)]) => {
            let mut g = grid(s);
            match d.rem_euclid(360) {
                0 => {}
                180 => {
                    horizontal(&mut g);
                    vertical(&mut g);
                }
                q @ (90 | 270) => {
                    let (h, w) = (g.len(), g.first().map_or(0, Vec::len));
                    g = (0..w).map(|c| (0..h).map(|r| swap(if q == 90 { g[h - 1 - r][c] } else { g[r][w - 1 - c] }, SWAP_TURN)).collect()).collect();
                }
                _ => return Err(Fail::Arg(1, "degrees must be a multiple of 90".into())),
            }
            Value::str(ungrid(&g))
        }
        ("scale", [Str(s), Int(..)]) => {
            let n = int(1, 1, 10)?;
            let g: Vec<Vec<char>> =
                grid(s).iter().flat_map(|row| std::iter::repeat_n(row.iter().flat_map(|c| std::iter::repeat_n(*c, n)).collect(), n)).collect();
            Value::str(ungrid(&g))
        }
        ("negative", [Str(s)]) => {
            let g = grid(s);
            let braille = g.iter().flatten().any(|c| ('\u{2800}'..='\u{28ff}').contains(c));
            let neg = |c: char| match c {
                ' ' if braille => '⣿',
                ' ' => '█',
                '█' => ' ',
                '\u{2800}'..='\u{28ff}' => char::from_u32(0x28ff - (c as u32 - 0x2800)).unwrap(),
                _ if NEGATIVE.iter().any(|(a, b)| *a == c || *b == c) => swap(c, NEGATIVE),
                _ => ' ',
            };
            // Not ungrid: trailing ink is the point here.
            Value::str(g.iter().map(|r| r.iter().map(|c| neg(*c)).collect::<String>()).collect::<Vec<_>>().join("\n"))
        }
        ("trim_art", [Str(s)]) => {
            let lines: Vec<&str> = s.lines().map(str::trim_end).skip_while(|l| l.is_empty()).collect();
            let end = lines.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
            let indent = lines[..end].iter().filter(|l| !l.is_empty()).map(|l| l.len() - l.trim_start_matches(' ').len()).min().unwrap_or(0);
            Value::str(lines[..end].iter().map(|l| l.get(indent..).unwrap_or("")).collect::<Vec<_>>().join("\n"))
        }
        _ => return Err(Fail::BadArgs),
    })
}

fn boxed(s: &str, style: &str) -> Result<String, String> {
    let shadow = style == "shadow";
    let [tl, h, tr, v, bl, br] = match style {
        "single" | "shadow" => ['┌', '─', '┐', '│', '└', '┘'],
        "double" => ['╔', '═', '╗', '║', '╚', '╝'],
        "round" => ['╭', '─', '╮', '│', '╰', '╯'],
        "heavy" => ['┏', '━', '┓', '┃', '┗', '┛'],
        "ascii" => ['+', '-', '+', '|', '+', '+'],
        "stars" => ['*'; 6],
        _ => return Err(format!("unknown style {style:?}; try single, double, round, heavy, ascii, stars or shadow")),
    };
    let g = grid(s);
    let w = g.first().map_or(0, Vec::len);
    let bar = h.to_string().repeat(w + 2);
    let mut out = vec![format!("{tl}{bar}{tr}")];
    out.extend(g.iter().map(|row| format!("{v} {} {v}", row.iter().collect::<String>())));
    out.push(format!("{bl}{bar}{br}"));
    if shadow {
        for line in out.iter_mut().skip(1) {
            line.push('▒');
        }
        out.push(format!(" {}", "▒".repeat(w + 4)));
    }
    Ok(out.join("\n"))
}

/// Text wrapped to 40 columns inside a speech (or thought) bubble.
fn bubble(s: &str, think: bool) -> String {
    let mut lines: Vec<String> = vec![];
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > 40 {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line += word;
        }
        lines.push(line);
    }
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let n = lines.len();
    let mut out = vec![format!(" {}", "_".repeat(w + 2))];
    for (i, l) in lines.iter().enumerate() {
        let (a, b) = match (think, n, i) {
            (true, _, _) => ('(', ')'),
            (_, 1, _) => ('<', '>'),
            (_, _, 0) => ('/', '\\'),
            _ if i == n - 1 => ('\\', '/'),
            _ => ('|', '|'),
        };
        out.push(format!("{a} {l:w$} {b}"));
    }
    out.push(format!(" {}", "-".repeat(w + 2)));
    out.join("\n")
}

fn horizontal(g: &mut [Vec<char>]) {
    for row in g {
        row.reverse();
        for c in row {
            *c = swap(*c, SWAP_H);
        }
    }
}

fn vertical(g: &mut [Vec<char>]) {
    g.reverse();
    for c in g.iter_mut().flatten() {
        *c = swap(*c, SWAP_V);
    }
}

fn swap(c: char, pairs: &[(char, char)]) -> char {
    pairs
        .iter()
        .find_map(|(a, b)| {
            if *a == c {
                Some(*b)
            } else if *b == c {
                Some(*a)
            } else {
                None
            }
        })
        .unwrap_or(c)
}

#[rustfmt::skip]
const SWAP_H: &[(char, char)] = &[
    ('(', ')'), ('[', ']'), ('{', '}'), ('<', '>'), ('/', '\\'), ('┌', '┐'), ('└', '┘'), ('╭', '╮'), ('╰', '╯'), ('╔', '╗'), ('╚', '╝'),
    ('┏', '┓'), ('┗', '┛'), ('├', '┤'), ('▌', '▐'), ('◀', '▶'), ('«', '»'),
];
#[rustfmt::skip]
const SWAP_V: &[(char, char)] = &[
    ('/', '\\'), ('┌', '└'), ('┐', '┘'), ('╭', '╰'), ('╮', '╯'), ('╔', '╚'), ('╗', '╝'), ('┏', '┗'), ('┓', '┛'), ('┬', '┴'), ('▀', '▄'),
    ('▲', '▼'), ('^', 'v'), ('_', '‾'),
];
/// Pairs that trade places under a quarter turn: strokes change direction, corners move round.
#[rustfmt::skip]
const SWAP_TURN: &[(char, char)] = &[('-', '|'), ('─', '│'), ('═', '║'), ('━', '┃'), ('/', '\\'), ('▀', '▐'), ('▄', '▌')];
const NEGATIVE: &[(char, char)] = &[('▀', '▄'), ('▌', '▐'), ('░', '▓'), ('▒', '▒')];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn frames() {
        assert_eq!(show(r#"boxed("ab\nc")"#), "┌────┐\n│ ab │\n│ c  │\n└────┘");
        assert_eq!(show(r#"boxed("x", "shadow")"#), "┌───┐\n│ x │▒\n└───┘▒\n ▒▒▒▒▒");
        assert!(try_eval(r#"boxed("x", "wavy")"#).is_err());
        assert!(show(r#"cowsay("moo")"#).starts_with(" _____\n< moo >\n -----\n   \\"));
        assert!(show(r#"think("a b", "cat")"#).contains("( a b )\n -----\n   o"));
        assert!(show(r#"say("hoo", "Owl")"#).contains("    \\\n    ,_,\n   (O,O)"));
        assert!(show(r#"say("one two three four five six seven eight nine ten")"#).contains("/ one two"));
        assert_eq!(show(r#""a\nbb".beside("c\nd\ne", 2)"#), "a   c\nbb  d\n    e");
        assert_eq!(show(r#""abc".stack("x", "center")"#), "abc\n x");
        assert_eq!(show(r#""....\n....".overlay("x y", 1, 1)"#), "....\n.x.y");
        assert_eq!(show(r#""(ab/".mirror"#), "\\ba)");
        assert_eq!(show(r#""/_\n|x".mirror("v")"#), "|x\n\\‾");
        assert_eq!(show(r#""ab\ncd".rotate(90)"#), "ca\ndb");
        assert_eq!(show(r#""ab\ncd".rotate(270)"#), "bd\nac");
        assert_eq!(show(r#""ab\ncd".rotate(180)"#), "dc\nba");
        assert_eq!(show(r#""a-\nb|".rotate(90)"#), "ba\n-|");
        // Four quarter turns come back home.
        assert_eq!(show(r#"clipart("cat").rotate(90).rotate(90).rotate(90).rotate(90)"#), show(r#"clipart("cat")"#));
        assert!(try_eval(r#""a".rotate(45)"#).is_err());
        assert_eq!(show(r#""ab".scale(2)"#), "aabb\naabb");
        assert_eq!(show(r#""█ ▀".negative"#), " █▄");
        assert_eq!(show(r#""⠁".negative"#), "⣾");
        assert_eq!(show(r#""\n  a\n   b \n\n".trim_art"#), "a\n b");
    }
}
