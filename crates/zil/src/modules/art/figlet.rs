//! Big letters: the built-in block font, and FIGlet `.flf` fonts with figlet's smushing rules.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;
use std::collections::HashMap;

pub const MODULE: Module = Module {
    name: "figlet",
    about: "big banner letters made of text characters, in many fonts",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Big banner letters made of ordinary characters, in the style of FIGlet, a classic program whose fonts are .flf files."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("banners", &[
            ("a header for a README", r#"banner("zil", "standard")"#),
            ("a sample of every font", r#"fonts().map(|f| banner("Hi", f)).join("\n")"#),
            ("a boxed title", r#"banner("v2", "small").boxed("round")"#),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("banner", "banner(s: str, font?: str) -> str", "write s in big letters", &[r#"banner("hi!")"#, r#"banner("zil", "slant")"#], &["fonts", "segments", "style"])
        .detail("font is \"block\" (the default, which has A-Z, 0-9 and some punctuation), a name from fonts(), or the path to a FIGlet .flf font file"),
    doc("fonts", "fonts() -> list", "the names of the fonts banner has built in", &["fonts()"], &["banner"]),
];

const FONTS: &[(&str, &str)] = &[
    ("standard", include_str!("fonts/standard.flf")),
    ("small", include_str!("fonts/small.flf")),
    ("slant", include_str!("fonts/slant.flf")),
    ("big", include_str!("fonts/big.flf")),
    ("mini", include_str!("fonts/mini.flf")),
    ("smslant", include_str!("fonts/smslant.flf")),
    ("shadow", include_str!("fonts/shadow.flf")),
    ("smshadow", include_str!("fonts/smshadow.flf")),
    ("script", include_str!("fonts/script.flf")),
    ("smscript", include_str!("fonts/smscript.flf")),
    ("lean", include_str!("fonts/lean.flf")),
    ("banner", include_str!("fonts/banner.flf")),
    ("bubble", include_str!("fonts/bubble.flf")),
    ("digital", include_str!("fonts/digital.flf")),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("fonts", []) => Value::list(std::iter::once("block").chain(FONTS.iter().map(|f| f.0)).map(Value::str).collect()),
        ("banner", [Str(s)]) => Value::str(block(s)?),
        ("banner", [Str(s), Str(f)]) if f.as_ref() == "block" => Value::str(block(s)?),
        ("banner", [Str(s), Str(f)]) => {
            let src = match FONTS.iter().find(|x| x.0 == f.as_ref()) {
                Some(x) => x.1.to_string(),
                None if f.ends_with(".flf") => {
                    let bytes = crate::host::get().read(f).map_err(|e| Fail::Arg(1, format!("can't read {f}: {}", crate::modules::fs::io_reason(&e))))?;
                    // Older fonts are Latin-1, whose bytes are exactly the first 256 code points.
                    String::from_utf8(bytes).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect())
                }
                None => return Err(Fail::Arg(1, format!("unknown font {f:?}; fonts() lists them, or pass a .flf path"))),
            };
            let font = Font::parse(&src).map_err(|e| Fail::Arg(1, format!("bad font: {e}")))?;
            Value::str(font.render(s).map_err(|e| Fail::Arg(0, e))?)
        }
        _ => return Err(Fail::BadArgs),
    })
}

fn block(s: &str) -> Result<String, Fail> {
    // 5 rows of 4 two-column cells per letter.
    crate::state::check_len(s.chars().count().saturating_mul(40), "characters").map_err(Fail::Msg)?;
    let mut rows = vec![String::new(); 5];
    for c in s.to_uppercase().chars() {
        let glyph = BLOCK.iter().find(|g| g.0 == c).ok_or_else(|| Fail::Arg(0, format!("the block font has no letter for {c:?}")))?.1;
        for (r, row) in rows.iter_mut().enumerate() {
            for bit in (0..3).rev() {
                row.push_str(if glyph[r] >> bit & 1 == 1 { "██" } else { "  " });
            }
            row.push_str("  ");
        }
    }
    Ok(rows.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n"))
}

// Layout bits, as in figlet's full_layout header field.
const EQUAL: u32 = 1;
const LOWLINE: u32 = 2;
const HIERARCHY: u32 = 4;
const PAIR: u32 = 8;
const BIGX: u32 = 16;
const HARDBLANK: u32 = 32;
const KERN: u32 = 64;
const SMUSH: u32 = 128;

/// A FIGlet font. ponytail: horizontal layout only; right-to-left fonts and vertical smushing are ignored.
struct Font {
    hardblank: char,
    height: usize,
    mode: u32,
    glyphs: HashMap<char, Vec<Vec<char>>>,
}

impl Font {
    fn parse(src: &str) -> Result<Font, String> {
        let mut lines = src.lines().map(|l| l.trim_end_matches('\r'));
        let header = lines.next().filter(|h| h.starts_with("flf2a")).ok_or("not a FIGlet font: no flf2a header")?;
        let hardblank = header.chars().nth(5).ok_or("no hardblank in the header")?;
        let nums: Vec<i64> = header.chars().skip(6).collect::<String>().split_whitespace().map_while(|n| n.parse().ok()).collect();
        let [height, _baseline, _max, old, comments, ..] = nums[..] else { return Err("short header".into()) };
        let mode = match nums.get(6) {
            Some(&full) if full & 128 != 0 => (full as u32 & 63) | SMUSH,
            Some(&full) if full & 64 != 0 => (full as u32 & 63) | KERN,
            Some(_) => 0,
            None if old < 0 => 0,
            None if old == 0 => KERN,
            None => old as u32 & 63 | SMUSH,
        };
        let height = usize::try_from(height).ok().filter(|h| (1..=100).contains(h)).ok_or("bad height")?;
        let mut lines = lines.skip(comments.max(0) as usize);
        let glyph = |lines: &mut dyn Iterator<Item = &str>| -> Option<Vec<Vec<char>>> {
            let rows: Vec<Vec<char>> = lines
                .take(height)
                .map(|l| {
                    let end = l.chars().last().unwrap_or(' ');
                    l.trim_end_matches(end).chars().collect()
                })
                .collect();
            (rows.len() == height).then_some(rows)
        };
        let mut glyphs = HashMap::new();
        let required = (32u8..=126).map(char::from).chain(['Ä', 'Ö', 'Ü', 'ä', 'ö', 'ü', 'ß']);
        for c in required {
            let Some(g) = glyph(&mut lines) else { break };
            glyphs.insert(c, g);
        }
        // Code-tagged characters: a line starting with the code (decimal, 0x hex or 0 octal), then the glyph.
        while let Some(tag) = lines.next() {
            let code = tag.split_whitespace().next().unwrap_or("");
            let code = if let Some(h) = code.strip_prefix("0x").or(code.strip_prefix("0X")) {
                u32::from_str_radix(h, 16).ok()
            } else if code.len() > 1 && code.starts_with('0') {
                u32::from_str_radix(&code[1..], 8).ok()
            } else {
                code.parse().ok()
            };
            let Some(g) = glyph(&mut lines) else { break };
            if let Some(c) = code.and_then(char::from_u32) {
                glyphs.insert(c, g);
            }
        }
        if !glyphs.contains_key(&'A') {
            return Err("no glyphs".into());
        }
        Ok(Font { hardblank, height, mode, glyphs })
    }

    fn render(&self, text: &str) -> Result<String, String> {
        let widest = self.glyphs.values().map(|g| g[0].len()).max().unwrap_or(0);
        crate::state::check_len(text.chars().count().saturating_mul(self.height * widest), "characters")?;
        let mut out = vec![];
        for line in text.lines() {
            let mut rows: Vec<Vec<char>> = vec![vec![]; self.height];
            let mut prev_w = 0;
            for c in line.chars() {
                let g = self.glyphs.get(&c).ok_or_else(|| format!("this font has no glyph for {c:?}"))?;
                let w = g[0].len();
                let amt = self.overlap(&rows, g, prev_w, w);
                for (row, grow) in rows.iter_mut().zip(g) {
                    let len = row.len();
                    for (k, &r) in grow.iter().enumerate().take(amt) {
                        if let Some(col) = (len + k).checked_sub(amt) {
                            row[col] = self.smush(row[col], r, prev_w, w).unwrap_or(r);
                        }
                    }
                    row.extend(grow.iter().skip(amt));
                }
                prev_w = w;
            }
            out.extend(rows.iter().map(|r| r.iter().map(|&c| if c == self.hardblank { ' ' } else { c }).collect::<String>().trim_end().to_string()));
        }
        Ok(out.join("\n").trim_end_matches('\n').to_string())
    }

    /// How many columns the next glyph can slide into the line so far, per figlet's `smushamt`.
    fn overlap(&self, rows: &[Vec<char>], g: &[Vec<char>], prev_w: usize, w: usize) -> usize {
        if self.mode & (SMUSH | KERN) == 0 {
            return 0;
        }
        let mut most = w;
        for (line, grow) in rows.iter().zip(g) {
            let cb = grow.iter().position(|c| *c != ' ').unwrap_or(grow.len());
            let amt = match line.iter().rposition(|c| *c != ' ') {
                None => cb + line.len(),
                Some(lb) => {
                    let fits = grow.get(cb).is_some_and(|&r| self.smush(line[lb], r, prev_w, w).is_some());
                    cb + line.len() - 1 - lb + usize::from(fits)
                }
            };
            most = most.min(amt);
        }
        most
    }

    /// What two overlapping characters become, or None if they can't share a column.
    fn smush(&self, l: char, r: char, prev_w: usize, w: usize) -> Option<char> {
        let hb = self.hardblank;
        if l == ' ' {
            return Some(r);
        }
        if r == ' ' {
            return Some(l);
        }
        if prev_w < 2 || w < 2 || self.mode & SMUSH == 0 {
            return None;
        }
        if self.mode & 63 == 0 {
            // Universal smushing: the later character wins, except over a hardblank.
            return Some(if r == hb { l } else { r });
        }
        if self.mode & HARDBLANK != 0 && l == hb && r == hb {
            return Some(l);
        }
        if l == hb || r == hb {
            return None;
        }
        if self.mode & EQUAL != 0 && l == r {
            return Some(l);
        }
        const LOW: &str = "|/\\[]{}()<>";
        if self.mode & LOWLINE != 0 {
            if l == '_' && LOW.contains(r) {
                return Some(r);
            }
            if r == '_' && LOW.contains(l) {
                return Some(l);
            }
        }
        if self.mode & HIERARCHY != 0 {
            let class = |c: char| ["|", "/\\", "[]", "{}", "()", "<>"].iter().position(|k| k.contains(c));
            if let (Some(a), Some(b)) = (class(l), class(r))
                && a != b
            {
                return Some(if a > b { l } else { r });
            }
        }
        if self.mode & PAIR != 0 && ["[]", "][", "{}", "}{", "()", ")("].contains(&format!("{l}{r}").as_str()) {
            return Some('|');
        }
        if self.mode & BIGX != 0 {
            match (l, r) {
                ('/', '\\') => return Some('|'),
                ('\\', '/') => return Some('Y'),
                ('>', '<') => return Some('X'),
                _ => {}
            }
        }
        None
    }
}

/// 3×5 pixel glyphs, one row per entry, high bit on the left.
#[rustfmt::skip]
const BLOCK: &[(char, [u8; 5])] = &[
    ('A', [0b010, 0b101, 0b111, 0b101, 0b101]), ('B', [0b110, 0b101, 0b110, 0b101, 0b110]), ('C', [0b011, 0b100, 0b100, 0b100, 0b011]),
    ('D', [0b110, 0b101, 0b101, 0b101, 0b110]), ('E', [0b111, 0b100, 0b110, 0b100, 0b111]), ('F', [0b111, 0b100, 0b110, 0b100, 0b100]),
    ('G', [0b011, 0b100, 0b101, 0b101, 0b011]), ('H', [0b101, 0b101, 0b111, 0b101, 0b101]), ('I', [0b111, 0b010, 0b010, 0b010, 0b111]),
    ('J', [0b001, 0b001, 0b001, 0b101, 0b010]), ('K', [0b101, 0b101, 0b110, 0b101, 0b101]), ('L', [0b100, 0b100, 0b100, 0b100, 0b111]),
    ('M', [0b101, 0b111, 0b111, 0b101, 0b101]), ('N', [0b110, 0b101, 0b101, 0b101, 0b101]), ('O', [0b010, 0b101, 0b101, 0b101, 0b010]),
    ('P', [0b110, 0b101, 0b110, 0b100, 0b100]), ('Q', [0b010, 0b101, 0b101, 0b110, 0b011]), ('R', [0b110, 0b101, 0b110, 0b101, 0b101]),
    ('S', [0b011, 0b100, 0b010, 0b001, 0b110]), ('T', [0b111, 0b010, 0b010, 0b010, 0b010]), ('U', [0b101, 0b101, 0b101, 0b101, 0b111]),
    ('V', [0b101, 0b101, 0b101, 0b101, 0b010]), ('W', [0b101, 0b101, 0b111, 0b111, 0b101]), ('X', [0b101, 0b101, 0b010, 0b101, 0b101]),
    ('Y', [0b101, 0b101, 0b010, 0b010, 0b010]), ('Z', [0b111, 0b001, 0b010, 0b100, 0b111]),
    ('0', [0b111, 0b101, 0b101, 0b101, 0b111]), ('1', [0b010, 0b110, 0b010, 0b010, 0b111]), ('2', [0b110, 0b001, 0b010, 0b100, 0b111]),
    ('3', [0b110, 0b001, 0b010, 0b001, 0b110]), ('4', [0b101, 0b101, 0b111, 0b001, 0b001]), ('5', [0b111, 0b100, 0b110, 0b001, 0b110]),
    ('6', [0b011, 0b100, 0b110, 0b101, 0b010]), ('7', [0b111, 0b001, 0b010, 0b010, 0b010]), ('8', [0b010, 0b101, 0b010, 0b101, 0b010]),
    ('9', [0b010, 0b101, 0b011, 0b001, 0b110]),
    (' ', [0, 0, 0, 0, 0]), ('!', [0b010, 0b010, 0b010, 0b000, 0b010]), ('?', [0b110, 0b001, 0b010, 0b000, 0b010]),
    ('.', [0, 0, 0, 0, 0b010]), (',', [0, 0, 0, 0b010, 0b100]), ('-', [0, 0, 0b111, 0, 0]), (':', [0, 0b010, 0, 0b010, 0]),
    ('\'', [0b010, 0b010, 0, 0, 0]), ('+', [0, 0b010, 0b111, 0b010, 0]), ('/', [0b001, 0b001, 0b010, 0b100, 0b100]),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn figlet() {
        assert_eq!(show(r#"banner("i")"#), "██████\n  ██\n  ██\n  ██\n██████");
        assert!(try_eval(r#"banner("~")"#).is_err());
        // Matches `figlet -f standard Hi`, smushing and all.
        assert_eq!(show(r#"banner("Hi", "standard")"#), " _   _ _\n| | | (_)\n| |_| | |\n|  _  | |\n|_| |_|_|");
        assert_eq!(show(r#"banner("zil", "slant")"#), "        _ __\n ____  (_) /\n/_  / / / /\n / /_/ / /\n/___/_/_/");
        assert_eq!(show(r#"banner("a\nb", "small").lines.len"#), "9");
        assert_eq!(show(r#"banner("Hi", "bubble")"#), "  _   _\n / \\ / \\\n( H | i )\n \\_/ \\_/");
        for f in ["small", "big", "mini", "smslant", "shadow", "smshadow", "script", "smscript", "lean", "banner", "digital"] {
            assert!(show(&format!(r#"banner("Zil 2!", {f:?})"#)).lines().count() >= 3);
        }
        assert!(try_eval(r#"banner("x", "comic")"#).is_err());
        assert!(try_eval(r#"banner("x", "nope.flf")"#).is_err());
    }
}
