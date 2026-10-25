//! The sky: sunrise, sunset, day length and the moon's phase, from the NOAA sunrise equation.

use crate::modules::Module;
use jiff::Zoned;
use std::f64::consts::PI;

pub const MODULE: Module = Module {
    name: "sky",
    about: "sunrise, sunset and hours of daylight anywhere, and the phase of the moon",
    #[rustfmt::skip]
    intro: &[
        ("", "\
When the sun rises and sets anywhere on Earth, and the phase of the moon.\n\n\
`sunrise`, `sunset` and `day_length` take a place as a city or country (a country means its capital), as a map with \
lat and lon, or as latitude and longitude, two numbers in degrees, north and east positive. The day defaults to \
today."),
        ("good to know", "\
Times come back in the time zone of the date you pass in, which is your own zone unless you converted it. To get a \
sunset in the place's own local time, pass a date in that zone, like `date(2026, 6, 21) to \"Oslo\"`. With no date, a \
named city or country uses today in its own zone, and coordinates use today in yours. Near the poles, some days have \
no sunrise or sunset at all."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("sky", &[
            ("sunset in Paris on the longest day, in Paris time", r#"sunset(48.86, 2.35, date("2026-06-21T12:00+02:00") to "Europe/Paris")"#),
            ("the longest day in Oslo", r#"day_length(59.91, 10.75, date(2026, 6, 21)) to h min"#),
            ("no sunrise at the pole in winter", r#"sunrise(89, 0, date(2026, 12, 21))"#),
            ("the moon on Christmas 2026", r#"moon_phase(date(2026, 12, 25))"#),
        ]),
    ],
    #[rustfmt::skip]
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Sun, sun};
    use crate::modules::Fail;
    use crate::modules::geo::Place;
    use crate::modules::units::unit;
    use crate::signature::{As, Qty};
    use crate::value::Value;
    use jiff::{Timestamp, Zoned};
    use std::f64::consts::TAU;

    /// Where and when: a named place defaults to today on its own clock.
    fn at_place(place: &Place, day: Option<&Zoned>) -> Result<(f64, f64, Zoned), Fail> {
        let (lat, lon, zone) = place.coords().map_err(|m| Fail::Arg(0, m))?;
        let z = match (day, zone) {
            (Some(d), _) => d.clone(),
            (None, Some(zone)) => crate::host::now().with_time_zone(jiff::tz::db().get(zone).map_err(|e| e.to_string())?),
            (None, None) => crate::host::now(),
        };
        Ok((lat, lon, z))
    }

    fn at_coords(lat: f64, lon: f64, day: Option<&Zoned>) -> Result<(f64, f64, Zoned), Fail> {
        if !(-90.0..=90.0).contains(&lat) {
            return Err(Fail::Arg(0, "latitude must be a number from -90 to 90".into()));
        }
        if !(-180.0..=180.0).contains(&lon) {
            return Err(Fail::Arg(1, "longitude must be a number from -180 to 180".into()));
        }
        Ok((lat, lon, day.map_or_else(crate::host::now, Zoned::clone)))
    }

    fn rise_or_set(at: (f64, f64, Zoned), rise: bool) -> Result<Option<Zoned>, Fail> {
        let (lat, lon, z) = at;
        let Sun::Times(r, s) = sun(lat, lon, &z) else { return Ok(None) };
        let ts = Timestamp::from_second((if rise { r } else { s }).round() as i64).map_err(|e| e.to_string())?;
        Ok(Some(ts.to_zoned(z.time_zone().clone())))
    }

    fn length(at: (f64, f64, Zoned)) -> Result<As<Qty>, Fail> {
        let (lat, lon, z) = at;
        let h = match sun(lat, lon, &z) {
            Sun::Times(rise, set) => (set - rise) / 3600.0,
            Sun::Always(up) => {
                if up {
                    24.0
                } else {
                    0.0
                }
            }
        };
        Ok(Value::qty(h, unit("h")?).into())
    }

    /// when the sun rises at a place on a day, or nil if it doesn't rise or doesn't set that day
    #[builtin(examples = ["sunrise(40.71, -74.01, date(2026, 3, 20))", r#"sunrise("Reykjavik").hour"#], see = ["sunset", "day_length"])]
    fn sunrise(lat: f64, lon: f64, day: Option<&Zoned>) -> Result<Option<Zoned>, Fail> {
        rise_or_set(at_coords(lat, lon, day)?, true)
    }

    #[builtin(name = "sunrise")]
    fn sunrise_at(place: Place, day: Option<&Zoned>) -> Result<Option<Zoned>, Fail> {
        rise_or_set(at_place(&place, day)?, true)
    }

    /// when the sun sets at a place on a day, or nil if it doesn't rise or doesn't set that day
    #[builtin(examples = ["sunset(51.5, -0.13, date(2026, 12, 21))", r#"sunset("Oslo", date(2026, 6, 21))"#], see = ["sunrise", "day_length"])]
    fn sunset(lat: f64, lon: f64, day: Option<&Zoned>) -> Result<Option<Zoned>, Fail> {
        rise_or_set(at_coords(lat, lon, day)?, false)
    }

    #[builtin(name = "sunset")]
    fn sunset_at(place: Place, day: Option<&Zoned>) -> Result<Option<Zoned>, Fail> {
        rise_or_set(at_place(&place, day)?, false)
    }

    /// how long the sun is up at a place on a day: 0 h if it never rises, 24 h if it never sets
    #[builtin(examples = ["day_length(0, 0, date(2026, 3, 20)) to h min", "day_length(70, 25, date(2026, 6, 21))"], see = ["sunrise", "sunset"])]
    fn day_length(lat: f64, lon: f64, day: Option<&Zoned>) -> Result<As<Qty>, Fail> {
        length(at_coords(lat, lon, day)?)
    }

    #[builtin(name = "day_length")]
    fn day_length_at(place: Place, day: Option<&Zoned>) -> Result<As<Qty>, Fail> {
        length(at_place(&place, day)?)
    }

    /// the moon's phase on a day (today without one), like "full moon", and how much of it is lit
    #[builtin(examples = ["moon_phase()", "moon_phase(date(2026, 1, 3))"], see = ["sunrise"])]
    fn moon_phase(day: Option<&Zoned>) -> String {
        let z = day.map_or_else(crate::host::now, Zoned::clone);
        // Days since a known new moon (2000-01-06 18:14 UTC), in synodic months.
        let age = ((z.timestamp().as_second() as f64 - 947182440.0) / 86400.0 / 29.530588853).rem_euclid(1.0);
        let lit = (1.0 - (TAU * age).cos()) / 2.0;
        const PHASES: [&str; 8] = [
            "🌑 new moon",
            "🌒 waxing crescent",
            "🌓 first quarter",
            "🌔 waxing gibbous",
            "🌕 full moon",
            "🌖 waning gibbous",
            "🌗 last quarter",
            "🌘 waning crescent",
        ];
        format!("{}, {:.0}% lit", PHASES[(age * 8.0 + 0.5) as usize % 8], lit * 100.0)
    }
}

enum Sun {
    Times(f64, f64),
    /// Up all day (true) or down all day.
    Always(bool),
}

/// Sunrise and sunset as unix seconds for the civil date of `day`.
fn sun(lat: f64, lon: f64, day: &Zoned) -> Sun {
    let rad = PI / 180.0;
    let days = day.date().to_zoned(jiff::tz::TimeZone::UTC).map_or(day.timestamp(), |z| z.timestamp()).as_second() as f64 / 86400.0;
    let n = (days + 2440588.0 - 2451545.0 + 0.0008).round();
    let j = n - lon / 360.0;
    let m = (357.5291 + 0.98560028 * j).rem_euclid(360.0);
    let c = 1.9148 * (m * rad).sin() + 0.02 * (2.0 * m * rad).sin() + 0.0003 * (3.0 * m * rad).sin();
    let l = (m + c + 180.0 + 102.9372).rem_euclid(360.0);
    let transit = 2451545.0 + j + 0.0053 * (m * rad).sin() - 0.0069 * (2.0 * l * rad).sin();
    let decl = ((l * rad).sin() * (23.4397 * rad).sin()).asin();
    let cos_w = ((-0.833 * rad).sin() - (lat * rad).sin() * decl.sin()) / ((lat * rad).cos() * decl.cos());
    if cos_w.abs() > 1.0 {
        return Sun::Always(cos_w < -1.0);
    }
    let w = cos_w.acos() / rad / 360.0;
    let unix = |jd: f64| (jd - 2440587.5) * 86400.0;
    Sun::Times(unix(transit - w), unix(transit + w))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn sky() {
        // Solar noon in New York near the equinox is about 13:03 EDT (74° W, plus the equation of time).
        let noon = show(
            r#"d = date("2026-03-20T12:00-04:00") to "America/New_York"; rise = sunrise(40.71, -74.01, d); ((rise + (sunset(40.71, -74.01, d) - rise) / 2)).format("%H:%M")"#,
        );
        assert!(["13:02", "13:03", "13:04"].contains(&noon.as_str()), "{noon}");
        // At the equator on the equinox, refraction stretches the day to about 12 h 7 min.
        let eq = show("round(day_length(0, 0, date(\"2026-03-20T12:00Z\")) to min)");
        assert!(["726 min", "727 min", "728 min"].contains(&eq.as_str()), "{eq}");
        assert_eq!(
            show("[sunrise(89, 0, date(2026, 12, 21)), day_length(89, 0, date(2026, 12, 21)), day_length(89, 0, date(2026, 6, 21))]"),
            "[nil, 0 h, 24 h]"
        );
        assert_eq!(show("moon_phase(date(\"2026-01-03T10:03Z\"))"), "🌕 full moon, 100% lit");
        assert!(show("moon_phase(date(\"2026-01-18T19:52Z\"))").starts_with("🌑 new moon"));
        // A place without a day means today on that place's clock.
        assert_eq!(show(r#"sunrise("Tokyo").format("%Z")"#), "JST");
        assert_eq!(show(r#"round(day_length("Oslo", date(2026, 6, 21)) to h)"#), "19 h");
    }
}
