//! Every built-in unit, one row each: the unit modules' tables joined, metric first.

use super::dims::{Dim, NONE};
use super::{astro, cgs, imperial, metric, us_customary};

/// A unit row: space-separated names (primary first), scale to SI, offset, dimension, "full name: description" for help.
pub type Row = (&'static str, f64, f64, Dim, &'static str);

const PARTS: &[&[Row]] = &[metric::TABLE, imperial::TABLE, us_customary::TABLE, cgs::TABLE, astro::TABLE];

const fn total() -> usize {
    let (mut n, mut i) = (0, 0);
    while i < PARTS.len() {
        n += PARTS[i].len();
        i += 1;
    }
    n
}

const fn join() -> [Row; total()] {
    let mut out = [PARTS[0][0]; total()];
    let (mut at, mut i) = (0, 0);
    while i < PARTS.len() {
        let mut j = 0;
        while j < PARTS[i].len() {
            out[at] = PARTS[i][j];
            at += 1;
            j += 1;
        }
        i += 1;
    }
    out
}

static JOINED: [Row; total()] = join();

/// Metric first: within a dimension the first unit with factor 1 is the one metric products are renamed to.
pub const TABLE: &[Row] = &JOINED;

/// Whether `name` is a dimensionless power of ten, `million` and the like, that can sit between a number and a unit.
pub fn is_number_word(name: &str) -> bool {
    TABLE.iter().any(|r| r.3 == NONE && r.1 >= 1e3 && r.0.split(' ').any(|n| n == name))
}

/// What the short-scale `-illion` word `name` is worth in the long scale: `billion` is 10^12 there.
pub fn long_scale(name: &str, short: f64) -> Option<f64> {
    (name.ends_with("illion") && short >= 1e9 && is_number_word(name)).then(|| 10f64.powf(2.0 * (short.log10().round() - 3.0)))
}
