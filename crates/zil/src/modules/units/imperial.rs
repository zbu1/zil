//! Imperial units, shared by the UK and the US, like feet, pounds and miles.

use super::Row;
use super::dims::*;
use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "imperial",
    about: "imperial units like feet, pounds and miles, plus the British gallon and pint",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Imperial units, as used in the UK and, for lengths and weights, the US. It holds feet, inches, miles, pounds, \
ounces and stones, and the British gallon and pint, which differ from the US ones."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("imperial", &[
            ("150 pounds in kilograms", "150 lb to kg"),
            ("a long ton in kilograms", "1 long_ton to kg"),
            ("60 mph in kilometers per hour", "60 mph to km/h"),
        ]),
    ],
    units: TABLE,
    ..Module::EMPTY
};

/// Imperial rows, in the same shape as the metric table.
#[rustfmt::skip]
pub const TABLE: &[Row] = &[
    ("mi mile miles", 1609.344, 0.0, LEN, "mile: the international mile, 5280 feet"),
    ("yd yard yards", 0.9144, 0.0, LEN, "yard: 3 feet, exactly 0.9144 m"),
    ("ft foot feet", 0.3048, 0.0, LEN, "foot: 12 inches, exactly 0.3048 m"),
    ("in inch inches", 0.0254, 0.0, LEN, "inch: exactly 2.54 cm"),
    ("nmi", 1852.0, 0.0, LEN, "nautical mile: exactly 1852 m, about one minute of latitude"),
    ("thou mil mils", 2.54e-5, 0.0, LEN, "thou: a thousandth of an inch, also called a mil"),
    ("fathom fathoms", 1.8288, 0.0, LEN, "fathom: 6 feet, for water depth"),
    ("chain chains", 20.1168, 0.0, LEN, "chain: Gunter's chain, 22 yards or 100 links"),
    ("rod rods perch perches pole poles", 5.0292, 0.0, LEN, "rod: 16.5 feet, a quarter chain; also called a perch or pole"),
    ("span spans", 0.2286, 0.0, LEN, "span: 9 inches, an outstretched hand from thumb to little finger"),
    ("barleycorn barleycorns", 0.0254 / 3.0, 0.0, LEN, "barleycorn: 1/3 inch, the step between shoe sizes"),
    ("link links", 0.201168, 0.0, LEN, "link: a hundredth of Gunter's chain, 7.92 inches"),
    ("lb lbs pound pounds", 0.45359237, 0.0, MASS, "pound: the avoirdupois pound, exactly 0.45359237 kg"),
    ("oz ounce ounces", 0.028349523125, 0.0, MASS, "ounce: the avoirdupois ounce, 1/16 pound; floz is the fluid ounce"),
    ("st stone", 6.35029318, 0.0, MASS, "stone: 14 pounds, used for body weight in the UK"),
    ("gr grain grains", 6.479891e-5, 0.0, MASS, "grain: 1/7000 pound, for bullets and arrows"),
    ("dr dram drams", 1.7718451953125e-3, 0.0, MASS, "dram: the avoirdupois dram, 1/16 ounce"),
    ("ozt", 0.0311034768, 0.0, MASS, "troy ounce: 480 grains, how gold and silver are priced"),
    ("lbt", 0.3732417216, 0.0, MASS, "troy pound: 12 troy ounces"),
    ("scruple scruples", 1.2959782e-3, 0.0, MASS, "scruple: the apothecaries' scruple, 20 grains"),
    ("dwt pennyweight pennyweights", 1.55517384e-3, 0.0, MASS, "pennyweight: 24 grains, 1/20 troy ounce"),
    ("long_ton long_tons", 1016.0469088, 0.0, MASS, "long ton: the imperial ton, 2240 pounds"),
    ("F fahrenheit degF", 5.0 / 9.0, 459.67, TEMP, "degree Fahrenheit: water freezes at 32 and boils at 212; a degree is 5/9 of a kelvin"),
    ("Ra rankine", 5.0 / 9.0, 0.0, TEMP, "degree Rankine: Fahrenheit-sized degrees counted from absolute zero; R is the gas constant"),
    ("gal_uk", 4.54609e-3, 0.0, d(3, 0, 0, 0), "imperial gallon: exactly 4.54609 L"),
    ("qt_uk", 1.1365225e-3, 0.0, d(3, 0, 0, 0), "imperial quart: a quarter imperial gallon"),
    ("pt_uk", 5.6826125e-4, 0.0, d(3, 0, 0, 0), "imperial pint: 20 imperial fluid ounces, a British pint of beer"),
    ("gill_uk", 1.420653125e-4, 0.0, d(3, 0, 0, 0), "imperial gill: 5 imperial fluid ounces"),
    ("floz_uk", 2.84130625e-5, 0.0, d(3, 0, 0, 0), "imperial fluid ounce: 1/160 imperial gallon"),
    ("acre acres", 4046.8564224, 0.0, d(2, 0, 0, 0), "acre: the international acre, 43560 square feet"),
    ("rood roods", 1011.7141056, 0.0, d(2, 0, 0, 0), "rood: a quarter acre"),
    ("cmil", 5.067074790975e-10, 0.0, d(2, 0, 0, 0), "circular mil: the area of a circle one mil across, for wire sizes"),
    ("kcmil MCM", 5.067074790975e-7, 0.0, d(2, 0, 0, 0), "thousand circular mils: for thick wire and cable"),
    ("mph", 0.44704, 0.0, d(1, 0, -1, 0), "mile per hour: road speeds in the US and UK"),
    ("kn knot knots", 1852.0 / 3600.0, 0.0, d(1, 0, -1, 0), "knot: one nautical mile per hour, used at sea and in the air"),
    ("BTU btu", 1055.05585262, 0.0, d(2, 1, -2, 0), "British thermal unit: warms a pound of water by 1 °F; heating and air conditioning"),
    ("thm therm therms", 1.05505585262e8, 0.0, d(2, 1, -2, 0), "therm: 100000 BTU, how natural gas is billed"),
    ("hp horsepower", 745.699_871_582_270_2, 0.0, d(2, 1, -3, 0), "horsepower: mechanical horsepower, 550 foot-pounds per second"),
    ("psi", 6894.757293168, 0.0, d(-1, 1, -2, 0), "pound per square inch: pound-force per square inch; tire pressure in the US"),
    ("inHg", 3386.389, 0.0, d(-1, 1, -2, 0), "inch of mercury: US weather reports and aviation altimeters"),
    ("inH2O", 249.08891, 0.0, d(-1, 1, -2, 0), "inch of water: low pressures in ducts and gas lines"),
    ("ksi", 6894757.293168, 0.0, d(-1, 1, -2, 0), "kilopound per square inch: 1000 psi, for material strength"),
    ("lbf", 4.4482216152605, 0.0, d(1, 1, -2, 0), "pound-force: the weight of one pound under standard gravity"),
    ("ozf", 0.278013850953781, 0.0, d(1, 1, -2, 0), "ounce-force: the weight of an ounce under standard gravity"),
    ("pdl poundal poundals", 0.138254954376, 0.0, d(1, 1, -2, 0), "poundal: accelerates a pound by 1 ft/s^2"),
    ("fc footcandle footcandles", 10.763910416709722, 0.0, LUX, "foot-candle: a lumen per square foot, about 10.76 lux"),
    ("fL foot_lambert foot_lamberts", 3.4262590996353905, 0.0, LUX, "foot-lambert: 1/π candela per square foot; cinema screens"),
    ("mpg_uk", 1609.344 / 4.54609e-3, 0.0, FUEL, "mile per imperial gallon: fuel economy in the UK"),
];
