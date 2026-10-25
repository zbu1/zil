//! The sky: sunrise, sunset, day length and the moon's phase, from the NOAA sunrise equation.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::units::unit;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::{Value, num};
use jiff::{Timestamp, Zoned};
use std::f64::consts::{PI, TAU};

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
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("sunrise", "sunrise(lat: num, lon: num, day?: date) -> date|nil / sunrise(place: str|map, day?: date) -> date|nil", "when the sun rises at a place on a day, or nil if it doesn't rise or doesn't set that day", &["sunrise(40.71, -74.01, date(2026, 3, 20))", r#"sunrise("Reykjavik").hour"#], &["sunset", "day_length"]),
    doc("sunset", "sunset(lat: num, lon: num, day?: date) -> date|nil / sunset(place: str|map, day?: date) -> date|nil", "when the sun sets at a place on a day, or nil if it doesn't rise or doesn't set that day", &["sunset(51.5, -0.13, date(2026, 12, 21))", r#"sunset("Oslo", date(2026, 6, 21))"#], &["sunrise", "day_length"]),
    doc("day_length", "day_length(lat: num, lon: num, day?: date) -> quantity / day_length(place: str|map, day?: date) -> quantity", "how long the sun is up at a place on a day: 0 h if it never rises, 24 h if it never sets", &["day_length(0, 0, date(2026, 3, 20)) to h min", "day_length(70, 25, date(2026, 6, 21))"], &["sunrise", "sunset"]),
    doc("moon_phase", "moon_phase(day?: date) -> str", "the moon's phase on a day (today by default), like \"full moon\", and how much of it is lit", &["moon_phase()", "moon_phase(date(2026, 1, 3))"], &["sunrise"]),
];

enum Sun {
    Times(f64, f64),
    /// Up all day (true) or down all day.
    Always(bool),
}

/// Sunrise and sunset as unix seconds for the civil date of `day`.
fn sun(lat: f64, lon: f64, day: &Zoned) -> Sun {
    let rad = PI / 180.0;
    let days = day.date().to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp().as_second() as f64 / 86400.0;
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

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let day = |v: Option<&Value>| match v {
        None => Ok(crate::host::now()),
        Some(Date(z)) => Ok((**z).clone()),
        Some(_) => Err(Fail::BadArgs),
    };
    Ok(match (name, args) {
        ("sunrise" | "sunset" | "day_length", [first, rest @ ..]) if rest.len() <= 2 => {
            let (lat, lon, z) = match (first, rest) {
                // A named place defaults to today on its own clock.
                (Str(_) | Map(_), [] | [Date(_)]) => {
                    let (lat, lon, zone) = crate::modules::geo::place(first).map_err(|m| Fail::Arg(0, m))?;
                    let z = match (rest.first(), zone) {
                        (None, Some(zone)) => crate::host::now().with_time_zone(jiff::tz::db().get(zone).map_err(|e| e.to_string())?),
                        (d, _) => day(d)?,
                    };
                    (lat, lon, z)
                }
                (lat, [lon, rest @ ..]) if rest.len() <= 1 => {
                    let lat = num(lat).filter(|l| (-90.0..=90.0).contains(l)).ok_or(Fail::Arg(0, "latitude must be a number from -90 to 90".into()))?;
                    let lon = num(lon).filter(|l| (-180.0..=180.0).contains(l)).ok_or(Fail::Arg(1, "longitude must be a number from -180 to 180".into()))?;
                    (lat, lon, day(rest.first())?)
                }
                _ => return Err(Fail::BadArgs),
            };
            let at = |secs: f64| -> Result<Value, Fail> {
                let ts = Timestamp::from_second(secs.round() as i64).map_err(|e| e.to_string())?;
                Ok(Value::date(ts.to_zoned(z.time_zone().clone())))
            };
            match (name, sun(lat, lon, &z)) {
                ("sunrise", Sun::Times(rise, _)) => at(rise)?,
                ("sunset", Sun::Times(_, set)) => at(set)?,
                ("day_length", Sun::Times(rise, set)) => Value::qty((set - rise) / 3600.0, unit("h")?),
                ("day_length", Sun::Always(up)) => Value::qty(if up { 24.0 } else { 0.0 }, unit("h")?),
                _ => Nil,
            }
        }
        ("moon_phase", [] | [Date(_)]) => {
            let z = day(args.first())?;
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
            Value::str(format!("{}, {:.0}% lit", PHASES[(age * 8.0 + 0.5) as usize % 8], lit * 100.0))
        }
        _ => return Err(Fail::BadArgs),
    })
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
