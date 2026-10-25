//! Dates: parsing (fixed formats and natural language), fields, date math, relative text.
//! Calendars, time zones and the sky (sunrise, moon) are children.

pub mod calendars;
mod parsing;
pub mod sky;
pub mod zones;

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::modules::units::{self, Unit};
use crate::modules::{Claim, Module};
use crate::value::Value;
use jiff::civil::{Date, Weekday};
use jiff::{SignedDuration, Span, Zoned};
use parsing::{parse, time, with_format};

pub const MODULE: Module = Module {
    name: "time",
    about: "dates and times: read them from text, do arithmetic with them, and show them in plain words",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Dates and times. A date is a moment: a day and a time of day in a time zone. A duration is a length of time."),
        ("writing", "\
Durations are quantities with a time unit, like `90 min`, and units can run together: `1h30m`."),
        ("good to know", "\
Dates typed without a zone are in your computer's local time. `today`, `tomorrow` and `yesterday` are midnight at the \
start of those days. Months and years follow the calendar, so adding a month to January 31 gives the last day of \
February."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("time", &[
            ("day of the week", r#"date("2026-12-25").weekday"#),
            ("adding a month stops at the end of a shorter month", r#"date("2026-01-31") + 1 mo"#),
            ("plain-English dates", r#"date("next friday")"#),
            ("how long until new year", r#"(date("2027-01-01") - now).parts"#),
            ("3 hours ago, in words", "(now - 3 h).relative"),
            ("first of next month", "(today + 1 mo).with({day: 1})"),
            ("months with a Friday the 13th", r#"(1..=12).filter(|m| date(2026, m, 13).weekday == "Friday")"#),
            ("a Unix timestamp as a date", "date(1798178400)"),
            ("durations written without spaces", "1h30m + 2d4h"),
            ("hours worked from 8:45 to 17:00", "17:00 - 8:45"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[
            ("date: a moment, made with date() or now", r#"date("2026-12-25 18:30")"#),
            ("time of day: 9:30 means 9:30 today", "9:30 + 45 min"),
            ("duration: an amount of time", "1h30m"),
        ]),
        ("pretty", &[
            ("a date, long: written out, time included", r#"pretty(date("2026-12-25"))"#),
            ("a date, short", r#"pretty(date("2026-12-25 18:30"), "short")"#),
            ("a date not in local time names its zone", r#"pretty(date("2026-12-25 18:30") to "Asia/Tokyo")"#),
            ("a duration, long: up to 3 of d, h, min and s", "pretty(5000 s)"),
            ("a duration, short: up to 2 of them", r#"pretty(5000 s, "short")"#),
        ]),
        ("operators", &[
            ("date + or - a duration moves it", r#"date("2026-01-31") + 1 mo"#),
            ("date - date is the duration between", r#"date("2027-01-01") - date("2026-12-25")"#),
            ("subtracting two times of day in quotes gives hours", r#""17:00" - "08:45""#),
            ("< and > compare which is earlier", "yesterday < now"),
        ]),
        ("conversions", &[
            ("to unix: seconds since 1970 (UTC)", r#"date("2026-12-25") to unix"#),
            ("to unix_ms does the same in milliseconds. Unix timestamps are how many systems store times", ""),
            ("to date reads a date from text", r#""2026-12-25" to date"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("fields", &["year", "month", "day", "hour", "minute", "second", "weekday"]),
        ("checks", &["is_weekend", "is_weekday", "is_today", "is_past", "is_future"]),
        ("between", &["diff", "age", "relative", "parts"]),
        ("convert", &["date", "with", "format", "unix", "date_ms", "unix_ms"]),
        ("timing", &["timeit", "stopwatch"]),
    ],
    call: fns::call,
    targets: &[("unix", "unix"), ("unix_ms", "unix_ms"), ("date", "date")],
    ident: Some(ident),
    binary: Some(binary),
    compare: Some(compare),
    children: &[calendars::MODULE, zones::MODULE, sky::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{
        age as years_since, diff as between, is_duration, is_weekend as weekend, msg, parse, parts as split, relative as words, seconds, with as with_fields,
        with_format,
    };
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::{As, Func, Map, Qty, Rest};
    use crate::value::Value;
    use jiff::{Timestamp, Zoned, civil};

    /// a date from text: ISO (2026-12-25), US (12/25/2026), written out (Dec 25 2026) or plain English (next friday at 5pm, 3 days ago)
    ///
    /// Also from a format string with strftime codes, from year, month, day and optional time numbers, from year, month and day in another calendar ("hebrew", "islamic", "persian", "julian", "coptic", "ethiopian" or "indian"), or from a Unix timestamp
    #[builtin(examples = [r#"date("2026-12-25")"#, r#"date("next friday at 5pm")"#, r#"date("3 days ago")"#], see = ["format", "with", "start_of"], pretty)]
    fn date(s: &str) -> Result<Zoned, Fail> {
        parse(s, &crate::host::now()).ok_or_else(|| {
            Fail::Arg(0, format!("cannot read {s:?} as a date\nhelp: try \"2026-12-25 18:30\", \"12/25/2026\", \"Dec 25 2026\" or \"next friday\", or give a format: date(s, \"%d.%m.%Y\")"))
        })
    }

    #[builtin(name = "date", examples = [r#"date("25.12.2026", "%d.%m.%Y")"#])]
    fn date_format(s: &str, fmt: &str) -> Result<Zoned, Fail> {
        with_format(s, fmt, &crate::host::tz()).ok_or_else(|| Fail::Arg(0, format!("{s:?} does not match the format {fmt:?}")))
    }

    #[builtin(name = "date", examples = ["date(2026, 12, 25, 18, 30)"])]
    fn date_parts(y: i64, m: i64, d: i64, #[default(0)] h: i64, #[default(0)] min: i64, #[default(0)] s: i64) -> Result<Zoned, String> {
        let small = |n: i64| i8::try_from(n).map_err(|_| format!("{n} out of range"));
        let year = i16::try_from(y).map_err(|_| format!("year {y} out of range"))?;
        let dt = civil::DateTime::new(year, small(m)?, small(d)?, small(h)?, small(min)?, small(s)?, 0);
        dt.and_then(|dt| dt.to_zoned(crate::host::tz())).map_err(|e| e.to_string())
    }

    #[builtin(name = "date", examples = [r#"date(5787, 7, 1, "hebrew")"#, r#"date(1405, 1, 1, "persian")"#])]
    fn date_in(y: i64, m: i64, d: i64, calendar: &str) -> Result<Zoned, Fail> {
        let date = super::calendars::world::to_date(calendar, y, m, d).map_err(|e| Fail::Arg(3, e))?;
        Ok(date.to_zoned(crate::host::tz()).map_err(|e| e.to_string())?)
    }

    #[builtin(name = "date", examples = ["date(0)"])]
    fn date_unix(unix: i64) -> Result<Zoned, String> {
        Ok(Timestamp::from_second(unix).map_err(|e| e.to_string())?.to_zoned(crate::host::tz()))
    }

    /// the year of a date
    #[builtin(examples = ["now.year"], see = ["month", "day"])]
    fn year(d: &Zoned) -> i64 {
        d.year() as i64
    }

    /// the month of a date, from 1 for January to 12
    #[builtin(examples = ["now.month"], see = ["year", "day"])]
    fn month(d: &Zoned) -> i64 {
        d.month() as i64
    }

    /// the day of the month, from 1 to 31
    #[builtin(examples = ["now.day"], see = ["month", "weekday"])]
    fn day(d: &Zoned) -> i64 {
        d.day() as i64
    }

    /// the hour of a date, from 0 to 23
    #[builtin(examples = ["now.hour"], see = ["minute", "second"])]
    fn hour(d: &Zoned) -> i64 {
        d.hour() as i64
    }

    /// the minute of a date, from 0 to 59
    #[builtin(examples = ["now.minute"], see = ["hour", "second"])]
    fn minute(d: &Zoned) -> i64 {
        d.minute() as i64
    }

    /// the second of a date, from 0 to 59
    #[builtin(examples = ["now.second"], see = ["hour", "minute"])]
    fn second(d: &Zoned) -> i64 {
        d.second() as i64
    }

    /// the name of the day of the week, like "Friday"
    #[builtin(examples = [r#"date("2026-12-25").weekday"#], see = ["day", "format"])]
    fn weekday(d: &Zoned) -> String {
        d.strftime("%A").to_string()
    }

    /// a date as text laid out by strftime codes, such as %Y for the year, %m the month, %d the day, %H:%M the time and %B the month's name
    ///
    /// Given a format string first instead, formats numbers printf-style
    #[builtin(examples = [r#"now.format("%B %d, %Y")"#, r#"now.format("%H:%M")"#], see = ["date", "fixed"])]
    fn format(d: &Zoned, fmt: &str) -> Result<String, String> {
        jiff::fmt::strtime::format(fmt.as_bytes(), d).map_err(|e| e.to_string())
    }

    #[builtin(name = "format", examples = [r#"format("%5.2f%%", 12.345)"#])]
    fn printf(fmt: &str, args: Rest<&Value>) -> Result<String, Fail> {
        let args: Vec<Value> = args.0.into_iter().cloned().collect();
        Ok(crate::modules::math::formatting::printf(fmt, &args)?)
    }

    /// the same date with some parts changed, given as a map of year, month, day, hour, minute and second, like {day: 1}
    #[builtin(examples = ["today.with({day: 1})", "now.with({hour: 9, minute: 0})"], see = ["start_of", "date"])]
    fn with(d: &Zoned, fields: &Map) -> Result<Zoned, Fail> {
        let mut out = Vec::new();
        for (k, v) in fields.borrow().iter() {
            let Value::Int(n, _) = v else {
                return Err(Fail::Arg(1, format!("`{k}` must be an integer, got {}", v.type_name())));
            };
            out.push((k.clone(), *n));
        }
        Ok(with_fields(d, &out)?)
    }

    /// whether the date is a Saturday or Sunday
    #[builtin(examples = [r#"date("2026-10-10").is_weekend"#], see = ["is_weekday", "add_workdays"])]
    fn is_weekend(d: &Zoned) -> bool {
        weekend(d.date())
    }

    /// whether the date is Monday to Friday
    #[builtin(examples = ["today.is_weekday"], see = ["is_weekend"])]
    fn is_weekday(d: &Zoned) -> bool {
        !weekend(d.date())
    }

    /// whether the date falls on today's date
    #[builtin(examples = ["now.is_today", "tomorrow.is_today"], see = ["is_past"])]
    fn is_today(d: &Zoned) -> bool {
        d.date() == crate::host::now().with_time_zone(d.time_zone().clone()).date()
    }

    /// whether the date is earlier than now
    #[builtin(examples = ["yesterday.is_past"], see = ["is_future", "is_today"])]
    fn is_past(d: &Zoned) -> bool {
        d.timestamp() < crate::host::timestamp()
    }

    /// whether the date is later than now
    #[builtin(examples = ["tomorrow.is_future"], see = ["is_past"])]
    fn is_future(d: &Zoned) -> bool {
        d.timestamp() > crate::host::timestamp()
    }

    /// how many whole years have passed since d, as for someone's age from their birthday
    #[builtin(examples = [r#"date("1990-06-15").age"#], see = ["diff"])]
    fn age(d: &Zoned) -> i64 {
        years_since(d, &crate::host::now())
    }

    /// the time from a to b in calendar terms
    ///
    /// Years, months, days and smaller
    #[builtin(examples = [r#"diff(date("2025-08-03"), date("2026-10-06 04:00"))"#], see = ["age", "relative", "parts"])]
    fn diff(a: &Zoned, b: &Zoned) -> Result<String, String> {
        between(a, b)
    }

    /// how far the date is from now, in words like "in 3 days" or "2 hours ago"
    #[builtin(examples = [r#"date("2026-12-25").relative"#, "(now - 3 h).relative"], see = ["diff", "parts"])]
    fn relative(d: &Zoned) -> String {
        words(d, &crate::host::now())
    }

    /// a duration split into the largest units that fit, up to three of days, hours, minutes and seconds
    ///
    /// For units of your choice, write `to h min`
    #[builtin(examples = [r#"(date("2026-12-25") - date("2026-10-06 14:24")).parts"#, "5000 s.parts"], see = ["relative", "diff"])]
    fn parts(duration: Qty) -> Result<String, Fail> {
        let (x, u) = duration.get();
        if !is_duration(u) {
            return Err(Fail::BadArgs);
        }
        Ok(split(u.to_si(x), 3))
    }

    /// the Unix timestamp: seconds since the start of 1970 in UTC, the same as `d to unix`
    #[builtin(examples = ["date(0).unix", "date(86400) to unix"], see = ["date", "unix_ms"])]
    fn unix(d: &Zoned) -> i64 {
        d.timestamp().as_second()
    }

    /// the Unix timestamp in milliseconds, the same as `d to unix_ms`
    #[builtin(examples = ["date(1).unix_ms"], see = ["date_ms", "unix"])]
    fn unix_ms(d: &Zoned) -> i64 {
        d.timestamp().as_millisecond()
    }

    /// the date for a Unix timestamp in milliseconds, the form JavaScript and Java use
    #[builtin(examples = ["date_ms(1798178400000) == date(1798178400)"], see = ["unix_ms", "date"])]
    fn date_ms(ms: i64) -> Result<Zoned, String> {
        Ok(Timestamp::from_millisecond(ms).map_err(msg)?.to_zoned(crate::host::tz()))
    }

    /// how long it takes to call f, which takes no arguments
    #[builtin(examples = ["timeit(|| (1..1000).sum) < 1 s"], see = ["stopwatch"])]
    fn timeit(it: &mut Interp, f: Func, span: &Span) -> Result<As<Qty>, Fail> {
        let t = crate::host::timestamp();
        it.call(&f, vec![], span)?;
        Ok(seconds(crate::host::timestamp().duration_since(t).as_secs_f64()).into())
    }

    /// the time since stopwatch was last called (0 s the first time), for timing parts of a script
    #[builtin(examples = ["stopwatch() >= 0 s"], see = ["timeit"], mutates)]
    fn stopwatch() -> As<Qty> {
        let now = crate::host::timestamp();
        seconds(crate::state::with(|s| s.stopwatch.replace(Some(now))).map_or(0.0, |t| now.duration_since(t).as_secs_f64())).into()
    }
}

/// Seconds in the unit that reads best: `4.2 ms`, `1.5 s`.
fn seconds(s: f64) -> Value {
    units::simplify(s, &units::unit("s").expect("seconds"))
}

/// `now`, `today`, `tomorrow`, `yesterday`.
fn ident(_: &mut Interp, name: &str) -> Claim {
    match name {
        "now" => Some(Ok(Value::date(crate::host::now()))),
        "today" | "tomorrow" | "yesterday" => Some(Ok(Value::date(parse(name, &crate::host::now()).unwrap()))),
        _ => None,
    }
}

/// `date ± duration`, and `date - date` in days.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::*;
    Some(match (op, a, b) {
        (BinOp::Add, Date(z), Qty(v, u)) | (BinOp::Add, Qty(v, u), Date(z)) => add(z, *v, u).map(Value::date),
        (BinOp::Sub, Date(z), Qty(v, u)) => add(z, -v, u).map(Value::date),
        // Elapsed time, except same-clock-time dates are whole calendar days even across a DST change.
        // `"17:00" - "08:45"`: two clock times, as today at those times.
        (BinOp::Sub, Str(x), Str(y)) => {
            let (x, y) = (time(&x.to_lowercase())?, time(&y.to_lowercase())?);
            let today = crate::host::now().date();
            let at = |t| today.to_datetime(t).to_zoned(crate::host::tz()).map(Value::date).map_err(msg);
            return binary(op, &at(x).ok()?, &at(y).ok()?);
        }
        (BinOp::Sub, Date(x), Date(y)) => {
            // Same zone first: jiff counts days in one calendar (and panics on mixed zones, and on
            // `total` of an empty span, as of 0.2.38).
            let x = x.with_time_zone(y.time_zone().clone());
            let cal = x.since(&**y).and_then(|s| if s.is_zero() { Ok(0.0) } else { s.total((jiff::Unit::Day, &**y)) });
            let secs = x.duration_since(y).as_secs_f64();
            // Whole days stay days; within a day, hours or minutes read better.
            let (n, u) = match cal {
                Ok(n) if n.fract() == 0.0 => (n, "d"),
                _ if secs.abs() < 3600.0 => (secs / 60.0, "min"),
                _ if secs.abs() < 86400.0 => (secs / 3600.0, "h"),
                _ => (secs / 86400.0, "d"),
            };
            units::unit(u).map(|u| Qty(n, u))
        }
        _ => return None,
    })
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Date(a), Value::Date(b)) => Some(a.timestamp().cmp(&b.timestamp())),
        _ => None,
    }
}

type R<T> = Result<T, String>;

/// An error as its message, for `map_err`.
pub fn msg(x: impl ToString) -> String {
    x.to_string()
}

pub const PERIODS: &str = "second, minute, hour, day, week, month, quarter, year";

fn unknown_period(p: &str) -> String {
    let hint = crate::error::did_you_mean(p, PERIODS.split(", "));
    if hint.is_empty() { format!("unknown period {p:?}\nnote: periods are {PERIODS}") } else { format!("unknown period {p:?}{hint}") }
}

/// `z + v u`; whole days, weeks, months and years follow the calendar (Jan 31 + 1 mo = Feb 28, and a
/// day keeps the clock time across a DST change), not a fixed length.
pub fn add(z: &Zoned, v: f64, u: &Unit) -> R<Zoned> {
    if u.dim() != units::unit("s")?.dim() {
        return Err(format!("cannot add {} to a date\nhelp: dates take durations, like `date + 3 d` or `date - 2 h`", units::describe(u, None)));
    }
    let n = v as i64;
    let span = match () {
        _ if v.fract() != 0.0 => None,
        _ if u.is("d") => Some(Span::new().try_days(n)),
        _ if u.is("wk") => Some(Span::new().try_weeks(n)),
        _ if u.is("mo") => Some(Span::new().try_months(n)),
        _ if u.is("yr") => Some(Span::new().try_years(n)),
        _ => None,
    };
    let r = match span {
        Some(span) => z.checked_add(span.map_err(msg)?),
        None => z.checked_add(SignedDuration::try_from_secs_f64(u.to_si(v)).map_err(msg)?),
    };
    r.map_err(msg)
}

const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

pub fn unknown_weekday(w: &str) -> String {
    format!("unknown weekday {w:?}{}", crate::error::did_you_mean(&w.to_lowercase(), WEEKDAYS))
}

/// "monday", "Mon", "thurs" (any prefix of 3+ letters).
pub fn weekday(w: &str) -> Option<Weekday> {
    const NAMES: [&str; 7] = WEEKDAYS;
    let w = w.to_lowercase();
    let i = NAMES.iter().position(|n| w.len() >= 3 && n.starts_with(&w))?;
    Weekday::from_monday_one_offset(i as i8 + 1).ok()
}

pub fn start_of(z: &Zoned, period: &str) -> R<Zoned> {
    let d = z.date();
    let day = match period {
        "second" => return z.with().subsec_nanosecond(0).build().map_err(msg),
        "minute" => return z.with().second(0).subsec_nanosecond(0).build().map_err(msg),
        "hour" => {
            return z.with().minute(0).second(0).subsec_nanosecond(0).build().map_err(msg);
        }
        "day" => d,
        "week" => d.checked_sub(Span::new().days(d.weekday().to_monday_zero_offset())).map_err(msg)?,
        "month" => d.first_of_month(),
        "quarter" => Date::new(d.year(), (d.month() - 1) / 3 * 3 + 1, 1).map_err(msg)?,
        "year" => d.first_of_year(),
        _ => return Err(unknown_period(period)),
    };
    day.to_zoned(z.time_zone().clone()).map_err(msg)
}

/// The last nanosecond of the period.
pub fn end_of(z: &Zoned, period: &str) -> R<Zoned> {
    let next = add_period(&start_of(z, period)?, period, 1)?;
    next.checked_sub(SignedDuration::from_nanos(1)).map_err(msg)
}

fn add_period(z: &Zoned, period: &str, n: i64) -> R<Zoned> {
    let span = match period {
        "second" => Span::new().try_seconds(n),
        "minute" => Span::new().try_minutes(n),
        "hour" => Span::new().try_hours(n),
        "day" => Span::new().try_days(n),
        "week" => Span::new().try_weeks(n),
        "month" => Span::new().try_months(n),
        "quarter" => Span::new().try_months(3 * n),
        "year" => Span::new().try_years(n),
        _ => return Err(unknown_period(period)),
    };
    z.checked_add(span.map_err(msg)?).map_err(msg)
}

/// Same date and time with some fields replaced: year, month, day, hour, minute, second.
pub fn with(z: &Zoned, fields: &[(String, i64)]) -> R<Zoned> {
    let mut w = z.with();
    for &(ref k, n) in fields {
        let small = |n: i64| i8::try_from(n).map_err(|_| format!("{k} {n} out of range"));
        w = match k.as_str() {
            "year" => w.year(i16::try_from(n).map_err(|_| format!("year {n} out of range"))?),
            "month" => w.month(small(n)?),
            "day" => w.day(small(n)?),
            "hour" => w.hour(small(n)?),
            "minute" => w.minute(small(n)?),
            "second" => w.second(small(n)?),
            _ => {
                const FIELDS: [&str; 6] = ["year", "month", "day", "hour", "minute", "second"];
                let hint = crate::error::did_you_mean(k, FIELDS);
                return Err(if hint.is_empty() {
                    format!("unknown field `{k}`\nnote: fields are {}", FIELDS.join(", "))
                } else {
                    format!("unknown field `{k}`{hint}")
                });
            }
        };
    }
    w.build().map_err(msg)
}

pub fn is_weekend(d: Date) -> bool {
    matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday)
}

/// Whole years from `birth` to `now`.
pub fn age(birth: &Zoned, now: &Zoned) -> i64 {
    let (b, n) = (birth.date(), now.with_time_zone(birth.time_zone().clone()).date());
    let before_birthday = (n.month(), n.day()) < (b.month(), b.day());
    (n.year() - b.year()) as i64 - i64::from(before_birthday)
}

/// Calendar difference from `a` to `b`: "1 yr 2 mo 3 d 4 h".
pub fn diff(a: &Zoned, b: &Zoned) -> R<String> {
    let s = a.until((jiff::Unit::Year, b)).map_err(msg)?;
    let fields = [
        (s.get_years() as i64, "yr"),
        (s.get_months() as i64, "mo"),
        (s.get_days() as i64, "d"),
        (s.get_hours() as i64, "h"),
        (s.get_minutes(), "min"),
        (s.get_seconds(), "s"),
    ];
    let parts: Vec<String> = fields.iter().filter(|f| f.0 != 0).map(|(n, u)| format!("{} {u}", n.abs())).collect();
    let sign = if s.is_negative() { "-" } else { "" };
    Ok(if parts.is_empty() { "0 s".into() } else { format!("{sign}{}", parts.join(" ")) })
}

pub fn is_duration(u: &Unit) -> bool {
    u.dim() == units::unit("s").unwrap().dim()
}

/// Automatic mixed units for `si` seconds: up to `n` of d, h, min, s, last one rounded.
pub fn parts(si: f64, n: usize) -> String {
    let us: Vec<_> = ["d", "h", "min", "s"].iter().map(|n| units::unit(n).unwrap()).collect();
    let start = us.iter().position(|v| si.abs() >= v.scale()).unwrap_or(3);
    let us = &us[start..(start + n).min(4)];
    let last = us.last().unwrap().scale();
    units::split((si / last).round() * last, us)
}

/// "in 3 days", "2 hours ago", "just now".
pub fn relative(z: &Zoned, now: &Zoned) -> String {
    let secs = (z.timestamp().as_second() - now.timestamp().as_second()) as f64;
    let a = secs.abs();
    let (n, unit) = match a {
        _ if a < 45.0 => return "just now".into(),
        _ if a < 45.0 * 60.0 => ((a / 60.0).round(), "minute"),
        _ if a < 22.0 * 3600.0 => ((a / 3600.0).round(), "hour"),
        _ if a < 26.0 * 86400.0 => ((a / 86400.0).round(), "day"),
        _ if a < 320.0 * 86400.0 => ((a / 2629746.0).round().max(1.0), "month"),
        _ => ((a / 31556952.0).round().max(1.0), "year"),
    };
    let s = if n == 1.0 { "" } else { "s" };
    if secs > 0.0 { format!("in {n} {unit}{s}") } else { format!("{n} {unit}{s} ago") }
}

#[cfg(test)]
mod tests {
    use super::calendars::{Off, add_workdays, calendar, workdays};
    use super::*;
    use crate::interp::tests::{show, try_eval};

    /// Tuesday 2026-10-06 14:30 UTC.
    fn now() -> Zoned {
        "2026-10-06T14:30:00[UTC]".parse().unwrap()
    }

    fn p(s: &str) -> String {
        let z = parse(s, &now()).unwrap_or_else(|| panic!("cannot parse {s:?}"));
        z.strftime("%Y-%m-%d %H:%M").to_string()
    }

    #[test]
    fn formats() {
        assert_eq!(p("2026-12-25"), "2026-12-25 00:00");
        assert_eq!(p("2026-12-25 10:30"), "2026-12-25 10:30");
        assert_eq!(p("2026-12-25T10:00Z"), "2026-12-25 10:00");
        assert_eq!(p("12/25/2026"), "2026-12-25 00:00");
        assert_eq!(p("03/04/2026"), "2026-03-04 00:00");
        assert_eq!(p("25/12/2026"), "2026-12-25 00:00");
        assert_eq!(p("12/25/26"), "2026-12-25 00:00");
        assert_eq!(p("Dec 25 2026"), "2026-12-25 00:00");
        assert_eq!(p("December 25, 2026"), "2026-12-25 00:00");
        assert_eq!(p("25 Dec 2026 5pm"), "2026-12-25 17:00");
        assert_eq!(p("Fri, 25 Dec 2026 10:00:00 +0000"), "2026-12-25 10:00");
        assert!(parse("garbage", &now()).is_none());
        assert!(parse("", &now()).is_none());
    }

    #[test]
    fn natural_language() {
        assert_eq!(p("now"), "2026-10-06 14:30");
        assert_eq!(p("today"), "2026-10-06 00:00");
        assert_eq!(p("tomorrow at 5pm"), "2026-10-07 17:00");
        assert_eq!(p("yesterday noon"), "2026-10-05 12:00");
        assert_eq!(p("next friday"), "2026-10-09 00:00");
        assert_eq!(p("last friday"), "2026-10-02 00:00");
        assert_eq!(p("tuesday"), "2026-10-06 00:00");
        assert_eq!(p("next tue"), "2026-10-13 00:00");
        assert_eq!(p("3 days ago"), "2026-10-03 14:30");
        assert_eq!(p("in 2 weeks"), "2026-10-20 14:30");
        assert_eq!(p("an hour from now"), "2026-10-06 15:30");
        assert_eq!(p("in 1 month"), "2026-11-06 14:30");
        assert_eq!(p("next month"), "2026-11-06 00:00");
        assert_eq!(p("end of month"), "2026-10-31 23:59");
        assert_eq!(p("start of the week"), "2026-10-05 00:00");
        assert_eq!(p("first day of next month"), "2026-11-01 00:00");
        assert_eq!(p("last day of year"), "2026-12-31 00:00");
        assert_eq!(p("5:30 pm"), "2026-10-06 17:30");
        assert_eq!(p("dec 25 2026 at 9am"), "2026-12-25 09:00");
        assert!(parse("next blursday", &now()).is_none());
    }

    #[test]
    fn calendar_fns() {
        let z = now();
        assert_eq!(start_of(&z, "quarter").unwrap().date().to_string(), "2026-10-01");
        assert_eq!(end_of(&z, "week").unwrap().strftime("%F %T").to_string(), "2026-10-11 23:59:59");
        assert_eq!(add_workdays(&z, 4, &Off::Dates(vec![])).unwrap().date().to_string(), "2026-10-12");
        assert_eq!(add_workdays(&z, -2, &Off::Dates(vec![])).unwrap().date().to_string(), "2026-10-02");
        let fri = parse("2026-10-09", &z).unwrap();
        let mon = parse("2026-10-19", &z).unwrap();
        assert_eq!(workdays(&fri, &mon, &Off::Dates(vec![])).unwrap(), 6);
        assert_eq!(workdays(&mon, &fri, &Off::Dates(vec![])).unwrap(), -6);
        let birth = parse("1990-10-07", &z).unwrap();
        assert_eq!(age(&birth, &z), 35);
        assert_eq!(diff(&parse("2025-08-03", &z).unwrap(), &parse("2026-10-06 04:00", &z).unwrap()).unwrap(), "1 yr 2 mo 3 d 4 h");
        assert_eq!(relative(&parse("in 3 days", &z).unwrap(), &z), "in 3 days");
        assert_eq!(relative(&parse("2 hours ago", &z).unwrap(), &z), "2 hours ago");
        assert_eq!(with(&z, &[("day".into(), 1)]).unwrap().date().to_string(), "2026-10-01");
        assert!(with(&z, &[("month".into(), 13)]).is_err());
        assert!(calendar(2026, 10).unwrap().ends_with("26 27 28 29 30 31"));
    }

    #[test]
    fn end_to_end() {
        assert_eq!(show(r#"date("2026-01-31") + 1 mo"#)[..10], *"2026-02-28");
        assert_eq!(show(r#"date("2026-12-25") - date("2026-12-20")"#), "5 d");
        assert_eq!(show(r#"date("2026-03-01T12:00") + 90 min"#)[..16], *"2026-03-01 13:30");
        assert_eq!(show(r#"date(0) to "UTC""#), "1970-01-01 00:00:00 +00:00");
        assert_eq!(show(r#"date(0) to unix"#), "0");
        assert_eq!(show(r#"(date(0) to UTC).year"#), "1970");
        assert_eq!(show(r#"date("2026-12-25")"#), "2026-12-25");
        assert_eq!(show("date(2026, 12, 25, 18, 30)")[..16], *"2026-12-25 18:30");
        assert_eq!(show("tomorrow - today"), "1 d");
        assert_eq!(show("today == now.start_of(\"day\")"), "true");
        assert_eq!(show(r#"date("2026-11-01").nth_weekday(4, "thursday")"#), "2026-11-26");
        assert_eq!(show(r#"date("2026-10-06").next("fri").weekday"#), "Friday");
        assert_eq!(show(r#"(date("2026-12-25T00:00Z") - date("2026-10-06T14:24Z")) to d h min"#), "79 d 9 h 36 min");
        assert_eq!(show("5.5 ft to ft in"), "5 ft 6 in");
        assert_eq!(show("-90 s to min s"), "-1 min 30 s");
        assert_eq!(show("5000 s.parts"), "1 h 23 min 20 s");
        assert_eq!(show("0.5 d.parts"), "12 h");
        // Chicago's DST ends 2026-11-01: days keep the clock time, whole-day gaps stay whole.
        let midnight = r#"(date("2026-11-01T05:00Z") to "America/Chicago")"#;
        assert_eq!(show(&format!("{midnight} + 1 d")), "2026-11-02");
        assert_eq!(show(&format!("({midnight} + 1 d) - {midnight}")), "1 d");
        assert_eq!(show(&format!("{midnight} + 24 h"))[..16], *"2026-11-01 23:00");
        assert_eq!(show(r#"date("2026-12-25") - (date("2026-12-25") to "Asia/Tokyo")"#), "0 d");
        // A zone held in a variable: `to (z)` or an interpolated string.
        assert_eq!(show(r#"z = "Tokyo"; [date(0) to (z), date(0) to "Asia/{z}"].map(|d| d.hour)"#), "[9, 9]");
        // Session 11: compact durations, clock times, cities, holidays, ms timestamps.
        assert_eq!(show("1h30m"), "90 min");
        assert_eq!(show("2d4h"), "52 h");
        assert_eq!(show("1m30s.parts"), "1 min 30 s");
        assert_eq!(show("17:00 - 8:45"), "8.25 h");
        assert_eq!(show(r#""17:00" - "08:45""#), "8.25 h");
        assert_eq!(show("9:30 - 9:15"), "15 min");
        assert_eq!(show("(9:30 + 45 min).format(\"%H:%M\")"), "10:15");
        assert_eq!(show("5pm.hour"), "17");
        assert_eq!(show(r#"(date("2026-12-25T17:00Z") to "tokyo").hour"#), "2");
        assert_eq!(show(r#"(date("2026-12-25T17:00Z") to "New York").hour"#), "12");
        assert_eq!(show(r#"clock(["Tokyo", "London"]).keys"#), r#"["Tokyo", "London"]"#);
        assert!(try_eval(r#"now to "Narnia""#).is_err());
        assert_eq!(show(r#"holidays(2026, "US").len"#), "11");
        assert_eq!(show(r#"holidays(2026, "UK")[-1]"#), "2026-12-28");
        assert_eq!(show(r#"date("2026-12-24").add_workdays(3, "US")"#), "2026-12-30");
        assert_eq!(show(r#"workdays(date("2026-12-21"), date("2027-01-04"), "UK")"#), "7");
        assert_eq!(show(r#"date("2026-12-24").add_workdays(1, [date("2026-12-25")])"#), "2026-12-28");
        assert_eq!(show("date_ms(1500).unix_ms"), "1500");
        assert_eq!(show("date(1) to unix_ms"), "1000");
        assert_eq!(show("timeit(|| 1) < 1 s"), "true");
        assert!(try_eval("5 km to h min").is_err());
        assert!(try_eval(r#"now.start_of("fortnight")"#).is_err());
    }
}
