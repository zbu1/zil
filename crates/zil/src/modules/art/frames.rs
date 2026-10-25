//! Frames and composition: boxes, speech bubbles, and art placed beside, above, over, flipped, turned or scaled.

use super::grid;
use crate::modules::Module;

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
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{NEGATIVE, SWAP_TURN, boxed as frame, bubble, horizontal, swap, vertical};
    use crate::modules::Fail;
    use crate::modules::art::{grid, piece, ungrid};

    /// `n` if it's from `lo` to `hi`, else an error blaming argument `i`.
    fn int(i: usize, n: i64, lo: i64, hi: i64) -> Result<usize, Fail> {
        if (lo..=hi).contains(&n) { Ok(n as usize) } else { Err(Fail::Arg(i, format!("expected an int from {lo} to {hi}"))) }
    }

    /// draw a box around text
    ///
    /// style is "single", "double", "round", "heavy", "ascii", "stars" or "shadow"
    #[builtin(examples = [r#"boxed("hello")"#, r#""zil\ncalc".boxed("round")"#], see = ["say", "stack"])]
    fn boxed(s: &str, #[default("single")] style: &str) -> Result<String, Fail> {
        frame(s, style).map_err(|e| Fail::Arg(1, e))
    }

    /// A clip art picture saying `s`, in a thought bubble if `think`.
    fn speak(s: &str, art: &str, think: bool) -> Result<String, Fail> {
        let pic = piece(art).ok_or_else(|| Fail::Arg(1, format!("no {art:?} in the gallery; clipart() lists them")))?;
        // Line the first row up under the bubble's tail.
        let pad = " ".repeat(4usize.saturating_sub(pic.len() - pic.trim_start_matches(' ').len()));
        let pic = pic.lines().map(|l| format!("{pad}{l}")).collect::<Vec<_>>().join("\n");
        let tail = if think { 'o' } else { '\\' };
        Ok(format!("{}\n   {tail}\n    {tail}\n{}", bubble(s, think), pic))
    }

    /// a clip art picture saying s in a speech bubble: the cow, or any clipart() name
    #[builtin(examples = [r#"say("moo")"#, r#"say("pika", "pikachu")"#], see = ["think", "cowsay", "boxed"])]
    fn say(s: &str, #[default("cow")] art: &str) -> Result<String, Fail> {
        speak(s, art, false)
    }

    /// the same as say, named after the classic Unix program
    #[builtin(examples = [r#"cowsay("moo")"#], see = ["say"])]
    fn cowsay(s: &str, #[default("cow")] art: &str) -> Result<String, Fail> {
        speak(s, art, false)
    }

    /// like say, but in a thought bubble
    #[builtin(examples = [r#"think("hmm", "ghost")"#], see = ["say"])]
    fn think(s: &str, #[default("cow")] art: &str) -> Result<String, Fail> {
        speak(s, art, true)
    }

    /// put picture b to the right of picture a, tops lined up, with gap spaces between
    #[builtin(examples = [r#""a\nb".beside("c\nd\ne")"#], see = ["stack", "overlay"])]
    fn beside(a: &str, b: &str, #[default(1)] gap: i64) -> Result<String, Fail> {
        let gap = int(2, gap, 0, 200)?;
        let (a, b) = (grid(a), grid(b));
        let wa = a.first().map_or(0, Vec::len);
        let rows = (0..a.len().max(b.len())).map(|r| {
            let mut row = a.get(r).cloned().unwrap_or_else(|| vec![' '; wa]);
            row.extend(std::iter::repeat_n(' ', gap));
            row.extend(b.get(r).into_iter().flatten());
            row
        });
        Ok(ungrid(&rows.collect::<Vec<_>>()))
    }

    /// put picture b under picture a, lined up "left", "center" or "right"
    #[builtin(examples = [r#""wide one".stack("x", "right")"#], see = ["beside"])]
    fn stack(a: &str, b: &str, #[default("left")] align: &str) -> Result<String, Fail> {
        if !["left", "center", "right"].contains(&align) {
            return Err(Fail::Arg(2, "align is \"left\", \"center\" or \"right\"".into()));
        }
        let lines: Vec<&str> = a.lines().chain(b.lines()).collect();
        let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let placed = lines.iter().map(|l| match align {
            "left" => l.to_string(),
            "center" => format!("{}{l}", " ".repeat((w - l.chars().count()) / 2)),
            _ => format!("{l:>w$}"),
        });
        Ok(placed.map(|l| l.trim_end().to_string()).collect::<Vec<_>>().join("\n"))
    }

    /// draw picture b on top of picture a, with b's top-left corner at column x and row y of a (counting from 0)
    ///
    /// Spaces in b let a show through
    #[builtin(examples = [r#"boxed("     ").overlay("hi", 2, 1)"#], see = ["beside"])]
    fn overlay(a: &str, b: &str, x: i64, y: i64) -> Result<String, Fail> {
        let (x, y) = (int(2, x, 0, 1000)?, int(3, y, 0, 1000)?);
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
        Ok(ungrid(&a))
    }

    /// flip a picture left to right ("h") or upside down ("v")
    ///
    /// Slashes and brackets are swapped so the shapes still look right
    #[builtin(examples = [r#""(=^.^=)/".mirror"#, r#""/\\\n\\/".mirror("v")"#], see = ["rotate", "flip"])]
    fn mirror(s: &str, #[default("h")] axis: &str) -> Result<String, Fail> {
        let mut g = grid(s);
        match axis {
            "h" => horizontal(&mut g),
            "v" => vertical(&mut g),
            _ => return Err(Fail::Arg(1, "axis is \"h\" or \"v\"".into())),
        }
        Ok(ungrid(&g))
    }

    /// turn a picture clockwise by 90, 180 or 270 degrees
    #[builtin(examples = [r#""abc\ndef".rotate(90)"#], see = ["mirror"])]
    fn rotate(s: &str, degrees: i64) -> Result<String, Fail> {
        let mut g = grid(s);
        match degrees.rem_euclid(360) {
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
        Ok(ungrid(&g))
    }

    /// make a picture n times bigger by repeating each character n times across and n times down
    #[builtin(examples = [r#""ab".scale(2)"#], see = ["banner"])]
    fn scale(s: &str, n: i64) -> Result<String, Fail> {
        let n = int(1, n, 1, 10)?;
        let g: Vec<Vec<char>> = grid(s).iter().flat_map(|row| std::iter::repeat_n(row.iter().flat_map(|c| std::iter::repeat_n(*c, n)).collect(), n)).collect();
        Ok(ungrid(&g))
    }

    /// swap ink and background: spaces become solid blocks and the other way round, and braille dots are inverted
    #[builtin(examples = [r#""▀ █".negative"#], see = ["invert"])]
    fn negative(s: &str) -> String {
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
        g.iter().map(|r| r.iter().map(|c| neg(*c)).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    /// remove blank lines at the top and bottom, indentation every line shares, and spaces at the ends of lines
    #[builtin(examples = [r#""\n   ab\n    c\n\n".trim_art"#], see = ["dedent"])]
    fn trim_art(s: &str) -> String {
        let lines: Vec<&str> = s.lines().map(str::trim_end).skip_while(|l| l.is_empty()).collect();
        let end = lines.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
        let indent = lines[..end].iter().filter(|l| !l.is_empty()).map(|l| l.len() - l.trim_start_matches(' ').len()).min().unwrap_or(0);
        lines[..end].iter().map(|l| l.get(indent..).unwrap_or("")).collect::<Vec<_>>().join("\n")
    }
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
