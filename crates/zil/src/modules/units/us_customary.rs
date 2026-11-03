//! US customary units: liquid and dry measures, cooking spoons, survey units.

use super::Row;
use super::dims::*;
use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "us_customary",
    about: "US customary units like gallons, cups and the short ton, which differ from the imperial ones",
    #[rustfmt::skip]
    intro: &[
        ("", "\
US customary units that differ from their imperial namesakes, like the gallon, quart and pint, plus cups and \
spoons, the short ton, and land survey units like the section."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("us_customary", &[
            ("a gallon in liters", "1 gal to L"),
            ("two cups in milliliters", "2 cup to mL"),
            ("a short ton in kilograms", "1 short_ton to kg"),
        ]),
    ],
    units: TABLE,
    ..Module::EMPTY
};

/// US customary rows, in the same shape as the metric table.
#[rustfmt::skip]
pub const TABLE: &[Row] = &[
    ("ft_us", 1200.0 / 3937.0, 0.0, LEN, "US survey foot: 1200/3937 m, slightly longer than the international foot"),
    ("short_ton short_tons", 907.18474, 0.0, MASS, "short ton: the US ton, 2000 pounds"),
    ("cwt hundredweight hundredweights", 45.359237, 0.0, MASS, "hundredweight: the US hundredweight, 100 pounds"),
    ("gal gallon gallons", 3.785411784e-3, 0.0, d(3, 0, 0, 0), "gallon: the US liquid gallon, 231 cubic inches"),
    ("qt quart quarts", 9.46352946e-4, 0.0, d(3, 0, 0, 0), "quart: the US liquid quart, a quarter gallon"),
    ("pt pint pints", 4.73176473e-4, 0.0, d(3, 0, 0, 0), "pint: the US liquid pint, half a quart"),
    ("cup cups", 2.365882365e-4, 0.0, d(3, 0, 0, 0), "cup: the US customary cup, half a pint"),
    ("floz", 2.95735295625e-5, 0.0, d(3, 0, 0, 0), "fluid ounce: the US fluid ounce, 1/8 cup"),
    ("tbsp tablespoon tablespoons", 1.478676478125e-5, 0.0, d(3, 0, 0, 0), "tablespoon: the US tablespoon, half a fluid ounce"),
    ("tsp teaspoon teaspoons", 4.92892159375e-6, 0.0, d(3, 0, 0, 0), "teaspoon: the US teaspoon, a third of a tablespoon"),
    ("gill gills", 1.1829411825e-4, 0.0, d(3, 0, 0, 0), "gill: the US gill, 4 fluid ounces"),
    ("fldr", 3.6966911953125e-6, 0.0, d(3, 0, 0, 0), "fluid dram: the US fluid dram, 1/8 fluid ounce"),
    ("minim minims", 6.1611519921875e-8, 0.0, d(3, 0, 0, 0), "minim: the US minim, 1/60 fluid dram"),
    ("butt butts", 126.0 * 3.785411784e-3, 0.0, d(3, 0, 0, 0), "butt: the wine butt, two hogsheads or 126 US gallons"),
    ("tun tuns", 252.0 * 3.785411784e-3, 0.0, d(3, 0, 0, 0), "tun: the wine tun, two butts or 252 US gallons"),
    ("bu bushel bushels", 3.523907016688e-2, 0.0, d(3, 0, 0, 0), "bushel: the US dry bushel, for grain"),
    ("pk peck pecks", 8.80976754172e-3, 0.0, d(3, 0, 0, 0), "peck: a quarter US bushel"),
    ("dry_qt", 1.101220942715e-3, 0.0, d(3, 0, 0, 0), "dry quart: the US dry quart, 1/32 bushel"),
    ("dry_pt", 5.506104713575e-4, 0.0, d(3, 0, 0, 0), "dry pint: half a US dry quart"),
    ("section sections", 2589988.110336, 0.0, d(2, 0, 0, 0), "section: one square mile of US land survey"),
    ("township townships", 93239571.972096, 0.0, d(2, 0, 0, 0), "township: 36 sections, six miles on a side"),
    ("TR ton_refrigeration", 3516.8528420667, 0.0, d(2, 1, -3, 0), "ton of refrigeration: cooling that melts a short ton of ice a day"),
    ("cfm", 0.028316846592 / 60.0, 0.0, FLOW, "cubic foot per minute: airflow of fans and ducts"),
    ("cfs", 0.028316846592, 0.0, FLOW, "cubic foot per second: river flow in the US"),
    ("gpm", 3.785411784e-3 / 60.0, 0.0, FLOW, "US gallon per minute: pumps and plumbing"),
    ("gph", 3.785411784e-3 / 3600.0, 0.0, FLOW, "US gallon per hour: slow flows like drip irrigation"),
    ("mpg", 1609.344 / 3.785411784e-3, 0.0, FUEL, "mile per US gallon: fuel economy in the US"),
];
