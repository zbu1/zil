//! Pictures as text for `img`: PNG, JPEG and GIF files drawn with an ASCII ramp, colored half blocks or dithered braille.

use super::{Dots, fg};
use crate::ansi::RESET;
use crate::modules::Fail;
use image::imageops::FilterType;

/// The image at `path`, `width` characters wide (the terminal's width without one).
pub fn img(path: &str, width: Option<i64>, mode: &str) -> Result<String, Fail> {
    let width = match width {
        None => crate::ansi::columns(),
        Some(w) if (1..=1000).contains(&w) => w as usize,
        Some(_) => return Err(Fail::Arg(1, "width must be an int from 1 to 1000".into())),
    };
    if !["blocks", "ascii", "braille"].contains(&mode) {
        return Err(Fail::Arg(2, "mode is \"blocks\", \"ascii\" or \"braille\"".into()));
    }
    let bytes = crate::host::get().read(path).map_err(|e| Fail::Arg(0, format!("can't open {path}: {e}")))?;
    let pic = image::load_from_memory(&bytes).map_err(|e| Fail::Arg(0, format!("can't open {path}: {e}")))?;
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
    use crate::interp::tests::try_eval;

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
}
