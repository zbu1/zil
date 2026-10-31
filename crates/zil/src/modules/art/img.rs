//! Pictures as text for `img`: PNG, JPEG and GIF files drawn with an ASCII ramp, colored half blocks or dithered braille.

use super::{Dots, fg};
use crate::ansi::RESET;
use crate::modules::Fail;
use crate::signature::Or;
use image::imageops::FilterType;

/// An image from a file's path or its bytes.
pub fn load(file: Or<&str, &[u8]>) -> Result<image::DynamicImage, Fail> {
    let (name, bytes) = match file {
        Or::A(p) => (p, crate::host::get().read(p).map_err(|e| Fail::Arg(0, format!("can't open {p}: {e}")))?),
        Or::B(b) => ("the image", b.to_vec()),
    };
    image::load_from_memory(&bytes).map_err(|e| Fail::Arg(0, format!("can't open {name}: {e}")))
}

/// The low `bits` bits of each pixel's `channels`, row by row, packed most significant first into bytes.
pub fn lsb(pic: &image::DynamicImage, bits: i64, channels: &str) -> Result<Vec<u8>, Fail> {
    if !(1..=8).contains(&bits) {
        return Err(Fail::Arg(1, format!("bits goes from 1 to 8, got {bits}")));
    }
    let idx: Vec<usize> = channels
        .chars()
        .map(|c| "rgba".find(c))
        .collect::<Option<_>>()
        .filter(|v: &Vec<_>| !v.is_empty())
        .ok_or_else(|| Fail::Arg(2, format!("channels is some of the letters r, g, b and a, like \"rgb\", got {channels:?}")))?;
    let px = pic.to_rgba8();
    let (mut out, mut acc, mut n) = (vec![], 0u8, 0);
    for p in px.pixels() {
        for &i in &idx {
            for k in (0..bits).rev() {
                acc = acc << 1 | (p[i] >> k) & 1;
                n += 1;
                if n == 8 {
                    out.push(acc);
                    (acc, n) = (0, 0);
                }
            }
        }
    }
    Ok(out)
}

/// A black and white PNG of one bit of one channel: white where it's set.
pub fn bit_plane(pic: &image::DynamicImage, channel: &str, bit: i64) -> Result<Vec<u8>, Fail> {
    let i = ["r", "g", "b", "a"]
        .iter()
        .position(|c| *c == channel)
        .ok_or_else(|| Fail::Arg(1, format!("channel is \"r\", \"g\", \"b\" or \"a\", got {channel:?}")))?;
    if !(0..8).contains(&bit) {
        return Err(Fail::Arg(2, format!("bit goes from 0, the lowest, to 7, got {bit}")));
    }
    let px = pic.to_rgba8();
    let plane = image::GrayImage::from_fn(px.width(), px.height(), |x, y| image::Luma([if px.get_pixel(x, y)[i] >> bit & 1 == 1 { 255 } else { 0 }]));
    let mut out = std::io::Cursor::new(vec![]);
    plane.write_to(&mut out, image::ImageFormat::Png).map_err(|e| Fail::Msg(e.to_string()))?;
    Ok(out.into_inner())
}

/// The image at `path`, `width` characters wide (the terminal's width without one).
pub fn img(file: Or<&str, &[u8]>, width: Option<i64>, mode: &str) -> Result<String, Fail> {
    let width = match width {
        None => crate::ansi::columns(),
        Some(w) if (1..=1000).contains(&w) => w as usize,
        Some(_) => return Err(Fail::Arg(1, "width must be an int from 1 to 1000".into())),
    };
    if !["blocks", "ascii", "braille"].contains(&mode) {
        return Err(Fail::Arg(2, "mode is \"blocks\", \"ascii\" or \"braille\"".into()));
    }
    let pic = load(file)?;
    // Braille samples the most: 2×4 dots for each of width × width·aspect/2 cells.
    let aspect = pic.height() as f64 / pic.width().max(1) as f64;
    crate::state::check_len((width as f64 * width as f64 * aspect * 4.0) as usize, "pixels").map_err(Fail::Msg)?;
    Ok(draw(&pic, width, mode))
}

/// Pixels resized to `w`×`h`, transparency composited onto black.
fn pixels(pic: &image::DynamicImage, w: usize, h: usize) -> Vec<Vec<[f64; 3]>> {
    let small = pic.resize_exact(w.max(1) as u32, h.max(1) as u32, FilterType::Triangle).to_rgba8();
    small.rows().map(|row| row.map(|p| [0, 1, 2].map(|i| p[i] as f64 * p[3] as f64 / 255.0)).collect()).collect()
}

fn brightness([r, g, b]: [f64; 3]) -> f64 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

fn draw(pic: &image::DynamicImage, width: usize, mode: &str) -> String {
    // ponytail: a character is taken as twice as tall as wide; real fonts vary a little.
    let aspect = pic.height() as f64 / pic.width().max(1) as f64;
    let rows_for = |cols: usize, px_per_row: f64| ((cols as f64 * aspect * px_per_row / 2.0).round() as usize).max(1);
    match mode {
        "ascii" => {
            const RAMP: &[u8] = b" .:-=+*#%@";
            let px = pixels(pic, width, rows_for(width, 1.0));
            let lines =
                px.iter().map(|row| row.iter().map(|p| RAMP[(brightness(*p) * 9.0).round() as usize] as char).collect::<String>().trim_end().to_string());
            lines.collect::<Vec<_>>().join("\n")
        }
        "braille" => {
            let (w, h) = (width * 2, rows_for(width, 2.0) * 4);
            let mut lum: Vec<Vec<f64>> = pixels(pic, w, h).iter().map(|row| row.iter().map(|p| brightness(*p)).collect()).collect();
            let mut dots = Dots::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let on = lum[y][x] > 0.5;
                    if on {
                        dots.set(x as i64, y as i64);
                    }
                    let err = lum[y][x] - if on { 1.0 } else { 0.0 };
                    for (dx, dy, k) in [(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)] {
                        let (nx, ny) = (x as i64 + dx, y + dy);
                        if (0..w as i64).contains(&nx) && ny < h {
                            lum[ny][nx as usize] += err * k / 16.0;
                        }
                    }
                }
            }
            dots.render()
        }
        _ => {
            let px = pixels(pic, width, rows_for(width, 2.0) * 2);
            let bg = |[r, g, b]: [f64; 3]| format!("\x1b[48;2;{};{};{}m", r.round() as u8, g.round() as u8, b.round() as u8);
            let lines = px.chunks(2).map(|pair| {
                let cells = (0..width).map(|x| format!("{}{}▀", fg(pair[0][x]), pair.get(1).map_or(String::new(), |b| bg(b[x]))));
                cells.collect::<String>() + RESET
            });
            lines.collect::<Vec<_>>().join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn img() {
        // Left half black, right half white, 8×4 pixels.
        let pic = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(8, 4, |x, _| if x < 4 { image::Rgb([0, 0, 0]) } else { image::Rgb([255; 3]) }));
        assert_eq!(draw(&pic, 8, "ascii"), "    @@@@\n    @@@@");
        assert_eq!(draw(&pic, 4, "braille"), "  ⣿⣿\n  ⣿⣿");
        let blocks = draw(&pic, 8, "blocks");
        assert_eq!(blocks.lines().count(), 4);
        assert!(blocks.starts_with("\x1b[38;2;0;0;0m\x1b[48;2;0;0;0m▀"));
        assert!(blocks.contains("\x1b[38;2;255;255;255m\x1b[48;2;255;255;255m▀"));
        assert!(try_eval(r#"img("no/such.png")"#).is_err());
        assert!(try_eval(r#"img("x.png", 10, "sixel")"#).is_err());
    }

    #[test]
    fn stego() {
        // "hi" hidden in the low bit of red along a 16×1 row.
        let bits: Vec<u8> = b"hi".iter().flat_map(|c| (0..8).rev().map(move |k| c >> k & 1)).collect();
        let pic = image::RgbImage::from_fn(16, 1, |x, _| image::Rgb([200 | bits[x as usize], 7, 0]));
        let mut png = std::io::Cursor::new(vec![]);
        image::DynamicImage::ImageRgb8(pic).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let b = format!("b\"{}\"", png.into_inner().iter().map(|x| format!("\\x{x:02x}")).collect::<String>());
        assert_eq!(show(&format!(r#"lsb({b}, 1, "r").str"#)), "hi");
        assert_eq!(show(&format!(r#"lsb({b}, 2, "g")"#)), r#"b"\xff\xff\xff\xff""#);
        assert_eq!(show(&format!("lsb({b}).len")), "6");
        let plane = format!(r#"bit_plane({b}, "r", 0)"#);
        assert_eq!(show(&format!(r#"lsb({plane}, 1, "r").str"#)), "hi");
        assert!(try_eval(&format!(r#"img({plane}, 16, "ascii")"#)).is_ok());
        assert!(try_eval(&format!("lsb({b}, 9)")).is_err());
        assert!(try_eval(&format!(r#"lsb({b}, 1, "x")"#)).is_err());
        assert!(try_eval(&format!(r#"bit_plane({b}, "q", 0)"#)).is_err());
        assert!(try_eval(&format!(r#"bit_plane({b}, "r", 8)"#)).is_err());
        assert!(try_eval(r#"lsb(b"not an image")"#).is_err());
    }
}
