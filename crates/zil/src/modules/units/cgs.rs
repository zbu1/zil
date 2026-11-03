//! CGS units: the centimeter-gram-second system and its electromagnetic and radiation relatives.

use super::Row;
use super::dims::*;
use crate::modules::Module;
use std::f64::consts::PI;

pub const MODULE: Module = Module {
    name: "cgs",
    about: "CGS units like the erg, dyne and gauss, and other older scientific units",
    #[rustfmt::skip]
    intro: &[
        ("", "\
The centimeter-gram-second system and the older scientific units that grew from it, like the erg, dyne, poise \
and gauss, the electrostatic and electromagnetic units, and the old radiation units like the rem and curie."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("cgs", &[
            ("an erg in joules", "1 erg to J"),
            ("a gauss in tesla", "1 gauss to T"),
            ("water's viscosity", "1 cP to Pa*s"),
        ]),
    ],
    units: TABLE,
    ..Module::EMPTY
};

/// CGS rows, in the same shape as the metric table.
#[rustfmt::skip]
pub const TABLE: &[Row] = &[
    ("erg ergs", 1e-7, 0.0, d(2, 1, -2, 0), "erg: the CGS unit of energy, 10^-7 J"),
    ("barye baryes", 0.1, 0.0, d(-1, 1, -2, 0), "barye: the CGS unit of pressure, a dyne per square centimeter"),
    ("dyn dyne dynes", 1e-5, 0.0, d(1, 1, -2, 0), "dyne: the CGS unit of force, 10^-5 N"),
    ("abA abampere abamperes biot biots", 10.0, 0.0, CURRENT, "abampere: the CGS-EMU unit of current, 10 A"),
    ("statV statvolt statvolts", 299.792458, 0.0, VOLT, "statvolt: the CGS-ESU unit of voltage, about 300 V"),
    ("abV abvolt abvolts", 1e-8, 0.0, VOLT, "abvolt: the CGS-EMU unit of voltage, 10^-8 V"),
    ("statohm statohms", 8.987551787368176e11, 0.0, OHM, "statohm: the CGS-ESU unit of resistance"),
    ("abohm abohms", 1e-9, 0.0, OHM, "abohm: the CGS-EMU unit of resistance, 10^-9 Ω"),
    ("statC statcoulomb statcoulombs franklin franklins", 3.33564095198152e-10, 0.0, COULOMB, "statcoulomb: the CGS-ESU unit of charge"),
    ("Mx maxwell maxwells", 1e-8, 0.0, WEBER, "maxwell: the CGS unit of magnetic flux, 10^-8 Wb"),
    ("Gs gauss", 1e-4, 0.0, TESLA, "gauss: the CGS unit of magnetic field, 10^-4 T; G is the gravitational constant"),
    ("abH abhenry abhenries", 1e-9, 0.0, HENRY, "abhenry: the CGS-EMU unit of inductance, 10^-9 H"),
    ("Ci curie curies", 3.7e10, 0.0, PER_TIME, "curie: radioactivity of a gram of radium, 3.7e10 Bq"),
    ("Rd rutherford rutherfords", 1e6, 0.0, PER_TIME, "rutherford: 10^6 Bq"),
    ("rad_dose", 0.01, 0.0, DOSE, "rad: the old unit of absorbed dose, 0.01 Gy; rad is the radian"),
    ("rem", 0.01, 0.0, DOSE, "rem: the old unit of equivalent dose, 0.01 Sv"),
    ("phot phots", 1e4, 0.0, LUX, "phot: the CGS unit of illuminance, 10^4 lux"),
    ("sb stilb stilbs", 1e4, 0.0, LUX, "stilb: the CGS unit of luminance, 10^4 nits"),
    ("lambert lamberts", 1e4 / PI, 0.0, LUX, "lambert: 1/π stilb"),
    ("poise", 0.1, 0.0, VISCOSITY, "poise: the CGS unit of dynamic viscosity, 0.1 Pa*s"),
    ("cP centipoise", 1e-3, 0.0, VISCOSITY, "centipoise: a hundredth of a poise; water is about 1 cP"),
    ("St stokes", 1e-4, 0.0, KINEMATIC_VISCOSITY, "stokes: the CGS unit of kinematic viscosity, 1 cm^2/s"),
    ("cSt centistokes", 1e-6, 0.0, KINEMATIC_VISCOSITY, "centistokes: a hundredth of a stokes; motor oil ratings"),
    ("galileo galileos", 0.01, 0.0, d(1, 0, -2, 0), "galileo: the CGS unit of acceleration, 1 cm/s^2; for gravity surveys"),
    ("kayser kaysers", 100.0, 0.0, d(-1, 0, 0, 0), "kayser: a wavenumber of one per centimeter"),
    ("Oe oersted oersteds", 1000.0 / (4.0 * PI), 0.0, d(-1, 0, 0, 1), "oersted: the CGS unit of magnetic field strength H"),
    ("debye debyes", 3.33564e-30, 0.0, d(1, 0, 1, 1), "debye: electric dipole moment of molecules"),
    ("roentgen roentgens", 2.58e-4, 0.0, d(0, -1, 1, 1), "roentgen: X-ray exposure, charge freed per kilogram of air; R is the gas constant"),
];
