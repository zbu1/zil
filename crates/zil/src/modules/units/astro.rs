//! Astronomical units: distances, masses and times on the scale of the solar system and beyond.

use super::Row;
use super::dims::*;
use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "astro",
    about: "astronomy units like light-years, parsecs and solar masses",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Units for astronomy, from the astronomical unit and light-year to the parsec, and the mass of the Earth, \
Jupiter and the Sun."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("astro", &[
            ("a light-year in astronomical units", "1 ly to au"),
            ("the Sun in Earth masses", "1 solar_mass to earth_mass"),
            ("light from the Sun", "1 au / 1 light_second"),
        ]),
    ],
    units: TABLE,
    ..Module::EMPTY
};

/// Astronomical rows, in the same shape as the metric table.
#[rustfmt::skip]
pub const TABLE: &[Row] = &[
    ("au", 1.495978707e11, 0.0, LEN, "astronomical unit: about the mean distance from Earth to the Sun"),
    ("ly lightyear lightyears", 9.4607304725808e15, 0.0, LEN, "light-year: how far light travels in a Julian year"),
    ("pc parsec parsecs", 3.085677581491367e16, 0.0, LEN, "parsec: the distance at which 1 au spans one arcsecond, about 3.26 light-years"),
    ("light_second light_seconds", 299792458.0, 0.0, LEN, "light-second: how far light travels in a second"),
    ("light_minute light_minutes", 299792458.0 * 60.0, 0.0, LEN, "light-minute: how far light travels in a minute; the Sun is about 8.3 away"),
    ("light_hour light_hours", 299792458.0 * 3600.0, 0.0, LEN, "light-hour: how far light travels in an hour"),
    ("earth_mass earth_masses", 5.9722e24, 0.0, MASS, "Earth mass: the mass of the Earth"),
    ("jupiter_mass jupiter_masses", 1.89813e27, 0.0, MASS, "Jupiter mass: about 318 Earth masses"),
    ("solar_mass solar_masses", 1.98847e30, 0.0, MASS, "solar mass: the mass of the Sun, about 333000 Earth masses"),
    ("yr_julian", 31557600.0, 0.0, TIME, "Julian year: exactly 365.25 days, the year a light-year is built on"),
    ("yr_tropical", 31556925.216, 0.0, TIME, "tropical year: equinox to equinox, about 365.24219 days"),
    ("foe foes", 1e44, 0.0, d(2, 1, -2, 0), "foe: 10^44 J, the energy of a supernova"),
    ("solar_luminosity", 3.828e26, 0.0, d(2, 1, -3, 0), "solar luminosity: the power the Sun gives off"),
];
