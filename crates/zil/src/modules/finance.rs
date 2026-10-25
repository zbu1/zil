//! Finance: pay, growth, loans, splitting bills, percent changes. Functions tell their arguments apart
//! by unit: `$1000` is a sum, `$500/mo` a payment, `7%/yr` a rate, `10 yr` a time, `"monthly"` compounding.

use crate::modules::units::money::{currency_of, places};
use crate::modules::units::{PER_TIME, TIME, Unit, convert_value, unit};
use crate::modules::{Fail, Module, Section};
use crate::value::{Value, num};
use indexmap::IndexMap;

pub const MODULE: Module = Module {
    name: "finance",
    about: "money math: pay rates, savings growth, loan payments, splitting bills, and profit margins",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Money math for everyday questions about pay, savings, loans and splitting costs.\n\n\
Every function here tells its arguments apart by unit, so they read like the question you're asking. An amount of money \
like `$1000` is a sum, money per time like `$500/mo` is a regular payment, a percentage per time like `7%/yr` is an \
interest rate, and a time like `10 yr` is how long. A word like \"monthly\" or \"daily\" says how often interest is \
added to the balance, where it starts earning interest itself."),
        ("good to know", "\
Without a word for it, interest is added with each payment when there are payments, as loans do, and otherwise once \
per period of the rate, so `7%/yr` on its own compounds yearly."),
    ],
    examples: EXAMPLES,
    guide: GUIDE,
    fns: fns::FNS,
    groups: &[
        ("pay", &["salary"]),
        ("growth", &["grow", "cagr", "doubling", "apy", "real_rate"]),
        ("loans", &["payment", "payoff", "amortize"]),
        ("splitting", &["share", "settle"]),
        ("percent", &["change", "margin", "markup"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const EXAMPLES: &[Section] = &[
    ("finance", &[
        ("pay by the hour, day, week, month and year", "salary($25/h)"),
        ("the monthly payment on a 30-year mortgage", "payment($400000, 6.5%/yr, 30 yr)"),
        ("total interest paid over the loan", "payment($400000, 6.5%/yr, 30 yr) * 30 yr - $400000"),
        ("the date $200 a month clears a $5000 card balance", "today + payoff($5000, 22%/yr, $200/mo)"),
        ("saving $500 a month for 30 years at 7%", "grow($0, 7%/yr, 30 yr, $500/mo)"),
        ("split a bill", "share($100, 3)"),
        ("who owes whom", "settle({ana: $120, ben: $0, cy: $30})"),
    ]),
];

#[rustfmt::skip]
const GUIDE: &[Section] = &[
];

#[zil_macros::builtins]
mod fns {
    use super::{Args, MO, YR, money, pair, per_unit, per_yr, pmt};
    use crate::modules::Fail;
    use crate::modules::units::money::currency_of;
    use crate::modules::units::{TIME, Unit, convert_value, unit};
    use crate::signature::{As, List, Map, Num, Or, Qty};
    use crate::value::{Value, num};
    use indexmap::IndexMap;

    /// Sorts the given arguments by kind, as `Args` reads them.
    fn args(vs: impl IntoIterator<Item = Option<Value>>) -> Result<Args, Fail> {
        Args::new(&vs.into_iter().flatten().collect::<Vec<_>>())
    }

    fn sum(v: &Or<Num, Qty>) -> Option<Value> {
        Some(v.value().clone())
    }

    fn word(s: Option<&str>) -> Option<Value> {
        s.map(Value::str)
    }

    fn opts(m: Option<&Map>) -> Option<Value> {
        m.map(|m| Value::Map(m.clone()))
    }

    /// a pay rate as pay per hour, day, week, month and year
    ///
    /// Assumes 40 hours a week and 52 paid weeks a year. Change them with a map like {hours: 37.5, weeks: 48}
    #[builtin(examples = ["salary($25/h)", "salary(85000 USD/yr, {hours: 37.5})"], see = ["grow"])]
    fn salary(rate: Qty, opts: Option<&Map>) -> Result<IndexMap<String, Value>, Fail> {
        let a = args([Some(rate.0.clone()), self::opts(opts)])?;
        a.only_opts(&["hours", "weeks"])?;
        let opt = |k: &str, default: f64| a.opts.get(k).map_or(Some(default), num).ok_or(format!("option `{k}` must be a number"));
        let (hours, weeks) = (opt("hours", 40.0)?, opt("weeks", 52.0)?);
        let (x, u) = rate.get();
        let (x, cur, per) = match currency_of(x, u) {
            Some(c) => c,
            None => (x, Unit::default(), u.clone()),
        };
        let [(t, -1)] = per.0.as_slice() else {
            return Err("needs pay per time, like $25/h or 85000 USD/yr".into());
        };
        let hours_per = match t.name.as_str() {
            "h" => 1.0,
            "min" => 1.0 / 60.0,
            "s" => 1.0 / 3600.0,
            "d" | "workday" => hours / 5.0,
            "wk" | "workwk" => hours,
            "mo" | "workmo" => hours * weeks / 12.0,
            "yr" | "workyr" => hours * weeks,
            p => return Err(Fail::Arg(0, format!("cannot tell how many hours are paid per `{p}`\nhelp: give pay per h, d, wk, mo or yr"))),
        };
        let hourly = x / hours_per;
        let rows = [("hour", 1.0), ("day", hours / 5.0), ("week", hours), ("month", hours * weeks / 12.0), ("year", hours * weeks)];
        Ok(rows.into_iter().map(|(k, h)| (k.to_string(), Value::qty(hourly * h, cur.clone()))).collect())
    }

    /// what money will be worth after time at the rate
    ///
    /// A starting sum, plus a regular payment if given, with interest compounding
    #[builtin(examples = ["grow($10000, 7%/yr, 30 yr)", "grow($0, 7%/yr, 30 yr, $500/mo)", r#"grow($1000, 5%/yr, 1 yr, "daily")"#], see = ["cagr", "payment"])]
    fn grow(sum: Or<Num, Qty>, rate: Qty, time: Qty, payment: Option<Or<Qty, &str>>, compounding: Option<&str>) -> Result<As<Or<f64, Qty>>, Fail> {
        let payment = payment.map(|p| match p {
            Or::A(q) => q.0,
            Or::B(s) => Value::str(s),
        });
        let a = args([self::sum(&sum), Some(rate.0), Some(time.0), payment, word(compounding)])?;
        let ((r, rate_period), t) = (a.rate()?, a.time()?);
        let (lump, cur) = match (&a.sums[..], &a.pay) {
            ([], Some(_)) => (0.0, Unit::default()),
            _ => a.sum("starting sum")?,
        };
        let (pay, cur, period) = match a.pay(&cur)? {
            Some((x, cur, period)) => (x, cur, period.scale()),
            None => (0.0, cur, rate_period),
        };
        let i = a.per_period(r, period);
        let n = t / period;
        let g = (1.0 + i).powf(n);
        let fv = lump * g + if i == 0.0 { pay * n } else { pay * (g - 1.0) / i };
        Ok(Value::qty(fv, cur).into())
    }

    /// the steady yearly growth rate that would take start to end over time (compound annual growth rate)
    #[builtin(examples = ["cagr($1000, $2500, 8 yr)"], see = ["grow", "change"])]
    fn cagr(start: Or<Num, Qty>, end: Or<Num, Qty>, time: Qty) -> Result<As<Qty>, Fail> {
        let (t, u) = time.get();
        if u.dim() != TIME {
            return Err(Fail::BadArgs);
        }
        let (s, e) = pair(start.value(), end.value())?;
        Ok(Value::qty((e / s).powf(YR / u.to_si(t)) - 1.0, per_yr()?).into())
    }

    /// how long it takes money to double at the rate
    #[builtin(examples = ["doubling(7%/yr)"], see = ["grow"])]
    fn doubling(rate: Qty, compounding: Option<&str>) -> Result<As<Qty>, Fail> {
        let a = args([Some(rate.0), word(compounding)])?;
        let (r, period) = a.rate()?;
        let secs = period * 2f64.ln() / (1.0 + a.per_period(r, period)).ln();
        Ok(Value::qty(secs / YR, unit("yr")?).into())
    }

    /// the yearly rate you really earn once compounding is included (annual percentage yield)
    #[builtin(examples = [r#"apy(5%/yr, "daily")"#, r#"apy(5%/yr, "monthly")"#], see = ["grow"])]
    fn apy(rate: Qty, compounding: &str) -> Result<As<Qty>, Fail> {
        let a = args([Some(rate.0), word(Some(compounding))])?;
        Ok(Value::qty(a.per_period(a.rate()?.0, YR), per_yr()?).into())
    }

    /// what a return is really worth once inflation is taken out
    #[builtin(examples = ["real_rate(7%/yr, 3%/yr)"], see = ["grow"])]
    fn real_rate(rate: Or<Qty, Num>, inflation: Or<Qty, Num>) -> Result<As<Qty>, Fail> {
        let a = args([Some(rate.value().clone()), Some(inflation.value().clone())])?;
        let plain = a.sums.iter().filter(|s| s.1.0.is_empty()).map(|s| s.0);
        let [r, inflation] = a.rates.iter().map(|r| r.0).chain(plain).collect::<Vec<_>>()[..] else {
            return Err(Fail::BadArgs);
        };
        Ok(Value::qty((1.0 + r) / (1.0 + inflation) - 1.0, per_yr()?).into())
    }

    /// the monthly payment that pays off a loan over time, interest included
    #[builtin(examples = ["payment($400000, 6.5%/yr, 30 yr)", "payment($25000, 7%/yr, 5 yr) * 5 yr"], see = ["payoff", "amortize"])]
    fn payment(loan: Or<Num, Qty>, rate: Qty, time: Qty, compounding: Option<&str>) -> Result<As<Qty>, Fail> {
        let a = args([self::sum(&loan), Some(rate.0), Some(time.0), word(compounding)])?;
        let ((p, cur), (r, _), t) = (a.sum("loan")?, a.rate()?, a.time()?);
        Ok(Value::qty(pmt(p, a.per_period(r, MO), t / MO), cur.mul(&unit("mo")?, -1)).into())
    }

    /// how long a regular payment takes to pay off a balance with interest, rounded up to whole payments
    #[builtin(examples = ["payoff($5000, 22%/yr, $200/mo)", "today + payoff($5000, 22%/yr, $200/mo)"], see = ["payment"])]
    fn payoff(balance: Or<Num, Qty>, rate: Qty, payment: Qty, compounding: Option<&str>) -> Result<As<Qty>, Fail> {
        let a = args([self::sum(&balance), Some(rate.0), Some(payment.0), word(compounding)])?;
        let ((b, cur), (r, _)) = (a.sum("balance")?, a.rate()?);
        let Some((pay, _, period)) = a.pay(&cur)? else {
            return Err("needs a payment, like $200/mo".into());
        };
        let i = a.per_period(r, period.scale());
        if pay <= b * i {
            return Err("the payment never covers the interest".into());
        }
        let n = if i == 0.0 { b / pay } else { -(1.0 - b * i / pay).ln() / (1.0 + i).ln() };
        Ok(Value::Qty((n - 1e-9).ceil(), period).into())
    }

    /// a loan's month-by-month schedule
    ///
    /// For each payment, its number n, the payment, how much is interest and how much pays down the loan (principal), and the balance left. Pass {extra: $200/mo} to see the effect of paying extra
    #[builtin(examples = ["amortize($1000, 12%/yr, 3 mo)", "amortize($400000, 6.5%/yr, 30 yr, {extra: $300/mo}).len * 1 mo to yr"], see = ["payment"])]
    fn amortize(loan: Or<Num, Qty>, rate: Qty, time: Qty, opts: Option<&Map>) -> Result<Vec<Value>, Fail> {
        let a = args([self::sum(&loan), Some(rate.0), Some(time.0), self::opts(opts)])?;
        a.only_opts(&["extra"])?;
        let ((p, cur), (r, _), t) = (a.sum("loan")?, a.rate()?, a.time()?);
        let i = a.per_period(r, MO);
        let monthly = cur.mul(&unit("mo")?, -1);
        let extra = match a.opts.get("extra") {
            None => 0.0,
            Some(Value::Qty(x, u)) => convert_value(*x, u, &monthly)?,
            Some(v) => num(v).ok_or("extra must be an amount a month, like $200/mo")?,
        };
        let pay = pmt(p, i, t / MO) + extra;
        let (mut rows, mut balance) = (Vec::new(), p);
        // ponytail: the 100k cap stops a runaway loop on odd inputs; no real loan runs 8000 years.
        while balance > p * 1e-9 && rows.len() < 100_000 {
            let interest = balance * i;
            let principal = (pay - interest).min(balance);
            balance -= principal;
            let mut m = IndexMap::from([("n".to_string(), Value::int(rows.len() as i64 + 1))]);
            for (k, v) in [("payment", interest + principal), ("interest", interest), ("principal", principal), ("balance", balance.max(0.0))] {
                m.insert(k.into(), Value::qty(v, cur.clone()));
            }
            rows.push(Value::map(m));
        }
        Ok(rows)
    }

    /// split a sum into parts, in equal shares or by the weights in a list, rounded to the cent so they still add up exactly
    #[builtin(examples = ["share($100, 3)", "share($100, [2, 1, 1])"], see = ["settle"])]
    fn share(sum: Or<Num, Qty>, parts: Or<i64, &List>) -> Result<Vec<Value>, Fail> {
        let (x, cur) = money(sum.value())?;
        let weights: Vec<f64> = match parts {
            Or::A(n) if n > 0 => {
                crate::state::check_len(n as usize, "items")?;
                vec![1.0; n as usize]
            }
            Or::A(_) => return Err(Fail::BadArgs),
            Or::B(l) => l.borrow().iter().map(|w| num(w).filter(|w| *w >= 0.0)).collect::<Option<_>>().ok_or("weights must be numbers, 0 or more")?,
        };
        let total_w: f64 = weights.iter().sum();
        if total_w <= 0.0 {
            return Err("needs a positive number of parts".into());
        }
        // Largest remainder, in cents: floor every share, then hand the leftover cents to the biggest fractions.
        let q = per_unit(&cur);
        let cents = (x.abs() * q).round();
        let raw: Vec<f64> = weights.iter().map(|w| cents * w / total_w).collect();
        let mut parts: Vec<f64> = raw.iter().map(|r| r.floor()).collect();
        let mut order: Vec<usize> = (0..parts.len()).collect();
        order.sort_by(|&i, &j| (raw[j] - parts[j]).total_cmp(&(raw[i] - parts[i])));
        let left = (cents - parts.iter().sum::<f64>()) as usize;
        for &i in order.iter().take(left) {
            parts[i] += 1.0;
        }
        Ok(parts.iter().map(|c| Value::qty(x.signum() * c / q, cur.clone())).collect())
    }

    /// given how much each person paid, the payments that even things out so everyone has paid the same
    #[builtin(examples = ["settle({ana: $120, ben: $0, cy: $30})"], see = ["share"])]
    fn settle(paid: &Map) -> Result<Vec<Value>, Fail> {
        let m = paid.borrow();
        let Some(first) = m.values().next() else {
            return Ok(vec![]);
        };
        let (_, cur) = money(first)?;
        let paid = m.iter().map(|(k, v)| Ok((k.clone(), pair(&Value::qty(0.0, cur.clone()), v)?.1))).collect::<Result<Vec<_>, Fail>>()?;
        let fair = paid.iter().map(|p| p.1).sum::<f64>() / paid.len() as f64;
        let by_size = |mut v: Vec<(String, f64)>| {
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            v
        };
        let mut owed = by_size(paid.iter().map(|(k, x)| (k.clone(), x - fair)).filter(|p| p.1 > 0.0).collect());
        let mut owing = by_size(paid.iter().map(|(k, x)| (k.clone(), fair - x)).filter(|p| p.1 > 0.0).collect());
        let half_cent = 0.5 / per_unit(&cur);
        let (mut out, mut i, mut j) = (Vec::new(), 0, 0);
        while i < owing.len() && j < owed.len() {
            let t = owing[i].1.min(owed[j].1);
            if t >= half_cent {
                out.push(Value::str(format!("{} pays {} {}", owing[i].0, owed[j].0, Value::qty(t, cur.clone()))));
            }
            owing[i].1 -= t;
            owed[j].1 -= t;
            i += (owing[i].1 < half_cent) as usize;
            j += (owed[j].1 < half_cent) as usize;
        }
        Ok(out)
    }

    /// how much a value changed, as a fraction of where it started
    ///
    /// From 80 to 100 is 0.25, a 25% rise
    #[builtin(examples = ["change($80, $100)", "change(80, 60).percent"], see = ["margin", "cagr"])]
    fn change(from: Or<Num, Qty>, to: Or<Num, Qty>) -> Result<f64, Fail> {
        let (a, b) = pair(from.value(), to.value())?;
        Ok((b - a) / a)
    }

    /// profit as a fraction of the selling price
    #[builtin(examples = ["margin($60, $100)"], see = ["markup"])]
    fn margin(cost: Or<Num, Qty>, price: Or<Num, Qty>) -> Result<f64, Fail> {
        let (c, p) = pair(cost.value(), price.value())?;
        Ok((p - c) / p)
    }

    /// profit as a fraction of what it cost you
    #[builtin(examples = ["markup($60, $100)"], see = ["margin"])]
    fn markup(cost: Or<Num, Qty>, price: Or<Num, Qty>) -> Result<f64, Fail> {
        let (c, p) = pair(cost.value(), price.value())?;
        Ok((p - c) / c)
    }
}

const YR: f64 = 31556952.0;
const MO: f64 = YR / 12.0;

/// Arguments sorted by unit.
#[derive(Default)]
struct Args {
    /// Value and currency, empty for plain numbers.
    sums: Vec<(f64, Unit)>,
    /// Nominal yearly rate, and the seconds it's quoted per (`1%/mo` compounds monthly).
    rates: Vec<(f64, f64)>,
    /// Seconds.
    time: Option<f64>,
    /// Amount per period, currency, period unit.
    pay: Option<(f64, Unit, Unit)>,
    /// Compounding periods a year; infinite for continuous.
    compound: Option<f64>,
    opts: IndexMap<String, Value>,
}

impl Args {
    fn new(vals: &[Value]) -> Result<Args, Fail> {
        let mut a = Args::default();
        for v in vals {
            match v {
                Value::Map(m) => a.opts = m.borrow().clone(),
                Value::Str(s) => a.compound = Some(compounding(s)?),
                Value::Qty(x, u) if u.dim() == TIME => a.time = Some(u.to_si(*x)),
                Value::Qty(x, u) if u.dim() == PER_TIME => {
                    let period = match u.0.as_slice() {
                        [(t, -1)] => t.scale(),
                        _ => YR,
                    };
                    a.rates.push((u.to_si(*x) * YR, period));
                }
                Value::Qty(x, u) => match currency_of(*x, u) {
                    Some((x, cur, rest)) if rest.0.is_empty() => a.sums.push((x, cur)),
                    Some((x, cur, rest)) if rest.dim() == PER_TIME && rest.0.len() == 1 => a.pay = Some((x, cur, rest.pow(-1))),
                    _ => return Err(Fail::BadArgs),
                },
                v => a.sums.push((num(v).ok_or(Fail::BadArgs)?, Unit::default())),
            }
        }
        Ok(a)
    }

    fn rate(&self) -> Result<(f64, f64), Fail> {
        match self.rates[..] {
            [r] => Ok(r),
            _ => Err("needs one rate, like 7%/yr".into()),
        }
    }

    fn time(&self) -> Result<f64, Fail> {
        self.time.ok_or_else(|| "needs a time, like 10 yr".into())
    }

    /// The one sum; `what` names it in the error.
    fn sum(&self, what: &str) -> Result<(f64, Unit), Fail> {
        match &self.sums[..] {
            [s] => Ok(s.clone()),
            _ => Err(format!("needs one {what}, like $1000").into()),
        }
    }

    /// The payment per period, in `cur` (its own currency if `cur` is a plain number), and the period.
    fn pay(&self, cur: &Unit) -> Result<Option<(f64, Unit, Unit)>, Fail> {
        let Some((x, c, period)) = &self.pay else {
            return Ok(None);
        };
        if cur.0.is_empty() {
            return Ok(Some((*x, c.clone(), period.clone())));
        }
        Ok(Some((convert_value(*x, c, cur)?, cur.clone(), period.clone())))
    }

    /// Interest per period of `secs`; compounds once per period unless told otherwise.
    fn per_period(&self, r: f64, secs: f64) -> f64 {
        match self.compound {
            Some(m) if m.is_infinite() => (r * secs / YR).exp() - 1.0,
            Some(m) => (1.0 + r / m).powf(m * secs / YR) - 1.0,
            None => r * secs / YR,
        }
    }

    fn only_opts(&self, keys: &[&str]) -> Result<(), Fail> {
        match self.opts.keys().find(|k| !keys.contains(&k.as_str())) {
            Some(k) => {
                let hint = crate::error::did_you_mean(k, keys.iter().copied());
                Err(if hint.is_empty() {
                    format!("unknown option `{k}`\nnote: options are {}", keys.join(", "))
                } else {
                    format!("unknown option `{k}`{hint}")
                }
                .into())
            }
            None => Ok(()),
        }
    }
}

/// Compounding periods a year.
fn compounding(s: &str) -> Result<f64, Fail> {
    Ok(match s {
        "yearly" | "annually" => 1.0,
        "quarterly" => 4.0,
        "monthly" => 12.0,
        "weekly" => YR / 604800.0,
        "daily" => YR / 86400.0,
        "continuous" | "continuously" => f64::INFINITY,
        _ => {
            const KINDS: [&str; 6] = ["yearly", "quarterly", "monthly", "weekly", "daily", "continuous"];
            let hint = crate::error::did_you_mean(s, KINDS);
            let more = if hint.is_empty() { format!("\nnote: compounding is {}", KINDS.join(", ")) } else { hint };
            return Err(format!("unknown compounding {s:?}{more}").into());
        }
    })
}

/// Monthly payment on a loan of `p` over `n` months at `i` a month.
fn pmt(p: f64, i: f64, n: f64) -> f64 {
    if i == 0.0 { p / n } else { p * i / (1.0 - (1.0 + i).powf(-n)) }
}

/// Two values as numbers in the first one's unit.
fn pair(a: &Value, b: &Value) -> Result<(f64, f64), Fail> {
    match (a, b) {
        (Value::Qty(x, u), Value::Qty(y, w)) if u.dim() == w.dim() => Ok((*x, convert_value(*y, w, u)?)),
        _ => Ok((num(a).ok_or(Fail::BadArgs)?, num(b).ok_or(Fail::BadArgs)?)),
    }
}

/// A money value, or a plain number, as amount and currency.
fn money(v: &Value) -> Result<(f64, Unit), Fail> {
    match v {
        Value::Qty(x, u) => match currency_of(*x, u) {
            Some((x, cur, rest)) if rest.0.is_empty() => Ok((x, cur)),
            _ => Err(Fail::BadArgs),
        },
        v => Ok((num(v).ok_or(Fail::BadArgs)?, Unit::default())),
    }
}

/// Smallest counted unit per 1: 100 for cents.
fn per_unit(cur: &Unit) -> f64 {
    10f64.powi(cur.0.first().map_or(2, |(c, _)| places(&c.name)))
}

fn per_yr() -> Result<Unit, Fail> {
    Ok(unit("yr")?.pow(-1))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn pay() {
        assert_eq!(show("$25/h to USD/workyr"), "$52,000.00/workyr");
        assert_eq!(show("$1200 / ($40/h) to workday"), "3.75 workday");
        assert_eq!(show("salary($25/h).year"), "$52,000.00");
        assert_eq!(show("salary(78000 USD/yr, {hours: 37.5}).hour"), "$40.00");
        assert_eq!(show("$25/h * 2080 h == 52000 USD"), "true");
        assert!(try_eval("salary($25/h, {hour: 30})").is_err());
    }

    #[test]
    fn growth() {
        assert_eq!(show("grow($10000, 7%/yr, 30 yr)"), "$76,122.55");
        assert_eq!(show("grow($0, 12%/yr, 1 yr, $100/mo)"), "$1,268.25");
        assert_eq!(show(r#"grow(1000, 5%/yr, 1 yr, "continuous")"#), "1051.27");
        assert_eq!(show("cagr($1000, $2000, 10 yr)"), "7.17735%/yr");
        assert_eq!(show(r#"apy(12%/yr, "monthly")"#), "12.6825%/yr");
        assert_eq!(show("doubling(1%/mo)"), "5.80506 yr");
        assert_eq!(show("real_rate(7%, 3%)"), "3.8835%/yr");
    }

    #[test]
    fn loans() {
        assert_eq!(show("payment($400000, 6.5%/yr, 30 yr)"), "$2,528.27/mo");
        assert_eq!(show("payment(1200, 0%/yr, 1 yr)"), "100 1/mo");
        assert_eq!(show("payoff($5000, 22%/yr, $200/mo)"), "34 mo");
        assert_eq!(show("amortize($400000, 6.5%/yr, 30 yr).len"), "360");
        assert_eq!(show("amortize($400000, 6.5%/yr, 30 yr)[-1].balance"), "$0.00");
        assert_eq!(show("amortize($1000, 12%/yr, 3 mo)[0]"), "{n: 1, payment: $340.02, interest: $10.00, principal: $330.02, balance: $669.98}");
        assert_eq!(show("amortize($400000, 6.5%/yr, 30 yr, {extra: $300/mo}).len"), "269");
        assert!(try_eval("payoff($5000, 22%/yr, $50/mo)").is_err());
    }

    #[test]
    fn splitting() {
        assert_eq!(show("share($100, 3)"), "[$33.34, $33.33, $33.33]");
        assert_eq!(show("share($100, [2, 1, 1])"), "[$50.00, $25.00, $25.00]");
        assert_eq!(show("share(-1, 3)"), "[-0.34, -0.33, -0.33]");
        assert_eq!(show("settle({ana: $120, ben: $0, cy: $30})"), r#"["ben pays ana $50.00", "cy pays ana $20.00"]"#);
        assert_eq!(show("change($80, $100)"), "0.25");
        assert_eq!(show("[margin(60, 100), markup(60, 100)]"), "[0.4, 0.666667]");
    }
}
