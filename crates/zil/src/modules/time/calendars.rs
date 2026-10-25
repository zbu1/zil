//! Calendars: periods, weekdays, business days, month grids, cron, zodiac signs. Weeks start Monday.

use super::{is_weekend, msg, weekday};
use crate::modules::{Fail, Module};
use crate::value::Value;
use jiff::civil::{Date, Weekday};
use jiff::{Span, Zoned};

pub(super) mod world;

pub const MODULE: Module = Module {
    name: "calendars",
    about: "calendar questions: weekdays, business days, holidays, week numbers, month grids, cron schedules, zodiac signs",
    #[rustfmt::skip]
    examples: &[
        ("calendars", &[
            ("Thanksgiving: 4th Thursday", r#"date(2026, 11, 1).nth_weekday(4, "thu")"#),
            ("Memorial Day: last Monday", r#"date(2026, 5, 1).nth_weekday(-1, "mon")"#),
            ("3 business days later", r#"date("2026-12-24").add_workdays(3)"#),
            ("workdays in December", r#"workdays(date("2026-12-01"), date("2026-12-31"))"#),
            ("skipping US holidays", r#"date("2026-12-24").add_workdays(3, "US")"#),
            ("this week's number, by the ISO standard", "today.iso_week"),
            ("what does this cron line mean?", r#"cron("*/15 9-17 * * mon-fri")"#),
            ("when does it run next?", r#""0 9 * * 1-5".next(3)"#),
            ("star sign and Chinese zodiac for a 1990 birthday", r#"[zodiac(date("1990-08-10")), chinese_zodiac(1990)]"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("fields", &["weekday_num", "day_of_year", "iso_week", "quarter"]),
        ("year", &["leap_year", "days_in_month", "days_in_year", "calendar"]),
        ("moving", &["start_of", "end_of", "next", "prev", "nth_weekday", "add_workdays", "workdays", "holidays"]),
        ("cron", &["cron"]),
        ("zodiac", &["zodiac", "chinese_zodiac"]),
    ],
    call: fns::call,
    children: &[world::MODULE],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Cron, Off, SIGNS, add_workdays as add, calendar as grid, chinese, off, unknown_country, workdays as count};
    use crate::modules::Fail;
    use crate::modules::time::{end_of as end, start_of as start, unknown_weekday, weekday};
    use crate::signature::{List, Or};
    use crate::value::Value;
    use jiff::Zoned;

    /// the day of the week as a number, from 1 for Monday to 7 for Sunday
    #[builtin(examples = ["today.weekday_num"], see = ["weekday"])]
    fn weekday_num(d: &Zoned) -> i64 {
        d.weekday().to_monday_one_offset() as i64
    }

    /// which day of the year it is, from 1 to 366
    #[builtin(examples = [r#"date("2026-12-31").day_of_year"#], see = ["iso_week"])]
    fn day_of_year(d: &Zoned) -> i64 {
        d.day_of_year() as i64
    }

    /// the week number by the ISO standard used in Europe and business
    ///
    /// Weeks start on Monday, and week 1 is the one with the year's first Thursday
    #[builtin(examples = [r#"date("2026-12-31").iso_week"#], see = ["day_of_year", "quarter"])]
    fn iso_week(d: &Zoned) -> i64 {
        d.date().iso_week_date().week() as i64
    }

    /// which quarter of the year the date is in, 1 to 4
    #[builtin(examples = ["today.quarter"], see = ["iso_week"])]
    fn quarter(d: &Zoned) -> i64 {
        (d.month() as i64 - 1) / 3 + 1
    }

    /// whether the year is a leap year
    #[builtin(examples = ["leap_year(2028)", "today.leap_year"], see = ["days_in_year"])]
    fn leap_year(d: Or<&Zoned, i64>) -> bool {
        match d {
            Or::A(z) => z.in_leap_year(),
            Or::B(y) => y % 4 == 0 && (y % 100 != 0 || y % 400 == 0),
        }
    }

    /// how many days the date's month has
    #[builtin(examples = [r#"date("2028-02-10").days_in_month"#], see = ["days_in_year"])]
    fn days_in_month(d: &Zoned) -> i64 {
        d.days_in_month() as i64
    }

    /// how many days the date's year has, 365 or 366
    #[builtin(examples = ["today.days_in_year"], see = ["leap_year"])]
    fn days_in_year(d: &Zoned) -> i64 {
        d.days_in_year() as i64
    }

    /// the first moment of the period the date is in
    ///
    /// period is "second", "minute", "hour", "day", "week" (starting Monday), "month", "quarter" or "year"
    #[builtin(examples = [r#"now.start_of("week")"#, r#"now.start_of("quarter")"#], see = ["end_of", "with"])]
    fn start_of(d: &Zoned, period: &str) -> Result<Zoned, Fail> {
        start(d, period).map_err(|m| Fail::Arg(1, m))
    }

    /// the last moment of the period the date is in, with periods as for start_of
    #[builtin(examples = [r#"now.end_of("month")"#, r#"(today.end_of("year") - now).parts"#], see = ["start_of"])]
    fn end_of(d: &Zoned, period: &str) -> Result<Zoned, Fail> {
        end(d, period).map_err(|m| Fail::Arg(1, m))
    }

    fn nth(d: &Zoned, wd: &str, nth: i32) -> Result<Zoned, Fail> {
        let wd = weekday(wd).ok_or_else(|| Fail::Arg(1, unknown_weekday(wd)))?;
        Ok(d.nth_weekday(nth, wd).map_err(|e| e.to_string())?)
    }

    /// the first date after d that falls on the given weekday, never d itself
    ///
    /// Given a cron expression instead, when it next runs, or a list of the next n times
    #[builtin(examples = [r#"today.next("friday")"#, r#"now.next("mon")"#], see = ["prev", "nth_weekday", "cron"])]
    fn next(d: &Zoned, weekday: &str) -> Result<Zoned, Fail> {
        nth(d, weekday, 1)
    }

    #[builtin(name = "next", examples = [r#""0 9 * * mon".next"#, r#""@monthly".next(2)"#])]
    fn next_run(cron: &str, n: Option<i64>) -> Result<Or<Zoned, Vec<Value>>, Fail> {
        let k = match n {
            None => 1,
            Some(n) if (1..=1000).contains(&n) => n as usize,
            Some(n) => return Err(Fail::Arg(1, format!("{n} is out of range\nnote: ask for 1 to 1000 runs"))),
        };
        let runs = Cron::parse(cron).map_err(|m| Fail::Arg(0, m))?.next(&crate::host::now(), k)?;
        Ok(match n {
            None => Or::A(runs.into_iter().next().expect("asked for one")),
            Some(_) => Or::B(runs.into_iter().map(Value::date).collect()),
        })
    }

    /// the latest date before d that falls on the given weekday, never d itself
    #[builtin(examples = [r#"today.prev("sunday")"#], see = ["next"])]
    fn prev(d: &Zoned, weekday: &str) -> Result<Zoned, Fail> {
        nth(d, weekday, -1)
    }

    /// the nth given weekday in d's month, like the 4th Thursday
    ///
    /// -1 is the last one, -2 the one before
    #[builtin(examples = [r#"date(2026, 11, 1).nth_weekday(4, "thu")"#, r#"date(2026, 5, 1).nth_weekday(-1, "mon")"#], see = ["next"])]
    fn nth_weekday(d: &Zoned, n: i64, weekday: &str) -> Result<Zoned, Fail> {
        let wd = super::weekday(weekday).ok_or_else(|| Fail::Arg(2, unknown_weekday(weekday)))?;
        let n = i8::try_from(n)
            .map_err(|_| Fail::Arg(1, format!("{n} is out of range\nnote: use 1 to 5 from the start of the month, or -1 to -5 from the end")))?;
        Ok(d.nth_weekday_of_month(n, wd).map_err(|e| e.to_string())?)
    }

    fn days_off(holidays: Option<Or<&List, &str>>) -> Result<Off, Fail> {
        let v: Vec<Value> = holidays
            .into_iter()
            .map(|h| match h {
                Or::A(l) => Value::List(l.clone()),
                Or::B(s) => Value::str(s),
            })
            .collect();
        off(&v, 2)
    }

    /// the date n business days later, counting only Monday to Friday and skipping holidays
    ///
    /// holidays is a list of dates, or "US" or "UK" for those public holidays. A negative n goes back
    #[builtin(
        examples = ["today.add_workdays(10)", r#"date("2026-12-24").add_workdays(1, "US")"#, r#"date("2026-12-24").add_workdays(1, [date("2026-12-28")])"#],
        see = ["workdays", "holidays", "is_weekend"]
    )]
    fn add_workdays(d: &Zoned, n: i64, holidays: Option<Or<&List, &str>>) -> Result<Zoned, Fail> {
        Ok(add(d, n, &days_off(holidays)?)?)
    }

    /// how many business days (Monday to Friday, less holidays) there are from a up to, but not including, b
    #[builtin(examples = [r#"workdays(today, date("2026-12-25"))"#, r#"workdays(date("2026-12-01"), date("2027-01-01"), "UK")"#], see = ["add_workdays", "holidays"])]
    fn workdays(a: &Zoned, b: &Zoned, holidays: Option<Or<&List, &str>>) -> Result<i64, Fail> {
        Ok(count(a, b, &days_off(holidays)?)?)
    }

    /// a year's public holidays, on the day they're taken off when they fall on a weekend
    ///
    /// "US" gives federal holidays and "UK" bank holidays in England
    #[builtin(examples = [r#"holidays(2026, "US")"#, r#"holidays(2026, "UK").len"#], see = ["add_workdays", "workdays"])]
    fn holidays(year: i64, country: &str) -> Result<Vec<Value>, Fail> {
        let y = i16::try_from(year).map_err(|_| Fail::Arg(0, format!("year {year} out of range")))?;
        let c = Off::Country(super::country(country).ok_or_else(|| Fail::Arg(1, unknown_country(country)))?);
        Ok(c.dates(y).into_iter().map(|d| Value::date(d.to_zoned(crate::host::tz()).expect("holiday in range"))).collect())
    }

    /// a cron schedule in plain English
    ///
    /// Cron, the Unix job scheduler, writes schedules as five fields, minute hour day-of-month month weekday, where * means every and */15 every 15th, or a shortcut like @daily
    #[builtin(examples = [r#"cron("0 9 * * 1-5")"#, r#"cron("30 4 1,15 * fri")"#, r#"cron("@hourly")"#], see = ["next"])]
    fn cron(expr: &str) -> Result<String, Fail> {
        Ok(Cron::parse(expr).map_err(|m| Fail::Arg(0, m))?.explain())
    }

    /// a month drawn as a calendar grid, weeks starting on Monday
    #[builtin(examples = ["calendar(2026, 12)"], see = ["date"])]
    fn calendar(d: &Zoned) -> Result<String, String> {
        grid(d.year() as i64, d.month() as i64)
    }

    #[builtin(name = "calendar")]
    fn calendar_of(year: i64, month: i64) -> Result<String, String> {
        grid(year, month)
    }

    /// the Western star sign for a birthday (today without one)
    #[builtin(examples = [r#"zodiac(date("2000-01-01"))"#], see = ["chinese_zodiac"])]
    fn zodiac(day: Option<&Zoned>) -> &'static str {
        let d = day.map_or_else(|| crate::host::now().date(), |z| z.date());
        let md = d.month() as i32 * 100 + d.day() as i32;
        // Each sign starts on the given month/day; Capricorn wraps over New Year.
        SIGNS.iter().rev().find(|s| md >= s.0).unwrap_or(&SIGNS[SIGNS.len() - 1]).1
    }

    /// the Chinese zodiac element and animal for a year
    ///
    /// It goes by the calendar year, so for a date in January or February before Lunar New Year it gives the next year's sign
    #[builtin(examples = ["chinese_zodiac(2026)", "chinese_zodiac(1984)"], see = ["zodiac"])]
    fn chinese_zodiac(year: Or<i64, &Zoned>) -> String {
        chinese(match year {
            Or::A(y) => y,
            Or::B(z) => z.year() as i64,
        })
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 8] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// A five-field cron expression: minute hour day-of-month month day-of-week.
struct Cron {
    /// Each field's text, after expanding `@daily` and friends.
    text: [String; 5],
    /// Bit i set when value i matches; weekday 7 folds into 0 (Sunday).
    bits: [u64; 5],
}

/// (field, low, high, names); names start at `low`, matched by their first three letters.
type Field = (&'static str, u32, u32, &'static [&'static str]);
const FIELDS: [Field; 5] = [("minute", 0, 59, &[]), ("hour", 0, 23, &[]), ("day", 1, 31, &[]), ("month", 1, 12, &MONTHS), ("weekday", 0, 7, &DAYS)];

impl Cron {
    fn parse(expr: &str) -> Result<Cron, String> {
        let expr = match expr.trim() {
            "@yearly" | "@annually" => "0 0 1 1 *",
            "@monthly" => "0 0 1 * *",
            "@weekly" => "0 0 * * 0",
            "@daily" | "@midnight" => "0 0 * * *",
            "@hourly" => "0 * * * *",
            e => e,
        };
        let text: [String; 5] = expr
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|v: Vec<_>| format!("expected 5 fields (minute hour day month weekday), got {}\nnote: like \"*/15 9-17 * * mon-fri\"", v.len()))?;
        let mut bits = [0; 5];
        for (i, (t, f)) in text.iter().zip(&FIELDS).enumerate() {
            bits[i] = field(t, f)?;
        }
        bits[4] |= bits[4] >> 7 & 1;
        Ok(Cron { text, bits })
    }

    fn day_matches(&self, d: Date) -> bool {
        let dom = self.bits[2] >> d.day() & 1 == 1;
        let dow = self.bits[4] >> d.weekday().to_sunday_zero_offset() & 1 == 1;
        // Classic cron: when both day fields are restricted, either one matching is enough.
        if self.text[2] != "*" && self.text[4] != "*" { dom || dow } else { dom && dow }
    }

    /// The next `n` times strictly after `from`, giving up after 10 years without one (Feb 29 can take 8).
    fn next(&self, from: &Zoned, n: usize) -> Result<Vec<Zoned>, String> {
        let hit = |i: usize, v: i8| self.bits[i] >> v & 1 == 1;
        let mut t = from.datetime().with().second(0).subsec_nanosecond(0).build().map_err(|e| e.to_string())? + Span::new().minutes(1);
        let mut limit = t.year() + 10;
        let mut out = vec![];
        while out.len() < n {
            if t.year() > limit {
                return Err(format!("`{}` never runs", self.text.join(" ")));
            }
            t = if !hit(3, t.month()) {
                t.first_of_month().start_of_day() + Span::new().months(1)
            } else if !self.day_matches(t.date()) {
                t.start_of_day() + Span::new().days(1)
            } else if !hit(1, t.hour()) {
                t.with().minute(0).build().map_err(|e| e.to_string())? + Span::new().hours(1)
            } else if !hit(0, t.minute()) {
                t + Span::new().minutes(1)
            } else {
                out.push(t.to_zoned(from.time_zone().clone()).map_err(|e| e.to_string())?);
                limit = t.year() + 10;
                t + Span::new().minutes(1)
            };
        }
        Ok(out)
    }

    fn explain(&self) -> String {
        let [min, hour, dom, month, dow] = &self.text;
        let single = |t: &str| t.parse::<u32>().ok();
        let hours: Option<Vec<u32>> = hour.split(',').map(single).collect();
        let mut parts = vec![];
        match (single(min), hours) {
            (Some(m), Some(hs)) => parts.push(format!("at {}", list(&hs.iter().map(|h| format!("{h:02}:{m:02}")).collect::<Vec<_>>()))),
            _ => {
                let m = if min == "*" { "every minute".into() } else { phrase(min, "minute", &[], "at ") };
                parts.push(match hour.as_str() {
                    "*" if !m.starts_with("every") => format!("{m} of every hour"),
                    "*" => m,
                    _ => format!("{m}, {}", phrase(hour, "hour", &[], "during ")),
                });
            }
        }
        let days = match (dom != "*", dow != "*") {
            (true, true) => Some(format!("{} of the month, or {}", phrase(dom, "day", &[], "on "), phrase(dow, "day", &DAYS, "on "))),
            (true, false) => Some(format!("{} of the month", phrase(dom, "day", &[], "on "))),
            (false, true) => Some(phrase(dow, "day", &DAYS, "on ")),
            _ => None,
        };
        parts.extend(days);
        if month != "*" {
            parts.push(phrase(month, "month", &MONTHS, "in "));
        }
        parts.join(", ")
    }
}

fn field(t: &str, &(fname, lo, hi, names): &Field) -> Result<u64, String> {
    let val = |s: &str| -> Result<u32, String> {
        let v = s.parse().ok().or_else(|| names.iter().position(|n| s.len() >= 3 && n.to_lowercase().starts_with(&s.to_lowercase())).map(|i| i as u32 + lo));
        v.filter(|v| (lo..=hi).contains(v)).ok_or_else(|| format!("`{s}` is not a valid {fname}\nnote: the {fname} field takes {lo}-{hi}"))
    };
    let mut bits = 0;
    for part in t.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => {
                (r, s.parse::<u32>().ok().filter(|&s| s > 0).ok_or_else(|| format!("`{s}` is not a step\nnote: a step is a positive number, like `*/15`"))?)
            }
            None => (part, 1),
        };
        let (a, b) = match range.split_once('-') {
            _ if range == "*" => (lo, hi),
            Some((a, b)) => (val(a)?, val(b)?),
            None if step > 1 => (val(range)?, hi),
            None => (val(range)?, val(range)?),
        };
        bits |= (a..=b).step_by(step as usize).fold(0, |m, v| m | 1u64 << v);
    }
    Ok(bits)
}

/// One field in words: `*/15` → "every 15 minutes", `1-5` → "on Monday through Friday".
fn phrase(t: &str, unit: &str, names: &[&str], prefix: &str) -> String {
    let lo = if names.len() == 12 { 1 } else { 0 };
    let name = |v: &str| match v.parse::<usize>() {
        Ok(n) if !names.is_empty() => names.get(n - lo).map_or(v.to_string(), |s| s.to_string()),
        _ if !names.is_empty() => names.iter().find(|n| n.to_lowercase().starts_with(&v.to_lowercase())).map_or(v.to_string(), |s| s.to_string()),
        _ => v.to_string(),
    };
    let unit = |plural: bool| match (names.is_empty(), plural) {
        (false, _) => String::new(),
        (true, false) => format!("{unit} "),
        (true, true) => format!("{unit}s "),
    };
    let parts: Vec<String> = t
        .split(',')
        .map(|p| match p.split_once('/') {
            Some((r, s)) => {
                let from = match r.split_once('-') {
                    _ if r == "*" => String::new(),
                    Some((a, b)) => format!(" from {} through {}", name(a), name(b)),
                    None => format!(" from {}", name(r)),
                };
                format!("every {s} {}{from}", unit(true).trim_end())
            }
            None => match p.split_once('-') {
                Some((a, b)) => format!("{prefix}{}{} through {}", unit(true), name(a), name(b)),
                None => format!("{prefix}{}{}", unit(false), name(p)),
            },
        })
        .collect();
    // `0,30` reads "at minutes 0 and 30", not "at minute 0 and at minute 30".
    if t.split(',').count() > 1 && !t.contains(['-', '/']) {
        return format!("{prefix}{}{}", unit(true), list(&t.split(',').map(name).collect::<Vec<_>>()));
    }
    list(&parts)
}

/// "a", "a and b", "a, b and c".
fn list(xs: &[String]) -> String {
    match xs {
        [] => String::new(),
        [x] => x.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Days off besides weekends: given dates, or a country's public holidays.
pub enum Off {
    Dates(Vec<Date>),
    Country(&'static str),
}

impl Off {
    // ponytail: recomputes a year's holidays per day checked; cache by year if long spans get slow.
    fn has(&self, d: Date) -> bool {
        is_weekend(d)
            || match self {
                Off::Dates(ds) => ds.contains(&d),
                Off::Country(_) => self.dates(d.year()).contains(&d),
            }
    }

    /// The holidays in `year`, as observed.
    fn dates(&self, year: i16) -> Vec<Date> {
        let Off::Country(c) = self else { return vec![] };
        let day = |m, d| Date::new(year, m, d).expect("valid holiday");
        let nth = |m, n, wd| day(m, 1).nth_weekday_of_month(n, wd).expect("valid holiday");
        let mut out = Vec::new();
        if *c == "US" {
            // Saturday holidays are observed Friday, Sunday ones Monday.
            for d in [day(1, 1), day(6, 19), day(7, 4), day(11, 11), day(12, 25)] {
                out.push(match d.weekday() {
                    Weekday::Saturday => d.yesterday().expect("in range"),
                    Weekday::Sunday => d.tomorrow().expect("in range"),
                    _ => d,
                });
            }
            out.extend([nth(1, 3, Weekday::Monday), nth(2, 3, Weekday::Monday), nth(5, -1, Weekday::Monday), nth(9, 1, Weekday::Monday)]);
            out.extend([nth(10, 2, Weekday::Monday), nth(11, 4, Weekday::Thursday)]);
        } else {
            // A weekend holiday moves to the next weekday not already a holiday (Christmas and Boxing Day).
            for mut d in [day(1, 1), day(12, 25), day(12, 26)] {
                while is_weekend(d) || out.contains(&d) {
                    d = d.tomorrow().expect("in range");
                }
                out.push(d);
            }
            let easter = easter(year);
            let shift = |n| easter.checked_add(Span::new().days(n)).expect("in range");
            out.extend([shift(-2), shift(1), nth(5, 1, Weekday::Monday), nth(5, -1, Weekday::Monday), nth(8, -1, Weekday::Monday)]);
        }
        out.sort();
        out
    }
}

/// Easter Sunday (Gregorian), by the anonymous algorithm.
fn easter(year: i16) -> Date {
    let y = year as i32;
    let (a, b, c) = (y % 19, y / 100, y % 100);
    let (d, e) = (b / 4, b % 4);
    let g = (8 * b + 13) / 25;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 19 * l) / 433;
    let month = (h + l - 7 * m + 90) / 25;
    let day = (h + l - 7 * m + 33 * month + 19) % 32;
    Date::new(year, month as i8, day as i8).expect("valid Easter")
}

const COUNTRIES: [&str; 2] = ["US", "UK"];

fn country(c: &str) -> Option<&'static str> {
    COUNTRIES.into_iter().find(|n| n.eq_ignore_ascii_case(c))
}

fn unknown_country(c: &str) -> String {
    format!("unknown holiday set {c:?}\nnote: built-in sets are {}; or pass a list of dates", COUNTRIES.join(", "))
}

/// The optional holidays argument at position `i`: a list of dates or a country.
fn off(rest: &[Value], i: usize) -> Result<Off, Fail> {
    Ok(match rest {
        [] => Off::Dates(vec![]),
        [Value::Str(c)] => Off::Country(country(c).ok_or_else(|| Fail::Arg(i, unknown_country(c)))?),
        [Value::List(l)] => Off::Dates(
            l.borrow()
                .iter()
                .map(|v| match v {
                    Value::Date(z) => Ok(z.date()),
                    v => Err(Fail::Arg(i, format!("holidays must be dates, got {}", v.type_name()))),
                })
                .collect::<Result<_, _>>()?,
        ),
        _ => return Err(Fail::BadArgs),
    })
}

// ponytail: business-day math walks day by day; fine for calendar-sized spans, closed form if ever needed for centuries.
/// Move `n` Monday-Friday days that aren't holidays (negative goes back); keeps the time of day.
pub fn add_workdays(z: &Zoned, n: i64, off: &Off) -> Result<Zoned, String> {
    let step = if n < 0 { -1 } else { 1 };
    let mut d = z.date();
    for _ in 0..n.unsigned_abs() {
        d = d.checked_add(Span::new().days(step)).map_err(msg)?;
        while off.has(d) {
            d = d.checked_add(Span::new().days(step)).map_err(msg)?;
        }
    }
    z.with().date(d).build().map_err(msg)
}

/// Monday-Friday days that aren't holidays from `a` up to (not including) `b`; negative if `b` is earlier.
pub fn workdays(a: &Zoned, b: &Zoned, off: &Off) -> Result<i64, String> {
    let (mut d, end, sign) = if a.date() <= b.date() { (a.date(), b.date(), 1) } else { (b.date(), a.date(), -1) };
    let mut n = 0;
    while d < end {
        n += i64::from(!off.has(d));
        d = d.tomorrow().map_err(msg)?;
    }
    Ok(n * sign)
}

/// Month grid like `cal`, Monday first.
pub fn calendar(year: i64, month: i64) -> Result<String, String> {
    let y = i16::try_from(year).map_err(|_| format!("year {year} out of range"))?;
    let m = i8::try_from(month).map_err(|_| format!("month {month} out of range"))?;
    let first = Date::new(y, m, 1).map_err(msg)?;
    let mut cells = vec!["  ".to_string(); first.weekday().to_monday_zero_offset() as usize];
    cells.extend((1..=first.days_in_month()).map(|d| format!("{d:>2}")));
    let rows: Vec<String> = cells.chunks(7).map(|c| c.join(" ")).collect();
    let title = first.strftime("%B %Y").to_string();
    Ok(format!("{title:^20}\nMo Tu We Th Fr Sa Su\n{}", rows.join("\n")))
}

const SIGNS: [(i32, &str); 12] = [
    (120, "♒ Aquarius"),
    (219, "♓ Pisces"),
    (321, "♈ Aries"),
    (420, "♉ Taurus"),
    (521, "♊ Gemini"),
    (621, "♋ Cancer"),
    (723, "♌ Leo"),
    (823, "♍ Virgo"),
    (923, "♎ Libra"),
    (1023, "♏ Scorpio"),
    (1122, "♐ Sagittarius"),
    (1222, "♑ Capricorn"),
];

fn chinese(y: i64) -> String {
    const ANIMALS: [&str; 12] = ["Rat", "Ox", "Tiger", "Rabbit", "Dragon", "Snake", "Horse", "Goat", "Monkey", "Rooster", "Dog", "Pig"];
    const ELEMENTS: [&str; 5] = ["Wood", "Fire", "Earth", "Metal", "Water"];
    // 1984 was a Wood Rat, the start of a 60-year cycle.
    let k = (y % 60 - 1984 % 60).rem_euclid(60);
    format!("{} {}", ELEMENTS[(k / 2 % 5) as usize], ANIMALS[(k % 12) as usize])
}

#[cfg(test)]
mod tests {
    use super::Cron;
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn cron() {
        for (expr, text) in [
            ("0 9 * * 1-5", "at 09:00, on Monday through Friday"),
            ("*/15 9-17 * * mon-fri", "every 15 minutes, during hours 9 through 17, on Monday through Friday"),
            ("30 4 1,15 * fri", "at 04:30, on days 1 and 15 of the month, or on Friday"),
            ("0,30 * * * *", "at minutes 0 and 30 of every hour"),
            ("0 0 1 jan *", "at 00:00, on day 1 of the month, in January"),
            ("@hourly", "at minute 0 of every hour"),
        ] {
            assert_eq!(show(&format!("cron({expr:?})")), text);
        }
        let from: jiff::Zoned = "2026-10-07T10:31:20[UTC]".parse().unwrap();
        let at = |expr: &str, n| Cron::parse(expr).unwrap().next(&from, n).unwrap().iter().map(|z| z.strftime("%F %R %a").to_string()).collect::<Vec<_>>();
        assert_eq!(at("*/15 * * * *", 2), ["2026-10-07 10:45 Wed", "2026-10-07 11:00 Wed"]);
        assert_eq!(at("0 9 * * 1-5", 3), ["2026-10-08 09:00 Thu", "2026-10-09 09:00 Fri", "2026-10-12 09:00 Mon"]);
        assert_eq!(at("0 0 13 * 5", 2), ["2026-10-09 00:00 Fri", "2026-10-13 00:00 Tue"]);
        assert_eq!(at("0 0 29 2 *", 2), ["2028-02-29 00:00 Tue", "2032-02-29 00:00 Sun"]);
        assert!(Cron::parse("0 0 30 2 *").unwrap().next(&from, 1).is_err());
        assert!(try_eval(r#"cron("61 * * * *")"#).is_err());
        assert!(try_eval(r#""* *".next"#).is_err());
    }

    #[test]
    fn zodiac() {
        assert_eq!(
            show(r#"[zodiac(date("2000-01-01")), zodiac(date("2000-01-20")), zodiac(date("2000-03-20")), zodiac(date("2000-12-31"))]"#),
            r#"["♑ Capricorn", "♒ Aquarius", "♓ Pisces", "♑ Capricorn"]"#
        );
        assert_eq!(
            show("[chinese_zodiac(2026), chinese_zodiac(1984), chinese_zodiac(2000), chinese_zodiac(1900)]"),
            r#"["Fire Horse", "Wood Rat", "Metal Dragon", "Metal Rat"]"#
        );
    }
}
