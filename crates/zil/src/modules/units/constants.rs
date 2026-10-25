//! Physical constants as quantities, so they carry their units through a calculation.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "constants",
    about: "physical constants with their units, like the speed of light c and gravity's G",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Physical constants, with their units, so they work in calculations with quantities."),
        ("good to know", "\
Planck's constant is `h_planck`, since `h` is hours, and the electron's charge is `e_charge`, since `e` is 2.718.... \
Assigning to a name like `c` hides the constant for the rest of the session."),
        ("data", "\
The constants' values are from CODATA 2018."),
    ],
    #[rustfmt::skip]
    guide: &[
        ("constants", &[
            ("speed of light", "c"),
            ("gravitational constant", "G"),
            ("Planck constant", "h_planck"),
            ("reduced Planck constant", "hbar"),
            ("Boltzmann constant", "k_B"),
            ("Avogadro constant", "N_A"),
            ("gas constant", "R"),
            ("elementary charge", "e_charge"),
            ("electron mass", "m_e"),
            ("proton mass", "m_p"),
            ("standard gravity", "g0"),
            ("vacuum permittivity", "eps0"),
            ("vacuum permeability", "mu0"),
            ("Stefan-Boltzmann constant", "sigma"),
        ]),
    ],
    #[rustfmt::skip]
    examples: &[
        ("constants", &[
            ("the energy in 1 kg of mass, E = mc²", "1 kg * c**2 to J"),
            ("how long sunlight takes to reach Earth", "1 au / c to min"),
            ("photon energy of green light", "h_planck * c / 530 nm to eV"),
            ("the weight of 70 kg", "70 kg * g0"),
        ]),
    ],
    #[rustfmt::skip]
    quantities: &[
        ("c", 299792458.0, "m s^-1"),
        ("G", 6.67430e-11, "m^3 kg^-1 s^-2"),
        ("h_planck", 6.62607015e-34, "J s"),
        ("hbar", 6.62607015e-34 / std::f64::consts::TAU, "J s"),
        ("k_B", 1.380649e-23, "J K^-1"),
        ("N_A", 6.02214076e23, "mol^-1"),
        ("R", 8.314462618, "J mol^-1 K^-1"),
        ("e_charge", 1.602176634e-19, "coulomb"),
        ("m_e", 9.1093837015e-31, "kg"),
        ("m_p", 1.67262192369e-27, "kg"),
        ("g0", 9.80665, "m s^-2"),
        ("eps0", 8.8541878128e-12, "farad m^-1"),
        ("mu0", 1.25663706212e-6, "N A^-2"),
        ("sigma", 5.670374419e-8, "W m^-2 K^-4"),
    ],
    ..Module::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn constants() {
        assert_eq!(show("1 kg * c**2 to J"), "8.98755e16 J");
        assert_eq!(show("c"), "299792458 m/s");
        assert_eq!(show("1 h to min"), "60 min");
        assert_eq!(show("c = 3\nc"), "3");
        assert_eq!(show("70 kg * g0"), "686.465 N");
    }
}
