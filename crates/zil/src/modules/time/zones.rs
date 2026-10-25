//! Time zones: the same moment on another clock. Zones are IANA names, or city or country names (`"Tokyo"`, `"Seattle"`, `"India"`).

use super::msg;
use crate::ast::Target;
use crate::modules::{Claim, Module};
use crate::value::Value;
use jiff::tz::TimeZone;

pub const MODULE: Module = Module {
    name: "zones",
    about: "the same moment in other time zones, and the time now around the world",
    #[rustfmt::skip]
    intro: &[
        ("", "\
The same moment as clocks read it in other time zones.\n\n\
To show a date in another zone, write `d to zone`, where zone is `UTC`, `local` (your computer's zone), a city or \
country name, or a name from the standard time zone list like \"Asia/Tokyo\" or \"America/New_York\"."),
        ("good to know", "\
Zones follow daylight saving time on their own."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("zones", &[
            ("Christmas evening here, in Tokyo time", r#"date("2026-12-25 18:30") to "Asia/Tokyo""#),
            ("5pm here is when in London?", r#"5pm to "London""#),
            ("world clock", r#"clock(["Tokyo", "London", "New York"])"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("conversions", &[
            ("to UTC, or to local for your own zone", r#"date("2026-12-25 18:30") to UTC"#),
            ("to a standard zone name", r#"date("2026-12-25 18:30") to "Asia/Tokyo""#),
            ("to a city or country", r#"date("2026-12-25 18:30") to "Sao Paulo""#),
            ("a zone in a variable needs parentheses", r#"z = "Asia/Tokyo"; date("2026-12-25 18:30") to (z)"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("UTC", "utc"), ("utc", "utc"), ("local", "local")],
    convert: Some(convert),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::zone;
    use crate::modules::Fail;
    use crate::signature::List;
    use crate::value::Value;
    use indexmap::IndexMap;
    use jiff::Zoned;
    use jiff::tz::TimeZone;

    /// the same moment in UTC, the zone with no offset that times are often stored in
    ///
    /// Same as `d to UTC`
    #[builtin(examples = ["date(0) to UTC", "date(0).utc.year"], see = ["local", "date"])]
    fn utc(d: &Zoned) -> Zoned {
        d.with_time_zone(TimeZone::UTC)
    }

    /// the same moment in your computer's time zone, the same as `d to local`
    #[builtin(examples = ["(date(0) to UTC).local.year"], see = ["utc"])]
    fn local(d: &Zoned) -> Zoned {
        d.with_time_zone(crate::host::tz())
    }

    /// the time right now in each zone or city
    #[builtin(examples = [r#"clock(["Tokyo", "London"])"#, r#"clock("Asia/Kolkata").hour >= 0"#], see = ["utc", "local"])]
    fn clock(places: &List) -> Result<IndexMap<String, Value>, Fail> {
        let mut m = IndexMap::new();
        for (i, p) in places.borrow().iter().enumerate() {
            let Value::Str(p) = p else {
                return Err(Fail::Arg(0, format!("item {i} must be a str, got {}", p.type_name())));
            };
            m.insert(p.to_string(), Value::date(crate::host::now().with_time_zone(zone(p).map_err(|m| Fail::Arg(0, m))?)));
        }
        Ok(m)
    }

    #[builtin(name = "clock")]
    fn clock_in(place: &str) -> Result<Zoned, Fail> {
        Ok(crate::host::now().with_time_zone(zone(place).map_err(|m| Fail::Arg(0, m))?))
    }
}

/// `d to "Europe/Paris"`.
fn convert(v: &Value, t: &Target) -> Claim {
    let (Value::Date(z), Target::Str(name)) = (v, t) else {
        return None;
    };
    Some(match name.as_str() {
        "local" => Ok(Value::date(z.with_time_zone(crate::host::tz()))),
        _ => zone(name).map(|tz| Value::date(z.with_time_zone(tz))),
    })
}

/// An IANA name (`"Asia/Tokyo"`) or the city part of one, any case, spaces for underscores (`"new york"`).
pub fn zone(name: &str) -> Result<TimeZone, String> {
    let db = jiff::tz::db();
    if let Ok(tz) = db.get(name) {
        return Ok(tz);
    }
    let city = name.trim().replace(' ', "_");
    let names: Vec<_> = db.available().collect();
    let full = names.iter().map(|n| n.as_str()).find(|n| n.rsplit('/').next().is_some_and(|c| c.eq_ignore_ascii_case(&city)));
    match full.or_else(|| crate::modules::geo::zone_of(name)) {
        Some(n) => db.get(n).map_err(msg),
        None => {
            let cities: Vec<_> = names.iter().filter_map(|n| n.as_str().rsplit_once('/').map(|(_, c)| c.replace('_', " ").to_lowercase())).collect();
            let hint = crate::error::did_you_mean(&name.to_lowercase(), cities.iter().map(|c| c.as_str()));
            Err(format!("unknown time zone, city or country {name:?}{hint}"))
        }
    }
}
