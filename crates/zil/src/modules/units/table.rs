//! Every built-in unit, one row each: the unit modules' tables joined, metric first.

use super::dims::Dim;
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
