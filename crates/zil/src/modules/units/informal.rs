//! Informal units: bananas for scale, smoots, fortnights, and friends. Real units that happen to be silly too.

use super::{DATA, DOSE, Dim, PER_TIME, Row, d, unit};
use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "informal",
    about: "joke and everyday units like bananas, smoots and fortnights, to give sizes a feel",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Everyday and joke units, to give a size a feel."),
        ("writing", "\
Every unit here also works with `_for_scale` added, like `banana_for_scale`, as in the internet joke."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("informal", &[
            ("height in bananas", "1.8 m to banana_for_scale"),
            ("the classic speed unit", "1 mph to furlong/fortnight"),
            ("the Harvard Bridge, famously measured in smoots", "364.4 smoot to m"),
            ("Everest's height in a fitting everyday thing", "8848 m.for_scale"),
            ("a 50-minute lecture is about a microcentury", "50 min to microcentury"),
            ("the energy in a Big Mac", "1 big_mac to kWh"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    units: TABLE,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{TABLE, unit};
    use crate::signature::{As, Qty};
    use crate::value::Value;

    /// the quantity in whichever informal unit gives the easiest number to picture
    #[builtin(examples = ["1.8 m.for_scale", "70 kg.for_scale", "2 h.for_scale"])]
    fn for_scale(q: Qty) -> Result<As<Qty>, String> {
        let (x, u) = q.get();
        let si = u.to_si(x);
        // ponytail: "relatable" = count nearest 3 on a log scale; tune the target if picks feel off.
        let best = TABLE
            .iter()
            .filter(|row| row.3 == u.dim())
            .map(|row| (row.0.split(' ').next().unwrap_or_default(), si / row.1))
            .min_by(|a, b| (a.1.abs().log10() - 0.5).abs().total_cmp(&(b.1.abs().log10() - 0.5).abs()));
        let (name, n) = best.ok_or_else(|| format!("no informal unit measures `{u}`"))?;
        Ok(Value::Qty(n, unit(name)?).into())
    }
}

const LEN: Dim = d(1, 0, 0, 0);
const MASS: Dim = d(0, 1, 0, 0);
const TIME: Dim = d(0, 0, 1, 0);
const AREA: Dim = d(2, 0, 0, 0);
const VOLUME: Dim = d(3, 0, 0, 0);
const SPEED: Dim = d(1, 0, -1, 0);
const ENERGY: Dim = d(2, 1, -2, 0);
const POWER: Dim = d(2, 1, -3, 0);
const ACCEL: Dim = d(1, 0, -2, 0);

/// Same shape as `units::TABLE`. Sizes are typical, not official: your banana may vary.
#[rustfmt::skip]
pub const TABLE: &[Row] = &[
    ("banana bananas", 0.178, 0.0, LEN, "banana: a typical banana, 17.8 cm; the internet's favorite scale"),
    ("credit_card credit_cards", 0.0856, 0.0, LEN, "credit card: the long side of an ID-1 card, 85.6 mm"),
    ("attoparsec attoparsecs", 0.030856775814913673, 0.0, LEN, "attoparsec: 10^-18 parsec, about 3.1 cm"),
    ("beard_second beard_seconds", 5e-9, 0.0, LEN, "beard-second: how far a beard grows in a second, 5 nm"),
    ("human_hair human_hairs", 7e-5, 0.0, LEN, "human hair: the width of a hair, about 70 µm"),
    ("sheet_of_paper sheets_of_paper", 1e-4, 0.0, LEN, "sheet of paper: the thickness of printer paper, 0.1 mm"),
    ("light_nanosecond light_nanoseconds", 0.299792458, 0.0, LEN, "light-nanosecond: how far light goes in 1 ns, about 30 cm; Grace Hopper's wire"),
    ("smoot smoots", 1.7018, 0.0, LEN, "smoot: Oliver Smoot's height, used to measure the Harvard Bridge in 1958"),
    ("altuve altuves", 1.65, 0.0, LEN, "altuve: the height of baseball player José Altuve, 1.65 m"),
    ("cubit cubits", 0.4572, 0.0, LEN, "cubit: elbow to fingertip, 18 inches"),
    ("hand hands", 0.1016, 0.0, LEN, "hand: 4 inches; horses are measured in hands"),
    ("step steps", 0.762, 0.0, LEN, "step: an adult walking step, about 76 cm"),
    ("giraffe giraffes", 5.5, 0.0, LEN, "giraffe: a tall adult giraffe, 5.5 m"),
    ("school_bus school_buses", 12.2, 0.0, LEN, "school bus: a full-size US school bus, about 12 m"),
    ("blue_whale blue_whales", 25.0, 0.0, LEN, "blue whale: a large adult, 25 m"),
    ("football_field football_fields", 91.44, 0.0, LEN, "football field: an American football field, 100 yards between goal lines"),
    ("city_block city_blocks", 80.0, 0.0, LEN, "city block: a Manhattan north-south block, about 80 m"),
    ("statue_of_liberty", 93.0, 0.0, LEN, "Statue of Liberty: ground to torch, pedestal included, 93 m"),
    ("furlong furlongs", 201.168, 0.0, LEN, "furlong: 1/8 mile, 220 yards; horse racing"),
    ("eiffel_tower eiffel_towers", 330.0, 0.0, LEN, "Eiffel Tower: 330 m to the tip"),
    ("sheppey sheppeys", 1408.176, 0.0, LEN, "sheppey: the closest distance at which sheep still look picturesque, 7/8 mile"),
    ("league leagues", 4828.032, 0.0, LEN, "league: 3 miles, about an hour's walk"),
    ("everest everests", 8848.86, 0.0, LEN, "Everest: Mount Everest's height above sea level, 8848.86 m"),
    ("marathon marathons", 42195.0, 0.0, LEN, "marathon: 42.195 km"),

    ("grain_of_rice grains_of_rice", 2.5e-5, 0.0, MASS, "grain of rice: a grain of long-grain rice, about 25 mg"),
    ("paperclip paperclips", 0.001, 0.0, MASS, "paperclip: a standard paperclip, about 1 g"),
    ("us_penny us_pennies", 0.0025, 0.0, MASS, "US penny: 2.5 g since 1982; handy for checking a kitchen scale"),
    ("bowling_ball bowling_balls", 7.26, 0.0, MASS, "bowling ball: the heaviest regulation ball, 16 lb"),
    ("corgi corgis", 12.0, 0.0, MASS, "corgi: a typical adult corgi, 12 kg"),
    ("slug slugs", 14.593903, 0.0, MASS, "slug: the imperial mass that 1 lbf accelerates by 1 ft/s^2"),
    ("firkin firkins", 40.8233133, 0.0, MASS, "firkin: 90 pounds, from the furlong/firkin/fortnight joke system"),
    ("grand_piano grand_pianos", 480.0, 0.0, MASS, "grand piano: a concert grand, about 480 kg"),
    ("honda_civic honda_civics", 1300.0, 0.0, MASS, "Honda Civic: a typical curb weight, 1300 kg"),
    ("elephant elephants", 6000.0, 0.0, MASS, "elephant: an adult African elephant, 6 tonnes"),
    ("boeing_747 boeing_747s jumbo_jet", 396890.0, 0.0, MASS, "Boeing 747: a 747-400 at maximum takeoff weight, 396.89 t"),

    ("planck_time planck_times", 5.391247e-44, 0.0, TIME, "Planck time: about 5.39e-44 s, the shortest time physics can talk about"),
    ("shake shakes", 1e-8, 0.0, TIME, "shake: 10 nanoseconds, from nuclear physics (\"two shakes\")"),
    ("jiffy jiffies", 1.0 / 60.0, 0.0, TIME, "jiffy: 1/60 s, one AC power cycle in North America"),
    ("blink blinks", 0.3, 0.0, TIME, "blink: a blink of an eye, about 0.3 s"),
    ("nanocentury nanocenturies", 3.15576, 0.0, TIME, "nanocentury: about π seconds"),
    ("moment moments", 90.0, 0.0, TIME, "moment: a medieval unit, 1/40 of an hour or 90 seconds"),
    ("sol sols", 88775.244, 0.0, TIME, "sol: a Mars solar day, 24 h 39 min 35 s"),
    ("microcentury microcenturies", 3155.76, 0.0, TIME, "microcentury: about 52.6 minutes, the length of a lecture"),
    ("scaramucci scaramuccis", 11.0 * 86400.0, 0.0, TIME, "scaramucci: 11 days, Anthony Scaramucci's time as White House communications director"),
    ("fortnight fortnights", 14.0 * 86400.0, 0.0, TIME, "fortnight: 14 days"),
    ("dog_year dog_years", 31556952.0 / 7.0, 0.0, TIME, "dog year: 1/7 of a year"),
    ("friedman friedmans", 31556952.0 / 2.0, 0.0, TIME, "friedman: six months, the \"next six months\" that are always promised"),

    ("barn barns", 1e-28, 0.0, AREA, "barn: 10^-28 m^2, a nuclear cross-section (\"as big as a barn\")"),
    ("parking_space parking_spaces", 12.5, 0.0, AREA, "parking space: about 2.5 m by 5 m"),
    ("tennis_court tennis_courts", 260.87, 0.0, AREA, "tennis court: a doubles court, 23.77 m by 10.97 m"),
    ("rhode_island", 4.001e9, 0.0, AREA, "Rhode Island: the area of the smallest US state, about 4001 km^2"),
    ("wales", 2.0779e10, 0.0, AREA, "Wales: the area of Wales, about 20779 km^2"),

    ("shot shots", 4.436e-5, 0.0, VOLUME, "shot: a US shot, 1.5 fl oz or about 44 mL"),
    ("can cans", 3.549e-4, 0.0, VOLUME, "can: a US soda can, 12 fl oz or about 355 mL"),
    ("wine_bottle wine_bottles", 7.5e-4, 0.0, VOLUME, "wine bottle: a standard bottle, 750 mL"),
    ("barrel barrels bbl", 0.158987294928, 0.0, VOLUME, "barrel: an oil barrel, 42 US gallons"),
    ("hogshead hogsheads", 0.238480942392, 0.0, VOLUME, "hogshead: a large cask, 63 US gallons"),
    ("bathtub bathtubs", 0.3, 0.0, VOLUME, "bathtub: a full bath, about 300 L"),
    ("olympic_pool olympic_pools", 2500.0, 0.0, VOLUME, "Olympic pool: 50 m by 25 m by 2 m, 2500 m^3"),

    ("snail snails", 0.001, 0.0, SPEED, "snail: a garden snail's pace, 1 mm/s"),
    ("walking_pace", 1.4, 0.0, SPEED, "walking pace: a typical adult walk, 1.4 m/s or about 5 km/h"),
    ("bike_pace", 16.0 / 3.6, 0.0, SPEED, "bike pace: a relaxed city bike ride, 16 km/h"),
    ("mach", 343.0, 0.0, SPEED, "mach: the speed of sound in air at 20 °C, 343 m/s"),
    ("lightspeed", 299792458.0, 0.0, SPEED, "lightspeed: the speed of light, 299792458 m/s"),

    ("aa_battery aa_batteries", 2.7 * 3600.0, 0.0, ENERGY, "AA battery: an alkaline AA, about 2.7 Wh"),
    ("banana_cal", 105.0 * 4184.0, 0.0, ENERGY, "banana (food): the food energy of a medium banana, 105 kcal"),
    ("big_mac big_macs", 563.0 * 4184.0, 0.0, ENERGY, "Big Mac: the food energy of one Big Mac, 563 kcal"),
    ("gallon_gas gallons_gas", 33.7 * 3.6e6, 0.0, ENERGY, "gallon of gas: 33.7 kWh, the EPA figure behind MPGe"),
    ("tnt ton_tnt", 4.184e9, 0.0, ENERGY, "ton of TNT: 4.184 GJ, for explosive yields"),
    ("hiroshima hiroshimas", 6.3e13, 0.0, ENERGY, "Hiroshima: the Hiroshima bomb's yield, about 15 kilotons of TNT"),

    ("human_resting", 100.0, 0.0, POWER, "resting human: the heat a person gives off at rest, about 100 W"),
    ("donkeypower", 250.0, 0.0, POWER, "donkeypower: about 250 W, a third of a horsepower"),
    ("toaster toasters", 1100.0, 0.0, POWER, "toaster: a typical toaster, 1100 W"),
    
    ("heartbeat heartbeats", 1.2, 0.0, PER_TIME, "heartbeat: a resting heart rate, 72 beats per minute"),
    ("hummingbird_wingbeat hummingbird_wingbeats", 50.0, 0.0, PER_TIME, "hummingbird wingbeat: about 50 beats a second"),
    
    ("gee gees g_force", 9.80665, 0.0, ACCEL, "gee: standard gravity, 9.80665 m/s^2; g is the gram, g0 the constant"),
    
    ("bed banana_dose", 1e-7, 0.0, DOSE, "banana equivalent dose: the radiation from eating one banana, 0.1 µSv"),

    ("nibble nibbles", 4.0, 0.0, DATA, "nibble: 4 bits, one hex digit"),
    ("tweet tweets", 280.0 * 8.0, 0.0, DATA, "tweet: 280 characters of plain text"),
    ("paragraph paragraphs", 600.0 * 8.0, 0.0, DATA, "paragraph: about 100 words of plain text, 600 bytes"),
    ("novel novels", 5e5 * 8.0, 0.0, DATA, "novel: a 90,000-word novel as plain text, about 500 KB"),
    ("floppy floppies", 1474560.0 * 8.0, 0.0, DATA, "floppy: a 3.5-inch HD floppy disk, 1.44 MB (1474560 bytes)"),
    ("photo photos", 3e6 * 8.0, 0.0, DATA, "photo: a phone photo, about 3 MB"),
    ("cdrom cdroms", 700.0 * 1048576.0 * 8.0, 0.0, DATA, "CD-ROM: a 700 MiB CD"),
    ("human_genome human_genomes", 3.1e9 * 2.0, 0.0, DATA, "human genome: 3.1 billion base pairs at 2 bits each, about 775 MB"),
    ("dvd dvds", 4.7e9 * 8.0, 0.0, DATA, "DVD: a single-layer DVD, 4.7 GB"),
    ("hour_of_hd_video hours_of_hd_video", 3e9 * 8.0, 0.0, DATA, "hour of HD video: streamed 1080p, about 3 GB"),
    ("library_of_congress", 1e13 * 8.0, 0.0, DATA, "Library of Congress: a common estimate of its printed collection, 10 TB"),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn informal() {
        assert_eq!(show("1.78 m to banana_for_scale"), "10 banana");
        assert_eq!(show("2 banana to cm"), "35.6 cm");
        assert_eq!(show("1 fortnight to d"), "14 d");
        assert_eq!(show("1 m.for_scale"), "3.33564 light_nanosecond");
        assert_eq!(show("70 kg.for_scale"), "4.79652 slug");
        assert_eq!(show("1 furlong/fortnight to mm/min"), "9.97857 mm/min");
        assert_eq!(show("1 gallon_gas to kWh"), "33.7 kWh");
        assert_eq!(show("1 mSv to bed"), "10000 bed");
        assert_eq!(show("3 gee * 70 kg to N"), "2059.4 N");
        assert_eq!(show("1 kJ / 1 kg to m^2/s^2"), "1000 m^2/s^2");
    }
}
