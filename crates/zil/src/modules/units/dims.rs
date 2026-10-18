//! Dimensions: exponents of the SI bases and extras, and the named kinds of unit (length, energy, ...).

/// Exponents of the 7 SI bases (m kg s A K mol cd), the non-SI extras data, money, angle, dose,
/// then slots for user base units (`unit pizza`).
pub type Dim = [i8; 16];
pub const NONE: Dim = [0; 16];
pub const FIRST_USER_BASE: usize = 11;

/// Length, mass, time, current.
pub const fn d(l: i8, m: i8, t: i8, i: i8) -> Dim {
    let mut x = NONE;
    (x[0], x[1], x[2], x[3]) = (l, m, t, i);
    x
}

/// Base `i` to the power `p`, times `rest`.
pub const fn base(i: usize, p: i8, rest: Dim) -> Dim {
    let mut x = rest;
    x[i] += p;
    x
}
pub const LEN: Dim = d(1, 0, 0, 0);
pub const MASS: Dim = d(0, 1, 0, 0);
pub const TIME: Dim = d(0, 0, 1, 0);
pub const PER_TIME: Dim = d(0, 0, -1, 0);
pub const CURRENT: Dim = d(0, 0, 0, 1);
pub const TEMP: Dim = base(4, 1, NONE);
pub const AMOUNT: Dim = base(5, 1, NONE);
pub const MOLAR: Dim = base(5, 1, d(-3, 0, 0, 0));
pub const LIGHT: Dim = base(6, 1, NONE);
pub const LUX: Dim = base(6, 1, d(-2, 0, 0, 0));
pub const DATA: Dim = base(7, 1, NONE);
pub const RATE: Dim = base(7, 1, PER_TIME);
pub const MONEY: Dim = base(8, 1, NONE);
pub const ANGLE: Dim = base(9, 1, NONE);
// Its own base, not J/kg, so specific energies don't print as sieverts.
pub const DOSE: Dim = base(10, 1, NONE);
pub const VOLT: Dim = d(2, 1, -3, -1);
pub const OHM: Dim = d(2, 1, -3, -2);
pub const COULOMB: Dim = d(0, 0, 1, 1);
pub const FARAD: Dim = d(-2, -1, 4, 2);
pub const SIEMENS: Dim = d(-2, -1, 3, 2);
pub const WEBER: Dim = d(2, 1, -2, -1);
pub const TESLA: Dim = d(0, 1, -2, -1);
pub const HENRY: Dim = d(2, 1, -2, -2);
pub const VISCOSITY: Dim = d(-1, 1, -1, 0);
pub const KINEMATIC_VISCOSITY: Dim = d(2, 0, -1, 0);
pub const FLOW: Dim = d(3, 0, -1, 0);
pub const FUEL: Dim = d(-2, 0, 0, 0);

/// Conversion targets offered first after `to`, most used first, by DIMS name; the rest follow alphabetically.
#[rustfmt::skip]
pub const COMMON: &[(&str, &[&str])] = &[
    ("length", &["m", "km", "cm", "mm", "mi", "ft", "in", "yd"]),
    ("mass", &["kg", "g", "lb", "oz", "t"]),
    ("time", &["s", "min", "h", "d", "wk", "mo", "yr", "ms"]),
    ("temperature", &["C", "F", "K"]),
    ("volume", &["L", "mL", "gal", "cup", "floz", "tbsp", "tsp", "m^3"]),
    ("area", &["m^2", "km^2", "ft^2", "acre", "ha"]),
    ("speed", &["kph", "mph", "m/s", "kn", "mach", "ft/s"]),
    ("data", &["B", "KB", "MB", "GB", "TB", "KiB", "MiB", "GiB"]),
    ("rate", &["Mbps", "Gbps", "kbps"]),
    ("energy", &["J", "kJ", "kcal", "cal", "kWh", "Wh", "eV", "BTU"]),
    ("power", &["W", "kW", "MW", "hp"]),
    ("pressure", &["Pa", "kPa", "bar", "atm", "psi", "mmHg"]),
    ("force", &["N", "kN", "lbf"]),
    ("angle", &["deg", "rad", "turn"]),
    ("current", &["A", "mA"]),
    ("voltage", &["V", "mV", "kV"]),
    ("resistance", &["Ω", "kΩ", "MΩ"]),
    ("frequency", &["Hz", "kHz", "MHz", "GHz", "rpm"]),
    ("flow", &["L/min", "gpm", "m^3/s"]),
];

/// Names of the kinds of unit in TABLE, for `help`. Each needs at least two units.
#[rustfmt::skip]
pub const DIMS: &[(&str, Dim)] = &[
    ("length", LEN), ("mass", MASS), ("time", TIME), ("temperature", TEMP),
    ("volume", d(3, 0, 0, 0)), ("area", d(2, 0, 0, 0)), ("speed", d(1, 0, -1, 0)),
    ("data", DATA), ("rate", RATE), ("energy", d(2, 1, -2, 0)), ("power", d(2, 1, -3, 0)),
    ("pressure", d(-1, 1, -2, 0)), ("force", d(1, 1, -2, 0)), ("angle", ANGLE),
    ("current", CURRENT), ("voltage", VOLT), ("resistance", OHM), ("charge", COULOMB),
    ("capacitance", FARAD), ("conductance", SIEMENS), ("magnetic_flux", WEBER), ("magnetic_field", TESLA),
    ("inductance", HENRY), ("frequency", PER_TIME), ("amount", AMOUNT), ("concentration", MOLAR),
    ("light", LIGHT), ("illuminance", LUX), ("dose", DOSE), ("viscosity", VISCOSITY),
    ("kinematic_viscosity", KINEMATIC_VISCOSITY), ("flow", FLOW), ("fuel_economy", FUEL),
    ("ratio", NONE),
];
