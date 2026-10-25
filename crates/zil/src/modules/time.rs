//! Dates: parsing (fixed formats and natural language), fields, date math, relative text.
//! Calendars, time zones and the sky (sunrise, moon) are children.

pub mod calendars;
mod parsing;
pub mod sky;
pub mod zones;

use crate::ast::BinOp;
use crate::interp::Interp;
use crate::modules::units::{self, Unit};
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::Value;
use jiff::civil::{self, Date, Weekday};
use jiff::{SignedDuration, Span, Timestamp, Zoned};
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
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("fields", &["year", "month", "day", "hour", "minute", "second", "weekday"]),
        ("checks", &["is_weekend", "is_weekday", "is_today", "is_past", "is_future"]),
        ("between", &["diff", "age", "relative", "parts"]),
        ("convert", &["date", "with", "format", "unix", "date_ms", "unix_ms"]),
        ("timing", &["timeit", "stopwatch"]),
    ],
    call,
    targets: &[("unix", "unix"), ("unix_ms", "unix_ms"), ("date", "date")],
    ident: Some(ident),
    binary: Some(binary),
    compare: Some(compare),
    children: &[calendars::MODULE, zones::MODULE, sky::MODULE],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("date", "date(s: str) -> date / date(s: str, fmt: str) -> date / date(y: int, m: int, d: int, h?: int, min?: int, s?: int) -> date / date(unix: int) -> date", "a date from text: ISO (2026-12-25), US (12/25/2026), written out (Dec 25 2026) or plain English (next friday at 5pm, 3 days ago)", &[r#"date("2026-12-25")"#, r#"date("next friday at 5pm")"#, r#"date("3 days ago")"#, r#"date("25.12.2026", "%d.%m.%Y")"#, "date(2026, 12, 25, 18, 30)", "date(0)"], &["format", "with", "start_of"]).pretty()
        .detail("Also from a format string with strftime codes, from year, month, day and optional time numbers, or from a Unix timestamp"),
    doc("year", "year(d: date) -> int", "the year of a date", &["now.year"], &["month", "day"]),
    doc("month", "month(d: date) -> int", "the month of a date, from 1 for January to 12", &["now.month"], &["year", "day"]),
    doc("day", "day(d: date) -> int", "the day of the month, from 1 to 31", &["now.day"], &["month", "weekday"]),
    doc("hour", "hour(d: date) -> int", "the hour of a date, from 0 to 23", &["now.hour"], &["minute", "second"]),
    doc("minute", "minute(d: date) -> int", "the minute of a date, from 0 to 59", &["now.minute"], &["hour", "second"]),
    doc("second", "second(d: date) -> int", "the second of a date, from 0 to 59", &["now.second"], &["hour", "minute"]),
    doc("weekday", "weekday(d: date) -> str", "the name of the day of the week, like \"Friday\"", &[r#"date("2026-12-25").weekday"#], &["day", "format"]),
    doc("format", "format(d: date, fmt: str) -> str / format(fmt: str, args?: any, ...) -> str", "a date as text laid out by strftime codes, such as %Y for the year, %m the month, %d the day, %H:%M the time and %B the month's name", &[r#"now.format("%B %d, %Y")"#, r#"now.format("%H:%M")"#, r#"format("%5.2f%%", 12.345)"#], &["date", "fixed"])
        .detail("Given a format string first instead, formats numbers printf-style"),
    doc("with", "with(d: date, fields: map) -> date", "the same date with some parts changed, given as a map of year, month, day, hour, minute and second, like {day: 1}", &["today.with({day: 1})", "now.with({hour: 9, minute: 0})"], &["start_of", "date"]),
    doc("is_weekend", "is_weekend(d: date) -> bool", "whether the date is a Saturday or Sunday", &[r#"date("2026-10-10").is_weekend"#], &["is_weekday", "add_workdays"]),
    doc("is_weekday", "is_weekday(d: date) -> bool", "whether the date is Monday to Friday", &["today.is_weekday"], &["is_weekend"]),
    doc("is_today", "is_today(d: date) -> bool", "whether the date falls on today's date", &["now.is_today", "tomorrow.is_today"], &["is_past"]),
    doc("is_past", "is_past(d: date) -> bool", "whether the date is earlier than now", &["yesterday.is_past"], &["is_future", "is_today"]),
    doc("is_future", "is_future(d: date) -> bool", "whether the date is later than now", &["tomorrow.is_future"], &["is_past"]),
    doc("age", "age(d: date) -> int", "how many whole years have passed since d, as for someone's age from their birthday", &[r#"date("1990-06-15").age"#], &["diff"]),
    doc("diff", "diff(a: date, b: date) -> str", "the time from a to b in calendar terms", &[r#"diff(date("2025-08-03"), date("2026-10-06 04:00"))"#], &["age", "relative", "parts"])
        .detail("Years, months, days and smaller"),
    doc("relative", "relative(d: date) -> str", "how far the date is from now, in words like \"in 3 days\" or \"2 hours ago\"", &[r#"date("2026-12-25").relative"#, "(now - 3 h).relative"], &["diff", "parts"]),
    doc("parts", "parts(duration: quantity) -> str", "a duration split into the largest units that fit, up to three of days, hours, minutes and seconds", &[r#"(date("2026-12-25") - date("2026-10-06 14:24")).parts"#, "5000 s.parts"], &["relative", "diff"])
        .detail("For units of your choice, write `to h min`"),
    doc("unix", "unix(d: date) -> int", "the Unix timestamp: seconds since the start of 1970 in UTC, the same as `d to unix`", &["date(0).unix", "date(86400) to unix"], &["date", "unix_ms"]),
    doc("unix_ms", "unix_ms(d: date) -> int", "the Unix timestamp in milliseconds, the same as `d to unix_ms`", &["date(1).unix_ms"], &["date_ms", "unix"]),
    doc("date_ms", "date_ms(ms: int) -> date", "the date for a Unix timestamp in milliseconds, the form JavaScript and Java use", &["date_ms(1798178400000) == date(1798178400)"], &["unix_ms", "date"]),
    doc("timeit", "timeit(f: fn) -> quantity", "how long it takes to call f, which takes no arguments", &["timeit(|| (1..1000).sum) < 1 s"], &["stopwatch"]),
    doc("stopwatch", "stopwatch() -> quantity", "the time since stopwatch was last called (0 s the first time), for timing parts of a script", &["stopwatch() >= 0 s"], &["timeit"]).mutates(),
];

fn call(it: &mut Interp, name: &'static str, args: &[Value], span: &crate::lexer::Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("date", [Str(s)]) => Value::date(parse(s, &crate::host::now()).ok_or_else(|| {
            Fail::Arg(0, format!("cannot read {s:?} as a date\nhelp: try \"2026-12-25 18:30\", \"12/25/2026\", \"Dec 25 2026\" or \"next friday\", or give a format: date(s, \"%d.%m.%Y\")"))
        })?),
        ("date", [Str(s), Str(f)]) => {
            let z = with_format(s, f, &crate::host::tz());
            Value::date(z.ok_or_else(|| Fail::Arg(0, format!("{s:?} does not match the format {f:?}")))?)
        }
        ("date", [Int(y, _), Int(m, _), Int(d, _), rest @ ..]) if rest.len() <= 3 => {
            let mut t = [0i64; 3];
            for (slot, v) in t.iter_mut().zip(rest) {
                let Int(n, _) = v else {
                    return Err(Fail::BadArgs);
                };
                *slot = *n;
            }
            let small = |n: i64| i8::try_from(n).map_err(|_| format!("{n} out of range"));
            let year = i16::try_from(*y).map_err(|_| format!("year {y} out of range"))?;
            let dt = civil::DateTime::new(year, small(*m)?, small(*d)?, small(t[0])?, small(t[1])?, small(t[2])?, 0);
            Value::date(dt.and_then(|dt| dt.to_zoned(crate::host::tz())).map_err(|e| e.to_string())?)
        }
        ("date", [Int(n, _)]) => {
            let ts = Timestamp::from_second(*n).map_err(|e| e.to_string())?;
            Value::date(ts.to_zoned(crate::host::tz()))
        }
        ("year", [Date(z)]) => Value::int(z.year() as i64),
        ("month", [Date(z)]) => Value::int(z.month() as i64),
        ("day", [Date(z)]) => Value::int(z.day() as i64),
        ("hour", [Date(z)]) => Value::int(z.hour() as i64),
        ("minute", [Date(z)]) => Value::int(z.minute() as i64),
        ("second", [Date(z)]) => Value::int(z.second() as i64),
        ("weekday", [Date(z)]) => Value::str(z.strftime("%A").to_string()),
        ("is_weekend", [Date(z)]) => Bool(is_weekend(z.date())),
        ("is_weekday", [Date(z)]) => Bool(!is_weekend(z.date())),
        ("is_today", [Date(z)]) => Bool(z.date() == crate::host::now().with_time_zone(z.time_zone().clone()).date()),
        ("is_past", [Date(z)]) => Bool(z.timestamp() < crate::host::timestamp()),
        ("is_future", [Date(z)]) => Bool(z.timestamp() > crate::host::timestamp()),
        ("with", [Date(z), Map(m)]) => {
            let mut fields = Vec::new();
            for (k, v) in m.borrow().iter() {
                let Int(n, _) = v else {
                    return Err(Fail::Arg(1, format!("`{k}` must be an integer, got {}", v.type_name())));
                };
                fields.push((k.clone(), *n));
            }
            Value::date(with(z, &fields)?)
        }
        ("age", [Date(z)]) => Value::int(age(z, &crate::host::now())),
        ("diff", [Date(a), Date(b)]) => Value::str(diff(a, b)?),
        ("relative", [Date(z)]) => Value::str(relative(z, &crate::host::now())),
        ("parts", [Qty(x, u)]) if is_duration(u) => Value::str(parts(u.to_si(*x), 3)),
        ("format", [Date(z), Str(f)]) => Value::str(jiff::fmt::strtime::format(f.as_bytes(), &**z).map_err(|e| e.to_string())?),
        ("format", [Str(f), rest @ ..]) => Value::str(crate::modules::math::formatting::printf(f, rest)?),
        ("unix", [Date(z)]) => Value::int(z.timestamp().as_second()),
        ("unix_ms", [Date(z)]) => Value::int(z.timestamp().as_millisecond()),
        ("date_ms", [Int(n, _)]) => Value::date(Timestamp::from_millisecond(*n).map_err(msg)?.to_zoned(crate::host::tz())),
        ("timeit", [f]) => {
            let t = crate::host::timestamp();
            it.call(f, vec![], span)?;
            seconds(crate::host::timestamp().duration_since(t).as_secs_f64())
        }
        ("stopwatch", []) => {
            let now = crate::host::timestamp();
            seconds(crate::state::with(|s| s.stopwatch.replace(Some(now))).map_or(0.0, |t| now.duration_since(t).as_secs_f64()))
        }
        _ => return Err(Fail::BadArgs),
    })
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
