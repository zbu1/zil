//! Text art: a clip-art gallery, color for any of it, QR codes, seven-segment digits, images and animation;
//! frames, generative art, game boards and FIGlet fonts below. Art is just a multi-line string, so every function composes.

mod animate;
pub mod figlet;
pub mod frames;
pub mod games;
pub mod generative;
mod img;

use crate::ansi::{RESET, strip_ansi};
use crate::modules::Module;
use crate::modules::dev::colors::Rgb;

pub const MODULE: Module = Module {
    name: "art",
    about: "pictures made of text: clip art, colored text, QR codes, images, and animation",
    #[rustfmt::skip]
    examples: &[
        ("gallery", &[
            ("the names of every piece in the gallery", "clipart()"),
            ("a cat in a box", r#"clipart("cat").boxed("round")"#),
            ("a rainbow banner", r#"banner("zil", "slant").rainbow"#),
            ("color codes don't count once stripped", r#"rainbow("hi").strip_ansi.len"#),
            ("a QR code to scan with your phone", r#"qr("https://github.com/zbu1/zil")"#),
            ("a big clock", r#"segments(now.format("%H:%M"))"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    children: &[frames::MODULE, generative::MODULE, games::MODULE, figlet::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::animate::animate as play;
    use super::img::img as picture;
    use super::{GALLERY, SEGMENTS, paint, piece};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::modules::dev::colors::{self, Color};
    use crate::signature::{Func, List, Or};
    use crate::value::Value;

    /// a piece of clip art from the built-in gallery by name, or with no name, the list of names
    #[builtin(examples = [r#"clipart("owl")"#, "clipart().len"], see = ["say", "boxed"])]
    fn clipart(name: Option<&str>) -> Result<Or<&'static str, Vec<Value>>, Fail> {
        match name {
            None => Ok(Or::B(GALLERY.iter().map(|g| Value::str(g.0)).collect())),
            Some(n) => Ok(Or::A(piece(n).ok_or_else(|| Fail::Arg(0, format!("no {n:?} in the gallery; clipart() lists them")))?)),
        }
    }

    /// color text with a rainbow running diagonally across it
    #[builtin(examples = [r#""rainbow".rainbow"#], see = ["gradient", "strip_ansi"])]
    fn rainbow(s: &str) -> String {
        paint(s, |col, row, _| colors::from_hsl((col * 12 + row * 6) as f64, 1.0, 0.6))
    }

    /// color text with a fade from one color on the left to another on the right
    #[builtin(examples = [r#"gradient("sunset", "gold", "crimson")"#], see = ["rainbow", "color"])]
    fn gradient(s: &str, from: Color, to: Color) -> Result<String, Fail> {
        let (a, b) = (from.rgb().map_err(|e| at(1, e))?, to.rgb().map_err(|e| at(2, e))?);
        Ok(paint(s, |col, _, w| colors::mix(a, b, if w > 1 { col as f64 / (w - 1) as f64 } else { 0.0 })))
    }

    /// A bad color, blamed on argument `i`.
    fn at(i: usize, e: Fail) -> Fail {
        match e {
            Fail::Msg(m) => Fail::Arg(i, m),
            e => e,
        }
    }

    /// a QR code for the text that phones can scan off the screen
    ///
    /// ec is how much damage it can survive and still scan: "L" 7%, "M" 15%, "Q" 25% or "H" 30%. Higher makes a bigger code
    #[builtin(examples = [r#"qr("zil")"#], see = ["url_build"])]
    fn qr(text: &str, #[default("M")] ec: &str) -> Result<String, Fail> {
        use qrcode::{EcLevel, QrCode, render::unicode::Dense1x2};
        let ec = match ec.to_uppercase().as_str() {
            "L" => EcLevel::L,
            "M" => EcLevel::M,
            "Q" => EcLevel::Q,
            "H" => EcLevel::H,
            _ => return Err(Fail::Arg(1, "ec is \"L\", \"M\", \"Q\" or \"H\"".into())),
        };
        let code = QrCode::with_error_correction_level(text.as_bytes(), ec).map_err(|e| Fail::Arg(0, format!("can't encode: {e}")))?;
        // Swapped colors: dark modules print as background, so the code reads right on a dark terminal.
        Ok(code.render::<Dense1x2>().dark_color(Dense1x2::Light).light_color(Dense1x2::Dark).build())
    }

    /// big digits like a digital clock's, for 0-9, A-F, -, ., : and spaces
    #[builtin(examples = [r#"segments("12:45")"#, r#"segments("C0FFEE")"#], see = ["banner", "clock"])]
    fn segments(s: Or<&str, i64>) -> Result<String, Fail> {
        let s = match s {
            Or::A(s) => s.to_string(),
            Or::B(n) => n.to_string(),
        };
        let mut rows = [String::new(), String::new(), String::new()];
        for c in s.chars() {
            let glyph: [String; 3] = match c {
                ':' => [" ".into(), "•".into(), "•".into()],
                '.' => [" ".into(), " ".into(), "▄".into()],
                _ => {
                    let m = SEGMENTS.iter().find(|s| s.0 == c.to_ascii_uppercase()).ok_or_else(|| Fail::Arg(0, format!("no segment digit for {c:?}")))?.1;
                    let on = |bit: u8, s: &'static str, off: &'static str| if m & bit != 0 { s } else { off };
                    // Bits: a top, b top right, c bottom right, d bottom, e bottom left, f top left, g middle.
                    [
                        format!(" {} ", on(1, "▄▄", "  ")),
                        format!("{}{}{}", on(32, "█", " "), on(64, "▄▄", "  "), on(2, "█", " ")),
                        format!("{}{}{}", on(16, "█", " "), on(8, "▄▄", "  "), on(4, "█", " ")),
                    ]
                }
            };
            for (row, g) in rows.iter_mut().zip(glyph) {
                *row += &g;
                row.push(' ');
            }
        }
        Ok(rows.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n"))
    }

    /// draw an image file width characters wide (the terminal's width without one, or 80)
    ///
    /// mode is "blocks", two pixels per character in full color, "ascii", brightness drawn with the characters .:-=+*#%@ so it pastes anywhere, or "braille", black-and-white dots, 8 per character, the sharpest of the three
    #[builtin(shown = [r#"img("cat.png")"#, r#"img("logo.jpg", 40, "ascii")"#, r#"img("photo.png", 60, "braille").negative"#], see = ["negative", "boxed"])]
    fn img(path: &str, width: Option<i64>, #[default("blocks")] mode: &str) -> Result<String, Fail> {
        picture(path, width, mode)
    }

    /// play an animation in the terminal at fps frames a second
    ///
    /// A list of pictures plays count times (once without one), and a function is called with each frame number from 0 and plays count frames (60 without one). When the output isn't a terminal, only the last frame is printed
    #[builtin(shown = [r#"animate(|i| life("glider", i), 8, 40)"#, r#"animate(["|", "/", "-", "\\"], 12, 5)"#], see = ["life", "fractal"])]
    fn animate(it: &mut Interp, frames: Or<&List, Func>, #[default(10.0)] fps: f64, count: Option<i64>, span: &Span) -> Result<(), Fail> {
        let frames = match frames {
            Or::A(l) => Value::List(l.clone()),
            Or::B(f) => f.0,
        };
        play(it, &frames, fps, count, span)
    }
}

/// Color every visible character by its column, row and the art's width; whitespace stays plain.
fn paint(s: &str, rgb: impl Fn(usize, usize, usize) -> Rgb) -> String {
    let s = strip_ansi(s);
    let w = s.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    let lines = s.lines().enumerate().map(|(row, line)| {
        let mut out = String::new();
        for (col, c) in line.chars().enumerate() {
            if c.is_whitespace() {
                out.push(c);
            } else {
                out += &format!("{}{c}", fg(rgb(col, row, w)));
            }
        }
        if out.len() > line.len() { out + RESET } else { out }
    });
    lines.collect::<Vec<_>>().join("\n")
}

/// The truecolor escape for a foreground color.
pub fn fg(c: Rgb) -> String {
    let [r, g, b] = c.map(|x| x.round().clamp(0.0, 255.0) as u8);
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// Art as a rectangle of chars, short lines padded with spaces.
// ponytail: one char = one column; wide chars (emoji, CJK) and color codes skew alignment, add unicode-width if that bites.
pub fn grid(s: &str) -> Vec<Vec<char>> {
    let mut g: Vec<Vec<char>> = s.lines().map(|l| l.chars().collect()).collect();
    let w = g.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut g {
        row.resize(w, ' ');
    }
    g
}

/// A grid back to text, trailing spaces dropped.
pub fn ungrid(g: &[Vec<char>]) -> String {
    g.iter().map(|row| row.iter().collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
}

/// A braille canvas: each character cell holds 2×4 dots, which come out about square in a terminal.
pub struct Dots {
    pub w: usize,
    pub h: usize,
    cells: Vec<u8>,
}

impl Dots {
    pub fn new(w: usize, h: usize) -> Dots {
        Dots { w, h, cells: vec![0; w.div_ceil(2) * h.div_ceil(4)] }
    }

    pub fn set(&mut self, x: i64, y: i64) {
        if (0..self.w as i64).contains(&x) && (0..self.h as i64).contains(&y) {
            const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
            let (x, y) = (x as usize, y as usize);
            self.cells[y / 4 * self.w.div_ceil(2) + x / 2] |= BIT[x % 2][y % 4];
        }
    }

    pub fn line(&mut self, (x0, y0): (f64, f64), (x1, y1): (f64, f64)) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            self.set((x0 + (x1 - x0) * t).round() as i64, (y0 + (y1 - y0) * t).round() as i64);
        }
    }

    /// Empty cells come out as spaces so lines trim cleanly.
    pub fn render(&self) -> String {
        let rows = self.cells.chunks(self.w.div_ceil(2).max(1));
        let lines =
            rows.map(|r| r.iter().map(|b| if *b == 0 { ' ' } else { char::from_u32(0x2800 + *b as u32).unwrap() }).collect::<String>().trim_end().to_string());
        lines.collect::<Vec<_>>().join("\n")
    }
}

/// A gallery piece by name, ignoring case.
pub fn piece(name: &str) -> Option<&'static str> {
    GALLERY.iter().find(|g| g.0 == name.trim().to_lowercase()).map(|g| g.1.trim_start_matches('\n'))
}

/// Original pieces and old folk art from Usenet days.
const GALLERY: &[(&str, &str)] = &[
    (
        "cat",
        r"
 /\_/\
( o.o )
 > ^ <",
    ),
    (
        "dog",
        r"
  __      _
o'')}____//
 `_/      )
 (_(_/-(_/",
    ),
    (
        "owl",
        r#"
 ,_,
(O,O)
(   )
-"-"-"#,
    ),
    (
        "bunny",
        r"
(\_/)
(•ᴗ•)
/ > ♥",
    ),
    (
        "cow",
        r"
         (__)
         (oo)
  /-------\/
 / |     ||
*  ||----||
   ~~    ~~",
    ),
    (
        "pikachu",
        r#"
 (\__/)
 (o^.^o)
z(_(")(")"#,
    ),
    (
        "fish",
        r"
   ><(((º>",
    ),
    (
        "duck",
        r"
   __
 <(o )___
  ( ._> /
   `---'",
    ),
    (
        "bat",
        r#"
 /\                 /\
/ \'._   (\_/)   _.'/ \
|.''._'--(o.o)--'_.''.|
 \_ / `;=/ " \=;` \ _/
   `\__| \___/ |__/`
        \(_|_)/
         " ` ""#,
    ),
    (
        "snail",
        r#"
    .----.   @   @
   / .-"-.`.  \v/
   | | '\ \ \_/ )
 ,-\ `-.' /.'  /
'---`----'----'"#,
    ),
    (
        "crab",
        r"
   _~^~^~_
\) /  o o  \ (/
  '_   -   _'
  / '-----' \",
    ),
    (
        "ghost",
        r"
  .-.
 (o o)
 | O \
  \   \
   `~~~'",
    ),
    (
        "skull",
        r"
  _____
 /     \
| () () |
 \  ^  /
  |||||
  |||||",
    ),
    (
        "coffee",
        r"
   ( (
    ) )
  ........
  |      |]
  \      /
   `----'",
    ),
    (
        "rocket",
        r"
    ^
   / \
  /___\
  |   |
  | z |
  |   |
 /|   |\
/_|___|_\
   /_\",
    ),
    (
        "house",
        r"
    /\
   /  \
  /____\
  | [] |
  |_||_|",
    ),
    (
        "tree",
        r"
    *
   /o\
  /o  \
 /  o  \
/_o___o_\
   |_|",
    ),
    (
        "cactus",
        r"
     _
    | |
 _  | |  _
| | | | | |
| |_| |_| |
 \___   _/
     | |
     | |",
    ),
    (
        "heart",
        r"
 ,d88b.d88b,
 88888888888
 `Y8888888Y'
   `Y888Y'
     `Y'",
    ),
];

#[rustfmt::skip]
const SEGMENTS: &[(char, u8)] = &[
    ('0', 63), ('1', 6), ('2', 91), ('3', 79), ('4', 102), ('5', 109), ('6', 125), ('7', 7), ('8', 127), ('9', 111),
    ('A', 119), ('B', 124), ('C', 57), ('D', 94), ('E', 121), ('F', 113), ('-', 64), (' ', 0),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn gallery() {
        assert_eq!(show(r#"clipart("CAT")"#), " /\\_/\\\n( o.o )\n > ^ <");
        assert!(try_eval(r#"clipart("unicorn")"#).is_err());
        assert_eq!(show(r#"rainbow("a b").strip_ansi"#), "a b");
        assert_eq!(show(r#"gradient("ab", "red", "blue")"#), "\x1b[38;2;255;0;0ma\x1b[38;2;0;0;255mb\x1b[0m");
        assert_eq!(show(r#"rainbow(" ")"#), " ");
        let q = show(r#"qr("hi")"#);
        // Version 1 is 21 modules plus a 4-module quiet zone each side: 29 wide, ceil(29 / 2) lines.
        assert_eq!((q.lines().count(), q.lines().next().unwrap().chars().count()), (15, 29));
        assert!(try_eval(r#"qr("x", "Z")"#).is_err());
        assert_eq!(show(r#"segments("1")"#), "\n   █\n   █");
        assert_eq!(show(r#"segments("8.")"#), " ▄▄\n█▄▄█\n█▄▄█ ▄");
        assert!(try_eval(r#"segments("x")"#).is_err());
    }
}
