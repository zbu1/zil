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
The constants' values are from CODATA 2022."),
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
            ("fine-structure constant", "alpha_fs"),
            ("Bohr radius", "a0"),
            ("Rydberg constant", "R_inf"),
            ("classical electron radius", "r_e"),
            ("Thomson cross section", "sigma_T"),
            ("neutron mass", "m_n"),
            ("muon mass", "m_mu"),
            ("tau mass", "m_tau"),
            ("deuteron mass", "m_d"),
            ("alpha particle mass", "m_alpha"),
            ("atomic mass unit", "m_u"),
            ("W boson mass", "m_W"),
            ("Z boson mass", "m_Z"),
            ("Higgs boson mass", "m_H"),
            ("Coulomb constant", "k_e"),
            ("Faraday constant", "faraday"),
            ("impedance of free space", "Z0"),
            ("Bohr magneton", "mu_B"),
            ("nuclear magneton", "mu_N"),
            ("magnetic flux quantum", "Phi0"),
            ("Josephson constant", "K_J"),
            ("von Klitzing constant", "R_K"),
            ("conductance quantum", "cond_quantum"),
            ("Wien displacement constant", "b_wien"),
            ("first radiation constant", "c1_rad"),
            ("second radiation constant", "c2_rad"),
            ("molar volume of an ideal gas at 0 °C and 1 atm", "V_m"),
            ("Loschmidt constant", "n0"),
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
        ("hbar", H / std::f64::consts::TAU, "J s"),
        ("k_B", 1.380649e-23, "J K^-1"),
        ("N_A", 6.02214076e23, "mol^-1"),
        ("R", 8.314462618, "J mol^-1 K^-1"),
        ("e_charge", 1.602176634e-19, "coulomb"),
        ("m_e", 9.1093837139e-31, "kg"),
        ("m_p", 1.67262192595e-27, "kg"),
        ("g0", 9.80665, "m s^-2"),
        ("eps0", 8.8541878188e-12, "farad m^-1"),
        ("mu0", 1.25663706127e-6, "N A^-2"),
        ("sigma", 5.670374419e-8, "W m^-2 K^-4"),
        ("a0", 5.29177210544e-11, "m"),
        ("R_inf", 10973731.568157, "m^-1"),
        ("r_e", 2.8179403205e-15, "m"),
        ("sigma_T", 6.6524587051e-29, "m^2"),
        ("m_n", 1.67492750056e-27, "kg"),
        ("m_mu", 1.883531627e-28, "kg"),
        ("m_tau", 3.16754e-27, "kg"),
        ("m_u", 1.66053906892e-27, "kg"),
        ("m_d", 3.3435837768e-27, "kg"),
        ("m_alpha", 6.6446573450e-27, "kg"),
        ("m_W", 80.3692 * GEV, "kg"),
        ("m_Z", 91.1880 * GEV, "kg"),
        ("m_H", 125.20 * GEV, "kg"),
        ("k_e", 8.9875517862e9, "N m^2 C^-2"),
        ("faraday", 6.02214076e23 * 1.602176634e-19, "coulomb mol^-1"),
        ("Z0", 376.730313412, "ohm"),
        ("mu_B", 9.2740100657e-24, "J T^-1"),
        ("mu_N", 5.0507837393e-27, "J T^-1"),
        ("Phi0", H / (2.0 * E), "Wb"),
        ("K_J", 2.0 * E / H, "Hz V^-1"),
        ("R_K", H / (E * E), "ohm"),
        ("cond_quantum", 2.0 * E * E / H, "S"),
        ("b_wien", 2.897771955e-3, "m K"),
        ("c1_rad", std::f64::consts::TAU * H * C * C, "W m^2"),
        ("c2_rad", H * C / 1.380649e-23, "m K"),
        ("V_m", 22.41396954e-3, "m^3 mol^-1"),
        ("n0", 2.686780111e25, "m^-3"),
    ],
    consts: &[("alpha_fs", 7.2973525643e-3)],
    ..Module::EMPTY
};

const H: f64 = 6.62607015e-34;
const E: f64 = 1.602176634e-19;
const C: f64 = 299792458.0;
/// GeV/c² in kilograms
const GEV: f64 = E * 1e9 / (C * C);

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
