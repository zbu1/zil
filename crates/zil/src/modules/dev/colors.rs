//! Colors as "#rrggbb" strings: parse, convert, mix, adjust, contrast, preview.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
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
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("make", &["color", "rgb", "hsl"]),
        ("adjust", &["mix", "lighten", "darken", "grayscale"]),
        ("inspect", &["contrast", "swatch"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("color", "color(c: str|list) -> str", "a color in \"#rrggbb\" form, from a CSS name, \"#rgb\", \"#rrggbb\" or a list [r, g, b]", &[r#"color("tomato")"#, r##"color("#f80")"##, "color([255, 0, 128])"], &["rgb", "hsl"]),
    doc("rgb", "rgb(r: num, g: num, b: num) -> str / rgb(c: str|list) -> list", "make a color from red, green and blue amounts (0-255), or give one argument to get a color's three amounts", &["rgb(255, 136, 0)", r#""orange".rgb"#], &["hsl", "color"]),
    doc("hsl", "hsl(h: num, s: num, l: num) -> str / hsl(c: str|list) -> list", "make a color from hue, saturation and lightness, or give one argument to get a color's three values", &["hsl(210, 80, 50)", r#""teal".hsl"#], &["rgb", "color"])
        .detail("Hue is the position on the color wheel in degrees (0 red, 120 green, 240 blue), saturation how vivid it is and lightness how close to white, both from 0 to 100"),
    doc("mix", "mix(a: str|list, b: str|list, t?: num) -> str", "blend two colors, where t is how far to go from a toward b, 0 to 1 (halfway by default)", &[r#"mix("red", "blue")"#, r#"mix("white", "black", 25%)"#], &["lighten", "darken"]),
    doc("lighten", "lighten(c: str|list, amount: num) -> str", "make a color lighter, keeping its hue", &[r##"lighten("#3478f6", 20%)"##], &["darken", "mix"])
        .detail("amount, like 20%, is added to its HSL lightness"),
    doc("darken", "darken(c: str|list, amount: num) -> str", "make a color darker, keeping its hue", &[r##"darken("#3478f6", 20%)"##], &["lighten", "mix"])
        .detail("amount, like 20%, is taken off its HSL lightness"),
    doc("grayscale", "grayscale(c: str|list) -> str", "the gray that looks as bright as the color does", &[r#"grayscale("tomato")"#], &["invert"]),
    doc("contrast", "contrast(a: str|list, b: str|list) -> float", "how readable one color is on the other, as the WCAG contrast ratio from 1 (identical) to 21 (black on white)", &[r#"contrast("white", "black")"#, r##"contrast("#777", "white")"##], &["swatch"])
        .detail("Body text needs 4.5 or more and large text 3"),
    doc("swatch", "swatch(c: str|list) -> str", "a block of the color followed by its hex code, to preview it", &[r#"swatch("tomato")"#, r#"["red", "gold", "teal"].map(swatch).join(" ")"#], &["color"])
        .detail("Needs a terminal with full (24-bit) color"),
];

pub type Rgb = [f64; 3];

/// Also handles the color form of `invert`, which `data.maps` owns and forwards here.
pub fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let c = |v: &Value| {
        parse(v).ok_or_else(|| Fail::from(format!("{v:?} is not a color\nhelp: use a name like \"tomato\", \"#f80\", \"#ff8800\" or [255, 136, 0]")))
    };
    let amount = |v: &Value| num(v).filter(|t| (0.0..=1.0).contains(t)).ok_or(Fail::from("amount must be from 0 to 1, like 0.2 or 20%"));
    Ok(match (name, args) {
        ("color", [v]) => hex(c(v)?),
        ("rgb", [r, g, b]) => hex(c(&Value::list(vec![r.clone(), g.clone(), b.clone()]))?),
        ("rgb", [v]) => Value::list(c(v)?.iter().map(|x| Value::int(x.round() as i64)).collect()),
        ("hsl", [h, s, l]) => match (num(h), num(s), num(l)) {
            (Some(h), Some(s), Some(l)) if (0.0..=100.0).contains(&s) && (0.0..=100.0).contains(&l) => hex(from_hsl(h, s / 100.0, l / 100.0)),
            _ => return Err("expected hue in degrees, saturation and lightness 0-100".into()),
        },
        ("hsl", [v]) => {
            let [h, s, l] = to_hsl(c(v)?);
            Value::list(vec![Value::int(h.round() as i64), Value::int((s * 100.0).round() as i64), Value::int((l * 100.0).round() as i64)])
        }
        ("mix", [a, b]) => hex(mix(c(a)?, c(b)?, 0.5)),
        ("mix", [a, b, t]) => hex(mix(c(a)?, c(b)?, amount(t)?)),
        ("lighten" | "darken", [v, t]) => {
            let [h, s, l] = to_hsl(c(v)?);
            let t = if name == "lighten" { amount(t)? } else { -amount(t)? };
            hex(from_hsl(h, s, (l + t).clamp(0.0, 1.0)))
        }
        ("invert", [v]) => hex(c(v)?.map(|x| 255.0 - x)),
        ("grayscale", [v]) => {
            let y = luminance(c(v)?);
            // Back from linear light to an sRGB channel.
            let g = if y <= 0.0031308 { y * 12.92 } else { 1.055 * y.powf(1.0 / 2.4) - 0.055 };
            hex([g * 255.0; 3])
        }
        ("contrast", [a, b]) => {
            let (x, y) = (luminance(c(a)?), luminance(c(b)?));
            Float(((x.max(y) + 0.05) / (x.min(y) + 0.05) * 100.0).round() / 100.0)
        }
        ("swatch", [v]) => {
            let [r, g, b] = c(v)?.map(|x| x.round() as u8);
            Value::str(format!("\x1b[48;2;{r};{g};{b}m    \x1b[0m {}", hex([r as f64, g as f64, b as f64])))
        }
        _ => return Err(Fail::BadArgs),
    })
}

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

fn hex(c: Rgb) -> Value {
    let [r, g, b] = c.map(|x| x.round().clamp(0.0, 255.0) as u8);
    Value::str(format!("#{r:02x}{g:02x}{b:02x}"))
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
