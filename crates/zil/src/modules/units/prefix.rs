//! SI and binary prefixes on units: `Gm`, `kibit`, `megajoule`, `picometers`.

use super::{TABLE, UnitDef, registry};
use std::cell::Cell;
use std::rc::Rc;

/// Symbols (the first is the one shown), full name and power of ten, quetta down to quecto.
const DECIMAL: &[(&str, &str, i32)] = &[
    ("Q", "quetta", 30),
    ("R", "ronna", 27),
    ("Y", "yotta", 24),
    ("Z", "zetta", 21),
    ("E", "exa", 18),
    ("P", "peta", 15),
    ("T", "tera", 12),
    ("G", "giga", 9),
    ("M", "mega", 6),
    ("k", "kilo", 3),
    ("h", "hecto", 2),
    ("da", "deca", 1),
    ("d", "deci", -1),
    ("c", "centi", -2),
    ("m", "milli", -3),
    ("µ u μ", "micro", -6),
    ("n", "nano", -9),
    ("p", "pico", -12),
    ("f", "femto", -15),
    ("a", "atto", -18),
    ("z", "zepto", -21),
    ("y", "yocto", -24),
    ("r", "ronto", -27),
    ("q", "quecto", -30),
];

/// Symbol, full name and power of two.
#[rustfmt::skip]
const BINARY: &[(&str, &str, i32)] = &[
    ("Ki ki", "kibi", 10), ("Mi mi", "mebi", 20), ("Gi gi", "gibi", 30), ("Ti ti", "tebi", 40),
    ("Pi pi", "pebi", 50), ("Ei ei", "exbi", 60), ("Zi zi", "zebi", 70), ("Yi yi", "yobi", 80),
];

/// The SI units and the few others the prefixes go on, by their first name in the table. `farad` and `coulomb` have
/// no symbol of their own (`F` and `C` are Fahrenheit and Celsius), so they only take full-name prefixes.
#[rustfmt::skip]
const PREFIXABLE: &[&str] = &[
    "m", "g", "s", "A", "K", "mol", "cd", "Hz", "N", "Pa", "J", "W", "V", "farad", "coulomb", "Ω", "S", "Wb", "T", "H",
    "lm", "lx", "Bq", "Gy", "Sv", "kat", "rad", "sr", "L", "t", "eV", "Da", "bar", "Wh", "Ah", "cal", "M", "bit", "B",
];

/// Data units take binary prefixes, and decimal ones only from kilo up.
fn is_data(base: &str) -> bool {
    matches!(base, "bit" | "B")
}

/// The prefixable unit that `rest` spells: its symbol, or one of its lowercase names.
fn base_of(rest: &str, symbol: bool) -> Option<&'static str> {
    PREFIXABLE.iter().copied().find(|p| {
        let Some(row) = TABLE.iter().find(|r| r.0.split(' ').next() == Some(*p)) else { return false };
        if symbol {
            rest == *p && p.chars().count() <= 3
        } else {
            row.0.split(' ').any(|n| n == rest && n.len() >= 3 && n.bytes().all(|b| b.is_ascii_lowercase()))
        }
    })
}

/// `name` as the shown prefix symbol, the unit and the prefix's factor.
fn split(name: &str) -> Option<(&'static str, &'static str, f64)> {
    for (binary, table) in [(false, DECIMAL), (true, BINARY)] {
        for (symbols, word, exp) in table {
            let by_symbol = symbols.split(' ').filter_map(|s| name.strip_prefix(s)).map(|r| (r, true));
            for (rest, symbol) in by_symbol.chain(name.strip_prefix(word).map(|r| (r, false))) {
                let Some(base) = base_of(rest, symbol) else { continue };
                if (binary && !is_data(base)) || (!binary && is_data(base) && *exp < 3) {
                    continue;
                }
                let factor = if binary { 2f64.powi(*exp) } else { format!("1e{exp}").parse().ok()? };
                return Some((symbols.split(' ').next().unwrap_or_default(), base, factor));
            }
        }
    }
    None
}

/// The unit `name` spells with a prefix, made once and kept for the session. A builtin function or constant keeps its name.
pub fn parse(name: &str) -> Option<Rc<UnitDef>> {
    let (prefix, base, factor) = split(name)?;
    let shown = format!("{prefix}{base}");
    if let Some(def) = registry(|r| r.borrow().get(&shown).cloned()) {
        return Some(def);
    }
    let taken =
        crate::modules::modules().any(|m| m.fns.iter().any(|f| f.is(name)) || m.consts.iter().any(|c| c.0 == name) || m.quantities.iter().any(|c| c.0 == name));
    if taken {
        return None;
    }
    let row = TABLE.iter().find(|r| r.0.split(' ').next() == Some(base))?;
    let def = Rc::new(UnitDef { name: shown.clone(), scale: Cell::new(row.1 * factor), offset: 0.0, dim: row.3 });
    registry(|r| r.borrow_mut().insert(shown, def.clone()));
    Some(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A prefix and unit that spell another table unit mean something else, so the exact name wins.
    #[test]
    fn exact_names_agree() {
        let mut clashes = vec![];
        for (symbols, _, exp) in DECIMAL {
            let prefix = symbols.split(' ').next().unwrap_or_default();
            for base in PREFIXABLE.iter().filter(|b| b.chars().count() <= 3 && !is_data(b)) {
                let name = format!("{prefix}{base}");
                let Some(exact) = super::super::rows().find(|r| r.0.split(' ').any(|n| n == name)) else { continue };
                let row = TABLE.iter().find(|r| r.0.split(' ').next() == Some(*base)).expect("prefixable unit");
                let want = row.1 * format!("1e{exp}").parse::<f64>().expect("power of ten");
                if exact.3 != row.3 || (exact.1 / want - 1.0).abs() > 1e-9 {
                    clashes.push(name);
                }
            }
        }
        clashes.sort();
        assert_eq!(clashes, ["Gs", "PS", "ct", "fL", "ft", "nt", "pt", "qt"], "a new clash: the exact unit wins, add it here if that is right");
    }

    #[test]
    fn prefixed_names() {
        use crate::interp::tests::{show, try_eval};
        assert_eq!(show("1 Gm to km"), "1000000 km");
        assert_eq!(show("1 mJ to J"), "0.001 J");
        assert_eq!(show("3 ks to s"), "3000 s");
        assert_eq!(show("1 megajoule to kJ"), "1000 kJ");
        assert_eq!(show("2 picometers"), "2 pm");
        assert_eq!(show("3 kibit to bit"), "3072 bit");
        assert_eq!(show("1 kibibyte to B"), "1024 B");
        assert_eq!(show("1 kB to B"), "1000 B");
        assert_eq!(show("1 Gs to T"), "0.0001 T");
        assert_eq!(show("1 kiloohm to Ω"), "1000 Ω");
        assert!(try_eval("1 mB").is_err(), "no millibytes");
        assert!(try_eval("1 kmin").is_err(), "minutes take no prefix");
        assert!(try_eval("1 kC").is_err(), "C is Celsius");
        assert!(try_eval("1 kilofoot").is_err());
    }
}
