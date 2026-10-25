//! Colors as "#rrggbb" strings: parse, convert, mix, adjust, contrast, preview.

use crate::modules::{Fail, Module};
use crate::signature::Arg;
use crate::value::{Value, num};

pub const MODULE: Module = Module {
    name: "colors",
    about: "make, convert, blend and adjust colors, and check text is readable on a background",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Colors are strings in the hex form web pages use, `\"#rrggbb\"`: two hex digits each for how much red, green and blue \
there is, from 00 to ff.\n\n\
Every function here that takes a color also accepts a CSS name like \"tomato\", the short form \"#f80\", or a list \
of three numbers from 0 to 255."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("colors", &[
            ("named to hex", r#"color("rebeccapurple")"#),
            ("halfway between", r#"mix("red", "blue")"#),
            ("a darker shade for a hovered button", r##"darken("#3478f6", 10%)"##),
            ("is white text readable on gray?", r##"contrast("white", "#777")"##),
            ("five hues evenly around the wheel", r#"(0..5).map(|i| hsl(i * 72, 70, 50))"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("make", &["color", "rgb", "hsl"]),
        ("adjust", &["mix", "lighten", "darken", "grayscale"]),
        ("inspect", &["contrast", "swatch"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

/// A color argument: a name or hex code, or a list [r, g, b], read by `rgb`, which explains what's wrong with one.
pub struct Color(Value);

impl<'a> Arg<'a> for Color {
    const TYPE: &'static str = "str|list";
    fn of(v: &'a Value) -> Option<Self> {
        matches!(v, Value::Str(_) | Value::List(_)).then(|| Color(v.clone()))
    }
}

impl Color {
    pub fn rgb(&self) -> Result<Rgb, Fail> {
        let v = &self.0;
        parse(v).ok_or_else(|| Fail::from(format!("{v:?} is not a color\nhelp: use a name like \"tomato\", \"#f80\", \"#ff8800\" or [255, 136, 0]")))
    }
}

/// The opposite color on the color wheel; `data.maps` owns `invert` and forwards colors here.
pub fn invert(c: &Color) -> Result<String, Fail> {
    Ok(hex(c.rgb()?.map(|x| 255.0 - x)))
}

#[zil_macros::builtins]
mod fns {
    use super::{Color, from_hsl, hex, luminance, mix as blend, to_hsl};
    use crate::modules::Fail;
    use crate::signature::Num;
    use crate::value::Value;

    fn amount(t: f64) -> Result<f64, Fail> {
        if (0.0..=1.0).contains(&t) { Ok(t) } else { Err(Fail::from("amount must be from 0 to 1, like 0.2 or 20%")) }
    }

    /// a color in "#rrggbb" form, from a CSS name, "#rgb", "#rrggbb" or a list [r, g, b]
    #[builtin(examples = [r#"color("tomato")"#, r##"color("#f80")"##, "color([255, 0, 128])"], see = ["rgb", "hsl"])]
    fn color(c: Color) -> Result<String, Fail> {
        Ok(hex(c.rgb()?))
    }

    /// make a color from red, green and blue amounts (0-255), or give one argument to get a color's three amounts
    #[builtin(examples = ["rgb(255, 136, 0)", r#""orange".rgb"#], see = ["hsl", "color"])]
    fn rgb(r: Num, g: Num, b: Num) -> Result<String, Fail> {
        Ok(hex(Color(Value::list(vec![r.0, g.0, b.0])).rgb()?))
    }

    #[builtin(name = "rgb")]
    fn rgb_of(c: Color) -> Result<Vec<Value>, Fail> {
        Ok(c.rgb()?.iter().map(|x| Value::int(x.round() as i64)).collect())
    }

    /// make a color from hue, saturation and lightness, or give one argument to get a color's three values
    ///
    /// Hue is the position on the color wheel in degrees (0 red, 120 green, 240 blue), saturation how vivid it is and lightness how close to white, both from 0 to 100
    #[builtin(examples = ["hsl(210, 80, 50)", r#""teal".hsl"#], see = ["rgb", "color"])]
    fn hsl(h: f64, s: f64, l: f64) -> Result<String, Fail> {
        if !(0.0..=100.0).contains(&s) || !(0.0..=100.0).contains(&l) {
            return Err("expected hue in degrees, saturation and lightness 0-100".into());
        }
        Ok(hex(from_hsl(h, s / 100.0, l / 100.0)))
    }

    #[builtin(name = "hsl")]
    fn hsl_of(c: Color) -> Result<Vec<Value>, Fail> {
        let [h, s, l] = to_hsl(c.rgb()?);
        Ok(vec![Value::int(h.round() as i64), Value::int((s * 100.0).round() as i64), Value::int((l * 100.0).round() as i64)])
    }

    /// blend two colors, where t is how far to go from a toward b, 0 to 1
    #[builtin(examples = [r#"mix("red", "blue")"#, r#"mix("white", "black", 25%)"#], see = ["lighten", "darken"])]
    fn mix(a: Color, b: Color, #[default(0.5)] t: f64) -> Result<String, Fail> {
        Ok(hex(blend(a.rgb()?, b.rgb()?, amount(t)?)))
    }

    /// make a color lighter, keeping its hue
    ///
    /// amount, like 20%, is added to its HSL lightness
    #[builtin(examples = [r##"lighten("#3478f6", 20%)"##], see = ["darken", "mix"])]
    fn lighten(c: Color, amount: f64) -> Result<String, Fail> {
        let [h, s, l] = to_hsl(c.rgb()?);
        Ok(hex(from_hsl(h, s, (l + self::amount(amount)?).clamp(0.0, 1.0))))
    }

    /// make a color darker, keeping its hue
    ///
    /// amount, like 20%, is taken off its HSL lightness
    #[builtin(examples = [r##"darken("#3478f6", 20%)"##], see = ["lighten", "mix"])]
    fn darken(c: Color, amount: f64) -> Result<String, Fail> {
        let [h, s, l] = to_hsl(c.rgb()?);
        Ok(hex(from_hsl(h, s, (l - self::amount(amount)?).clamp(0.0, 1.0))))
    }

    /// the gray that looks as bright as the color does
    #[builtin(examples = [r#"grayscale("tomato")"#], see = ["invert"])]
    fn grayscale(c: Color) -> Result<String, Fail> {
        let y = luminance(c.rgb()?);
        // Back from linear light to an sRGB channel.
        let g = if y <= 0.0031308 { y * 12.92 } else { 1.055 * y.powf(1.0 / 2.4) - 0.055 };
        Ok(hex([g * 255.0; 3]))
    }

    /// how readable one color is on the other, as the WCAG contrast ratio from 1 (identical) to 21 (black on white)
    ///
    /// Body text needs 4.5 or more and large text 3
    #[builtin(examples = [r#"contrast("white", "black")"#, r##"contrast("#777", "white")"##], see = ["swatch"])]
    fn contrast(a: Color, b: Color) -> Result<f64, Fail> {
        let (x, y) = (luminance(a.rgb()?), luminance(b.rgb()?));
        Ok(((x.max(y) + 0.05) / (x.min(y) + 0.05) * 100.0).round() / 100.0)
    }

    /// a block of the color followed by its hex code, to preview it
    ///
    /// Needs a terminal with full (24-bit) color
    #[builtin(examples = [r#"swatch("tomato")"#, r#"["red", "gold", "teal"].map(swatch).join(" ")"#], see = ["color"])]
    fn swatch(c: Color) -> Result<String, Fail> {
        let [r, g, b] = c.rgb()?.map(|x| x.round() as u8);
        Ok(format!("\x1b[48;2;{r};{g};{b}m    \x1b[0m {}", hex([r as f64, g as f64, b as f64])))
    }
}

pub type Rgb = [f64; 3];

pub fn parse(v: &Value) -> Option<Rgb> {
    match v {
        Value::List(l) => match l.borrow()[..] {
            [ref r, ref g, ref b] => {
                let ch = |v: &Value| num(v).filter(|x| (0.0..=255.0).contains(x));
                Some([ch(r)?, ch(g)?, ch(b)?])
            }
            _ => None,
        },
        Value::Str(s) => {
            let s = s.trim().to_lowercase();
            let h = match NAMES.iter().find(|n| n.0 == s) {
                Some(n) => n.1,
                None => s.strip_prefix('#').unwrap_or(&s),
            };
            let digits: Vec<f64> = h.chars().map(|c| c.to_digit(16).map(f64::from)).collect::<Option<_>>()?;
            match digits[..] {
                [r, g, b] => Some([r * 17.0, g * 17.0, b * 17.0]),
                [r1, r2, g1, g2, b1, b2] => Some([r1 * 16.0 + r2, g1 * 16.0 + g2, b1 * 16.0 + b2]),
                _ => None,
            }
        }
        _ => None,
    }
}

fn hex(c: Rgb) -> String {
    let [r, g, b] = c.map(|x| x.round().clamp(0.0, 255.0) as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Hue in degrees, saturation and lightness 0-1.
fn to_hsl(c: Rgb) -> [f64; 3] {
    let [r, g, b] = c.map(|x| x / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, s, l]
}

pub fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| (v + m) * 255.0)
}

/// WCAG relative luminance.
fn luminance(c: Rgb) -> f64 {
    let [r, g, b] = c.map(|x| {
        let x = x / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

// ponytail: a few dozen CSS names, not all 148; add rows as people ask.
#[rustfmt::skip]
const NAMES: &[(&str, &str)] = &[
    ("black", "000000"), ("white", "ffffff"), ("red", "ff0000"), ("lime", "00ff00"), ("green", "008000"), ("blue", "0000ff"),
    ("yellow", "ffff00"), ("cyan", "00ffff"), ("aqua", "00ffff"), ("magenta", "ff00ff"), ("fuchsia", "ff00ff"),
    ("gray", "808080"), ("grey", "808080"), ("silver", "c0c0c0"), ("maroon", "800000"), ("olive", "808000"),
    ("navy", "000080"), ("purple", "800080"), ("teal", "008080"), ("orange", "ffa500"), ("gold", "ffd700"),
    ("pink", "ffc0cb"), ("hotpink", "ff69b4"), ("brown", "a52a2a"), ("tomato", "ff6347"), ("coral", "ff7f50"),
    ("salmon", "fa8072"), ("crimson", "dc143c"), ("indigo", "4b0082"), ("violet", "ee82ee"), ("plum", "dda0dd"),
    ("lavender", "e6e6fa"), ("beige", "f5f5dc"), ("ivory", "fffff0"), ("khaki", "f0e68c"), ("chocolate", "d2691e"),
    ("tan", "d2b48c"), ("turquoise", "40e0d0"), ("skyblue", "87ceeb"), ("steelblue", "4682b4"), ("royalblue", "4169e1"),
    ("slategray", "708090"), ("forestgreen", "228b22"), ("seagreen", "2e8b57"), ("orchid", "da70d6"),
    ("rebeccapurple", "663399"), ("cornflowerblue", "6495ed"), ("hotdogstand", "ff0000"),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn color() {
        assert_eq!(show(r#"color("tomato")"#), "#ff6347");
        assert_eq!(show(r##"color("#F80")"##), "#ff8800");
        assert_eq!(show("rgb(255, 136, 0)"), "#ff8800");
        assert_eq!(show(r##""#ff8800".rgb"##), "[255, 136, 0]");
        assert_eq!(show("hsl(0, 100, 50)"), "#ff0000");
        assert_eq!(show(r##""#ff0000".hsl"##), "[0, 100, 50]");
        assert_eq!(show(r#"hsl(hsl("teal")[0], hsl("teal")[1], hsl("teal")[2])"#), "#008080");
        assert_eq!(show(r#"mix("black", "white")"#), "#808080");
        assert_eq!(show(r#"lighten("black", 50%)"#), "#808080");
        assert_eq!(show(r#"invert("navy")"#), "#ffff7f");
        assert_eq!(show(r#"contrast("white", "black")"#), "21");
        assert_eq!(show(r#"grayscale("white")"#), "#ffffff");
        assert!(try_eval(r#"color("nope")"#).is_err());
        assert!(try_eval(r#"lighten("red", 2)"#).is_err());
    }
}
