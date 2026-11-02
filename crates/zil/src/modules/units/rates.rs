//! Currencies: the common codes known offline, and exchange rates loaded once from cache or open.er-api.com.

use super::{MONEY, UnitDef, registry};
use crate::state;
use std::cell::Cell;
use std::rc::Rc;

/// Common currencies, known without fetching, so `25 USD/h * 40 h` stays offline.
pub const CURRENCIES: &str = "AUD BGN BRL CAD CHF CNY CZK DKK EUR GBP HKD HUF IDR ILS INR ISK JPY KRW MXN MYR NOK NZD PHP PLN RON SEK SGD THB TRY USD ZAR";

pub fn currency(code: &str, scale: f64) -> Rc<UnitDef> {
    Rc::new(UnitDef { name: code.into(), scale: Cell::new(scale), offset: 0.0, dim: MONEY })
}

pub fn rate_error() -> String {
    state::with(|s| format!("cannot load currency rates: {}", s.rates_error.borrow()))
}

/// Fills in every currency's rate, once; on failure their scales stay NaN and `rate_error` says why. Never while
/// completion probes: a fetch could block a Tab, and a failure there would stick.
pub fn load_rates_once() {
    if state::with(|s| s.probing.get() || s.rates_loaded.replace(true)) {
        return;
    }
    apply_rates(load_rates(false));
}

/// Fetches the rates now, whatever the cache holds, and returns how many currencies they cover.
pub fn refresh_rates() -> Result<usize, String> {
    state::with(|s| s.rates_loaded.set(true));
    let rates = load_rates(true);
    let n = rates.as_ref().map_or(0, Vec::len);
    apply_rates(rates);
    if n == 0 { Err(rate_error()) } else { Ok(n) }
}

fn apply_rates(rates: Result<Vec<(String, f64)>, String>) {
    state::with(|s| s.rates_error.borrow_mut().clear());
    match rates {
        Ok(rates) => registry(|r| {
            let mut r = r.borrow_mut();
            for (code, per_eur) in rates {
                match r.get(&code) {
                    Some(def) => def.scale.set(1.0 / per_eur),
                    None => {
                        let def = currency(&code, 1.0 / per_eur);
                        r.entry(code.to_lowercase()).or_insert(def.clone());
                        r.insert(code, def);
                    }
                }
            }
            // A cent is a hundredth of a dollar, so it follows the dollar's rate.
            if let (Some(cent), Some(usd)) = (r.get("cent"), r.get("USD")) {
                cent.scale.set(usd.scale.get() / 100.0);
            }
        }),
        Err(e) => state::with(|s| *s.rates_error.borrow_mut() = e),
    }
}

fn fetch_rates() -> Result<String, String> {
    crate::host::get().http_get("https://open.er-api.com/v6/latest/EUR", &[])
}

/// Units per 1 EUR from open.er-api.com (about 160 currencies, updated daily), kept in the host's cache for `rates_hours`.
fn load_rates(force: bool) -> Result<Vec<(String, f64)>, String> {
    let host = crate::host::get();
    let cache = host.cache_get("er-rates.json");
    let age = cache.as_ref().map(|(_, at)| host.now().duration_since(*at).as_secs());
    let max_age = state::with(|s| s.settings.borrow().rates_hours) * 3600;
    let body = match cache {
        Some((body, _)) if !force && age.is_some_and(|a| a < max_age) => body,
        _ => match fetch_rates() {
            Ok(body) => {
                host.cache_put("er-rates.json", &body);
                body
            }
            // Offline: fall back to a stale cache if there is one.
            Err(e) => match cache {
                Some((body, _)) if !force => {
                    let days = age.map_or(0, |a| a.max(0) / 86400);
                    host.note(&format!("note: using currency rates cached {days} day{} ago", if days == 1 { "" } else { "s" }));
                    body
                }
                _ => return Err(e),
            },
        },
    };
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let mut rates: Vec<(String, f64)> =
        json["rates"].as_object().ok_or("unexpected response")?.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect();
    rates.push(("EUR".into(), 1.0));
    Ok(rates)
}
