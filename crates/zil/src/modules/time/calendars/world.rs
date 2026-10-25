//! Other calendars: Hebrew, Islamic, Persian, Julian, Coptic, Ethiopian, Indian national, and Julian day numbers.
//! Conversions go through fixed day numbers (RD, day 1 = 0001-01-01 Gregorian), after Reingold & Dershowitz's
//! "Calendrical Calculations"; Persian uses Borkowski's 33-year breaks, which match the astronomical calendar.

use crate::modules::Module;
use jiff::Span;
use jiff::civil::Date;

pub const MODULE: Module = Module {
    name: "world",
    about: "dates in other calendars: Hebrew, Islamic, Persian, Julian, Coptic, Ethiopian, Indian, and Julian day numbers",
    #[rustfmt::skip]
    intro: &[
        ("", "\
A date converts to another calendar with `to`, like `today to hebrew`, and `date(y, m, d, \"hebrew\")` makes one from \
it. Months are numbered as in each calendar, so in the Hebrew one Nisan is 1 and Tishrei 7."),
        ("good to know", "\
The Hebrew and Islamic days really start at sunset, but these go by the date's midnight-to-midnight day. The Islamic \
calendar here is the arithmetic (tabular) one, which can be a day off from the one set by sighting the moon."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("calendars", &[
            ("Rosh Hashanah 2026 in the Hebrew calendar", r#"date("2026-09-12") to hebrew"#),
            ("today in every calendar", "today to calendars"),
            ("Ramadan 1448 starts", r#"date(1448, 9, 1, "islamic")"#),
            ("the Julian day number astronomers use", r#"date("2000-01-01T12:00Z") to jd"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    #[rustfmt::skip]
    targets: &[
        ("hebrew", "hebrew"), ("islamic", "islamic"), ("persian", "persian"), ("julian", "julian"), ("coptic", "coptic"),
        ("ethiopian", "ethiopian"), ("indian", "indian"), ("jd", "jd"), ("mjd", "mjd"), ("calendars", "calendars_of"),
    ],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{CALENDARS, show};
    use crate::value::Value;
    use indexmap::IndexMap;
    use jiff::Zoned;

    /// the date in the Hebrew calendar, with months counted from Nisan, so Tishrei is 7
    #[builtin(examples = [r#"date("2026-09-12").hebrew"#, "today to hebrew"], see = ["calendars_of"])]
    fn hebrew(d: &Zoned) -> Result<String, String> {
        show("hebrew", d)
    }

    /// the date in the Islamic (Hijri) calendar, by the arithmetic rules
    #[builtin(examples = ["today to islamic"], see = ["calendars_of"])]
    fn islamic(d: &Zoned) -> Result<String, String> {
        show("islamic", d)
    }

    /// the date in the Persian (Solar Hijri) calendar used in Iran and Afghanistan
    #[builtin(examples = ["today to persian", r#"date("2026-03-21") to persian"#], see = ["calendars_of"])]
    fn persian(d: &Zoned) -> Result<String, String> {
        show("persian", d)
    }

    /// the date in the Julian calendar, which Orthodox churches still use and which runs 13 days behind
    #[builtin(examples = ["today to julian"], see = ["calendars_of"])]
    fn julian(d: &Zoned) -> Result<String, String> {
        show("julian", d)
    }

    /// the date in the Coptic calendar of Egypt
    #[builtin(examples = ["today to coptic"], see = ["calendars_of"])]
    fn coptic(d: &Zoned) -> Result<String, String> {
        show("coptic", d)
    }

    /// the date in the Ethiopian calendar
    #[builtin(examples = ["today to ethiopian"], see = ["calendars_of"])]
    fn ethiopian(d: &Zoned) -> Result<String, String> {
        show("ethiopian", d)
    }

    /// the date in the Indian national (Saka) calendar
    #[builtin(examples = ["today to indian"], see = ["calendars_of"])]
    fn indian(d: &Zoned) -> Result<String, String> {
        show("indian", d)
    }

    /// the Julian day number: days since noon UTC on 1 January 4713 BC (Julian), with the time of day as a fraction
    #[builtin(examples = [r#"date("2000-01-01T12:00Z") to jd"#, "now to jd"], see = ["mjd"])]
    fn jd(d: &Zoned) -> f64 {
        d.timestamp().as_millisecond() as f64 / 86_400_000.0 + 2_440_587.5
    }

    /// the modified Julian day number, which is the Julian day minus 2400000.5, so days start at midnight
    #[builtin(examples = ["now to mjd"], see = ["jd"])]
    fn mjd(d: &Zoned) -> f64 {
        d.timestamp().as_millisecond() as f64 / 86_400_000.0 + 40_587.0
    }

    /// the date in every calendar zil knows
    ///
    /// Also written `to calendars`
    #[builtin(examples = ["today to calendars", r#"date("2026-09-12").calendars_of.hebrew"#], see = ["hebrew", "islamic", "persian"])]
    fn calendars_of(d: &Zoned) -> Result<IndexMap<String, Value>, String> {
        let mut m = IndexMap::from([("gregorian".to_string(), Value::str(d.strftime("%-d %B %Y").to_string()))]);
        for (name, ..) in CALENDARS {
            m.insert(name.to_string(), Value::str(show(name, d)?));
        }
        m.insert("jd".into(), Value::Float(jd(d)));
        Ok(m)
    }
}

type Ymd = (i64, i64, i64);

/// A calendar's name, month names, and conversions from and to fixed day numbers.
type Calendar = (&'static str, &'static [&'static str], fn(i64) -> Result<Ymd, String>, fn(Ymd) -> Result<i64, String>);

const CALENDARS: [Calendar; 7] = [
    ("julian", &GREGORIAN_MONTHS, |d| Ok(julian_from_fixed(d)), |(y, m, d)| Ok(fixed_from_julian(y, m, d))),
    ("hebrew", &HEBREW_MONTHS, |d| Ok(hebrew_from_fixed(d)), |(y, m, d)| Ok(fixed_from_hebrew(y, m, d))),
    ("islamic", &ISLAMIC_MONTHS, |d| Ok(islamic_from_fixed(d)), |(y, m, d)| Ok(fixed_from_islamic(y, m, d))),
    ("persian", &PERSIAN_MONTHS, persian_from_fixed, |(y, m, d)| fixed_from_persian(y, m, d)),
    ("coptic", &COPTIC_MONTHS, |d| Ok(coptic_from_fixed(d, COPTIC_EPOCH)), |(y, m, d)| Ok(fixed_from_coptic(y, m, d, COPTIC_EPOCH))),
    ("ethiopian", &ETHIOPIAN_MONTHS, |d| Ok(coptic_from_fixed(d, ETHIOPIC_EPOCH)), |(y, m, d)| Ok(fixed_from_coptic(y, m, d, ETHIOPIC_EPOCH))),
    ("indian", &INDIAN_MONTHS, |d| Ok(indian_from_fixed(d)), |(y, m, d)| Ok(fixed_from_indian(y, m, d))),
];

const GREGORIAN_MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const HEBREW_MONTHS: [&str; 13] = ["Nisan", "Iyyar", "Sivan", "Tammuz", "Av", "Elul", "Tishrei", "Heshvan", "Kislev", "Tevet", "Shevat", "Adar", "Adar II"];
#[rustfmt::skip]
const ISLAMIC_MONTHS: [&str; 12] = [
    "Muharram", "Safar", "Rabi al-Awwal", "Rabi al-Thani", "Jumada al-Awwal", "Jumada al-Thani", "Rajab", "Shaban", "Ramadan", "Shawwal",
    "Dhu al-Qadah", "Dhu al-Hijjah",
];
const PERSIAN_MONTHS: [&str; 12] = ["Farvardin", "Ordibehesht", "Khordad", "Tir", "Mordad", "Shahrivar", "Mehr", "Aban", "Azar", "Dey", "Bahman", "Esfand"];
#[rustfmt::skip]
const COPTIC_MONTHS: [&str; 13] = [
    "Thout", "Paopi", "Hathor", "Koiak", "Tobi", "Meshir", "Paremhat", "Parmouti", "Pashons", "Paoni", "Epip", "Mesori", "Pi Kogi Enavot",
];
const ETHIOPIAN_MONTHS: [&str; 13] =
    ["Meskerem", "Tikimt", "Hidar", "Tahsas", "Tir", "Yekatit", "Megabit", "Miyazya", "Ginbot", "Sene", "Hamle", "Nehase", "Pagume"];
const INDIAN_MONTHS: [&str; 12] =
    ["Chaitra", "Vaishakha", "Jyeshtha", "Ashadha", "Shravana", "Bhadra", "Ashvin", "Kartika", "Agrahayana", "Pausha", "Magha", "Phalguna"];

fn calendar(name: &str) -> Result<&'static Calendar, String> {
    CALENDARS.iter().find(|c| c.0 == name).ok_or_else(|| {
        let names: Vec<_> = CALENDARS.iter().map(|c| c.0).collect();
        format!("unknown calendar {name:?}{}", crate::error::did_you_mean(name, names.into_iter()))
    })
}

/// 1970-01-01 as a fixed day number.
const UNIX_EPOCH: i64 = 719_163;

fn fixed(d: Date) -> i64 {
    d.since(Date::constant(1970, 1, 1)).expect("dates are in range").get_days() as i64 + UNIX_EPOCH
}

fn date(rd: i64) -> Result<Date, String> {
    Date::constant(1970, 1, 1).checked_add(Span::new().try_days(rd - UNIX_EPOCH).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// `d` as (year, month, day) in `calendar`.
pub fn from_date(calendar_name: &str, d: Date) -> Result<Ymd, String> {
    (calendar(calendar_name)?.2)(fixed(d))
}

/// "1 Tishrei 5787".
fn show(calendar_name: &str, d: &jiff::Zoned) -> Result<String, String> {
    let (y, m, day) = from_date(calendar_name, d.date())?;
    let months = calendar(calendar_name)?.1;
    // In a common Hebrew year, month 12 is plain Adar; in a leap year it's Adar I.
    let month = if calendar_name == "hebrew" && m == 12 && hebrew_leap_year(y) { "Adar I" } else { months[m as usize - 1] };
    Ok(format!("{day} {month} {y}"))
}

/// The Gregorian date of `y`/`m`/`d` in `calendar`, checked by converting back.
pub fn to_date(calendar_name: &str, y: i64, m: i64, d: i64) -> Result<Date, String> {
    let (_, months, from, to) = calendar(calendar_name)?;
    let bad = || format!("{calendar_name} date {y}-{m}-{d} doesn't exist");
    if !(1..=months.len() as i64).contains(&m) || !(1..=31).contains(&d) || y.abs() > 100_000 {
        return Err(bad());
    }
    let rd = to((y, m, d))?;
    if from(rd)? != (y, m, d) {
        return Err(bad());
    }
    date(rd)
}

fn leap_gregorian(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn fixed_from_julian(y: i64, m: i64, d: i64) -> i64 {
    let leap = y.rem_euclid(4) == 0;
    -2 + 365 * (y - 1)
        + (y - 1).div_euclid(4)
        + (367 * m - 362).div_euclid(12)
        + if m <= 2 {
            0
        } else if leap {
            -1
        } else {
            -2
        }
        + d
}

fn julian_from_fixed(rd: i64) -> Ymd {
    let y = (4 * (rd + 1) + 1464).div_euclid(1461);
    let leap = y.rem_euclid(4) == 0;
    let prior = rd - fixed_from_julian(y, 1, 1);
    let correction = if rd < fixed_from_julian(y, 3, 1) {
        0
    } else if leap {
        1
    } else {
        2
    };
    let m = (12 * (prior + correction) + 373).div_euclid(367);
    (y, m, rd - fixed_from_julian(y, m, 1) + 1)
}

const COPTIC_EPOCH: i64 = 103_605;
const ETHIOPIC_EPOCH: i64 = 2_796;

/// Coptic and Ethiopian: twelve 30-day months and a short 13th, differing only in the epoch.
fn fixed_from_coptic(y: i64, m: i64, d: i64, epoch: i64) -> i64 {
    epoch - 1 + 365 * (y - 1) + y.div_euclid(4) + 30 * (m - 1) + d
}

fn coptic_from_fixed(rd: i64, epoch: i64) -> Ymd {
    let y = (4 * (rd - epoch) + 1463).div_euclid(1461);
    let m = (rd - fixed_from_coptic(y, 1, 1, epoch)).div_euclid(30) + 1;
    (y, m, rd + 1 - fixed_from_coptic(y, m, 1, epoch))
}

const ISLAMIC_EPOCH: i64 = 227_015;

fn fixed_from_islamic(y: i64, m: i64, d: i64) -> i64 {
    d + 29 * (m - 1) + (6 * m - 1).div_euclid(11) + (y - 1) * 354 + (3 + 11 * y).div_euclid(30) + ISLAMIC_EPOCH - 1
}

fn islamic_from_fixed(rd: i64) -> Ymd {
    let y = (30 * (rd - ISLAMIC_EPOCH) + 10646).div_euclid(10631);
    let m = (11 * (rd - fixed_from_islamic(y, 1, 1)) + 330).div_euclid(325);
    (y, m, rd - fixed_from_islamic(y, m, 1) + 1)
}

const HEBREW_EPOCH: i64 = -1_373_427;

fn hebrew_leap_year(y: i64) -> bool {
    (7 * y + 1).rem_euclid(19) < 7
}

fn hebrew_elapsed_days(y: i64) -> i64 {
    let months = (235 * y - 234).div_euclid(19);
    let parts = 12084 + 13753 * months;
    let days = 29 * months + parts.div_euclid(25920);
    if (3 * (days + 1)).rem_euclid(7) < 3 { days + 1 } else { days }
}

fn hebrew_new_year(y: i64) -> i64 {
    let (a, b, c) = (hebrew_elapsed_days(y - 1), hebrew_elapsed_days(y), hebrew_elapsed_days(y + 1));
    let delay = if c - b == 356 {
        2
    } else if b - a == 382 {
        1
    } else {
        0
    };
    HEBREW_EPOCH + b + delay
}

fn hebrew_month_days(y: i64, m: i64) -> i64 {
    let len = hebrew_new_year(y + 1) - hebrew_new_year(y);
    let short = matches!(m, 2 | 4 | 6 | 10 | 13) || (m == 12 && !hebrew_leap_year(y)) || (m == 8 && len % 10 != 5) || (m == 9 && len % 10 == 3);
    if short { 29 } else { 30 }
}

fn fixed_from_hebrew(y: i64, m: i64, d: i64) -> i64 {
    let last = if hebrew_leap_year(y) { 13 } else { 12 };
    // The year starts in Tishrei (7); Nisan to Elul come after Tishrei to the last month.
    let before: i64 = if m < 7 { (7..=last).chain(1..m).map(|mm| hebrew_month_days(y, mm)).sum() } else { (7..m).map(|mm| hebrew_month_days(y, mm)).sum() };
    hebrew_new_year(y) + d - 1 + before
}

fn hebrew_from_fixed(rd: i64) -> Ymd {
    let approx = ((rd - HEBREW_EPOCH) as f64 * 98496.0 / 35975351.0).floor() as i64 + 1;
    let y = (approx - 1..).take_while(|&y| hebrew_new_year(y) <= rd).last().expect("the year before approx has started");
    let start = if rd < fixed_from_hebrew(y, 1, 1) { 7 } else { 1 };
    let m = (start..).find(|&m| rd <= fixed_from_hebrew(y, m, hebrew_month_days(y, m))).expect("a month holds the day");
    (y, m, rd - fixed_from_hebrew(y, m, 1) + 1)
}

/// `(leap, gregorian year, March day of Nowruz)` for Persian year `y`: Borkowski's algorithm, valid from -61 to 3177.
fn persian_year(y: i64) -> Result<(i64, i64, i64), String> {
    const BREAKS: [i64; 20] = [-61, 9, 38, 199, 426, 686, 756, 818, 1111, 1181, 1210, 1635, 2060, 2097, 2192, 2262, 2324, 2394, 2456, 3178];
    if !(BREAKS[0]..BREAKS[19]).contains(&y) {
        return Err(format!("Persian year {y} is out of range\nnote: the calendar is worked out for years -61 to 3177"));
    }
    let gy = y + 621;
    let (mut leap_j, mut jp, mut jump) = (-14, BREAKS[0], 0);
    for &jm in &BREAKS[1..] {
        jump = jm - jp;
        if y < jm {
            break;
        }
        leap_j += jump / 33 * 8 + jump % 33 / 4;
        jp = jm;
    }
    let mut n = y - jp;
    leap_j += n / 33 * 8 + (n % 33 + 3) / 4;
    if jump % 33 == 4 && jump - n == 4 {
        leap_j += 1;
    }
    let leap_g = gy / 4 - (gy / 100 + 1) * 3 / 4 - 150;
    let march = 20 + leap_j - leap_g;
    if jump - n < 6 {
        n = n - jump + (jump + 4) / 33 * 33;
    }
    let leap = ((n + 1) % 33 - 1) % 4;
    Ok((if leap == -1 { 4 } else { leap }, gy, march))
}

fn fixed_from_gregorian(y: i64, m: i64, d: i64) -> i64 {
    365 * (y - 1) + (y - 1).div_euclid(4) - (y - 1).div_euclid(100)
        + (y - 1).div_euclid(400)
        + (367 * m - 362).div_euclid(12)
        + if m <= 2 {
            0
        } else if leap_gregorian(y) {
            -1
        } else {
            -2
        }
        + d
}

fn fixed_from_persian(y: i64, m: i64, d: i64) -> Result<i64, String> {
    let (_, gy, march) = persian_year(y)?;
    Ok(fixed_from_gregorian(gy, 3, march) + (m - 1) * 31 - m / 7 * (m - 7) + d - 1)
}

fn persian_from_fixed(rd: i64) -> Result<Ymd, String> {
    let gy = date(rd)?.year() as i64;
    let mut y = gy - 621;
    let (leap, _, march) = persian_year(y)?;
    let mut k = rd - fixed_from_gregorian(gy, 3, march);
    if (0..=185).contains(&k) {
        return Ok((y, 1 + k / 31, k % 31 + 1));
    }
    if k >= 0 {
        k -= 186;
    } else {
        y -= 1;
        k += 179 + i64::from(leap == 1);
    }
    Ok((y, 7 + k / 30, k % 30 + 1))
}

/// Saka years start on 22 March (21 March in Gregorian leap years), 78 years behind.
fn indian_start(sy: i64) -> i64 {
    let gy = sy + 78;
    fixed_from_gregorian(gy, 3, if leap_gregorian(gy) { 21 } else { 22 })
}

fn indian_month_days(sy: i64, m: i64) -> i64 {
    match m {
        1 if leap_gregorian(sy + 78) => 31,
        2..=6 => 31,
        _ => 30,
    }
}

fn fixed_from_indian(y: i64, m: i64, d: i64) -> i64 {
    indian_start(y) + (1..m).map(|mm| indian_month_days(y, mm)).sum::<i64>() + d - 1
}

fn indian_from_fixed(rd: i64) -> Ymd {
    let gy = date(rd).map_or(0, |d| d.year() as i64);
    let y = if rd < indian_start(gy - 78) { gy - 79 } else { gy - 78 };
    let mut k = rd - indian_start(y);
    let mut m = 1;
    while k >= indian_month_days(y, m) {
        k -= indian_month_days(y, m);
        m += 1;
    }
    (y, m, k + 1)
}

#[cfg(test)]
mod tests {
    use super::{fixed, fixed_from_gregorian, from_date, to_date};
    use crate::interp::tests::{show, try_eval};
    use jiff::civil::Date;

    #[test]
    fn fixed_days() {
        assert_eq!(fixed(Date::constant(1, 1, 1)), 1);
        assert_eq!(fixed(Date::constant(2026, 10, 25)), fixed_from_gregorian(2026, 10, 25));
    }

    #[test]
    fn known_dates() {
        for (greg, cal, ymd) in [
            ((2026, 9, 12), "hebrew", (5787, 7, 1)),
            ((2024, 4, 23), "hebrew", (5784, 1, 15)),
            ((2025, 3, 1), "islamic", (1446, 9, 1)),
            ((2025, 3, 21), "persian", (1404, 1, 1)),
            ((2024, 3, 20), "persian", (1403, 1, 1)),
            ((2026, 1, 7), "julian", (2025, 12, 25)),
            ((2025, 9, 11), "coptic", (1742, 1, 1)),
            ((2025, 9, 11), "ethiopian", (2018, 1, 1)),
            ((2026, 3, 22), "indian", (1948, 1, 1)),
            ((2024, 3, 21), "indian", (1946, 1, 1)),
        ] {
            let d = Date::constant(greg.0, greg.1, greg.2);
            assert_eq!(from_date(cal, d), Ok(ymd), "{cal} {greg:?}");
            assert_eq!(to_date(cal, ymd.0, ymd.1, ymd.2), Ok(d), "{cal} {ymd:?}");
        }
    }

    #[test]
    fn round_trips() {
        for cal in ["hebrew", "islamic", "persian", "julian", "coptic", "ethiopian", "indian"] {
            let mut prev = from_date(cal, Date::constant(1899, 12, 31)).unwrap();
            for n in 0..365 * 130 {
                let d = Date::constant(1900, 1, 1).checked_add(jiff::Span::new().days(n)).unwrap();
                let ymd = from_date(cal, d).unwrap();
                assert_ne!(ymd, prev, "{cal} {d}");
                assert_eq!(to_date(cal, ymd.0, ymd.1, ymd.2), Ok(d), "{cal} {d}");
                prev = ymd;
            }
        }
    }

    #[test]
    fn calendars() {
        assert_eq!(show(r#"date("2026-09-12") to hebrew"#), "1 Tishrei 5787");
        assert_eq!(show(r#"date("2024-03-01").hebrew"#), "21 Adar I 5784");
        assert_eq!(show(r#"date(1405, 1, 1, "persian").str[..10]"#), "2026-03-21");
        assert_eq!(show(r#"date("2000-01-01T12:00Z") to jd"#), "2451545");
        assert_eq!(show(r#"date("2000-01-01T00:00Z") to mjd"#), "51544");
        assert_eq!(show(r#"(date("2026-09-12") to calendars).keys().len"#), "9");
        assert!(try_eval(r#"date(5786, 13, 1, "hebrew")"#).unwrap_err().msg.contains("doesn't exist"));
        assert!(try_eval(r#"date(1, 1, 1, "mayan")"#).unwrap_err().msg.contains("unknown calendar"));
    }
}
