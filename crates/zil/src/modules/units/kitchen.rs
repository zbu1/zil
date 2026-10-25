//! Kitchen conversions: ingredient densities, so cups become grams.

use super::terms;
use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "kitchen",
    about: "convert recipes between cups and grams using how dense each ingredient is",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Converting recipes between volumes, like cups, and weights, like grams. To convert you need the ingredient's density, \
since a cup of flour weighs much less than a cup of honey. Multiply a volume by `density(\"flour\")` to get grams, or \
divide grams by it to get cups."),
        ("good to know", "\
Baking densities assume the ingredient is spooned in and leveled, not packed."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("kitchen", &[
            ("a cup of flour", r#"1 cup * density("flour") to g"#),
            ("250 g of sugar in cups", r#"250 g / density("sugar") to cup"#),
            ("a stick of butter", r#"0.5 cup * density("butter") to g"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{DENSITIES, terms};
    use crate::modules::Fail;
    use crate::signature::{As, Qty};
    use crate::value::Value;

    /// the typical density of an ingredient in g/mL, so a volume times it gives a mass
    #[builtin(examples = [r#"density("honey")"#, r#"2 tbsp * density("butter") to g"#])]
    fn density(ingredient: &str) -> Result<As<Qty>, Fail> {
        let key = ingredient.trim().to_lowercase().replace([' ', '-'], "_");
        let key = key.trim_end_matches('s');
        let hit = DENSITIES.iter().find(|d| d.0.split(' ').any(|n| n.trim_end_matches('s') == key));
        let (_, g_ml) = hit.ok_or_else(|| {
            let names: Vec<_> = DENSITIES.iter().map(|d| d.0.split(' ').next().unwrap()).collect();
            Fail::Arg(0, format!("no density for {ingredient:?}\nnote: known: {}", names.join(", ")))
        })?;
        Ok(Value::qty(*g_ml, terms("g mL^-1")?).into())
    }
}

// ponytail: one typical density per ingredient; brands and packing vary by ±10%.
#[rustfmt::skip]
const DENSITIES: &[(&str, f64)] = &[
    ("water", 1.0), ("milk", 1.03), ("cream heavy_cream", 0.99), ("yogurt", 1.03), ("buttermilk", 1.03),
    ("butter", 0.96), ("oil olive_oil vegetable_oil", 0.92), ("honey", 1.42), ("maple_syrup syrup", 1.32), ("molasses", 1.4),
    ("flour all_purpose_flour", 0.53), ("bread_flour", 0.55), ("whole_wheat_flour", 0.51), ("cake_flour", 0.48), ("almond_flour", 0.41),
    ("sugar granulated_sugar", 0.85), ("brown_sugar", 0.93), ("powdered_sugar icing_sugar", 0.51),
    ("cocoa cocoa_powder", 0.42), ("cornstarch", 0.54), ("baking_soda", 0.97), ("baking_powder", 0.81), ("salt table_salt", 1.2), ("kosher_salt", 0.6),
    ("rice", 0.85), ("oats rolled_oats", 0.38), ("chocolate_chips", 0.72), ("peanut_butter", 1.08), ("rice_flour", 0.67),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn kitchen() {
        assert_eq!(show(r#"(1 cup * density("flour") to g).round"#), "125 g");
        assert_eq!(show(r#"(1 cup * density("Brown Sugar") to g).round"#), "220 g");
        assert_eq!(show(r#"(1 cup * density("chocolate chip") to g).round"#), "170 g");
        assert_eq!(show(r#"(250 g / density("sugar") to cup).round(2)"#), "1.24 cup");
        assert!(try_eval(r#"density("unobtainium")"#).is_err());
    }
}
