//! Places: countries and cities from an embedded GeoNames extract (CC-BY 4.0, geonames.org): about 250
//! countries and 1300 cities (capitals and everything over 500k people). Time zones, currencies and the
//! sky look places up here, so `5pm to "Seattle"`, `$100 to "Japan"` and `sunset("Oslo")` work. Zip codes are
//! looked up online through Zippopotam.us (zippopotam.us) and cached.

use crate::ast::Target;
use crate::error::did_you_mean;
use crate::modules::units::{self, MONEY};
use crate::modules::{Claim, Module};
use crate::value::{Table, Value, num};
use indexmap::IndexMap;
use std::sync::LazyLock;

pub const MODULE: Module = Module {
    name: "geo",
    about: "facts about countries and cities, and distances and directions between places",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Built-in facts about the world's countries and cities, and distances and directions between places.\n\n\
The functions here look up countries by name or by their two- or three-letter ISO code, and cities by name. Short \
names and missing accents are fine. When several cities share a name the biggest one wins, so add a country or state \
after a comma to pick another, like `\"Portland, US\"`.\n\n\
Where a function takes a place, give a city or country name (a country means its capital), or a map with lat and lon."),
        ("good to know", "\
A place's name also works with `to` as a currency, so `$100 to \"Vietnam\"` gives dong, and as a time zone."),
        ("data", "\
City and country data comes from GeoNames (geonames.org, CC-BY 4.0) and covers capitals and cities of over 500,000 \
people."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("geo", &[
            ("currency of a country", r#"country("Vietnam").currency"#),
            ("time in any city", r#"date("2026-12-25 18:30") to "Seattle""#),
            ("flying distance from New York to London", r#"great_circle("New York", "London") to mi"#),
            ("which compass direction Mecca is from London", r#"bearing("London", "Mecca")"#),
            ("sunset in a city", r#"sunset("Oslo", date(2026, 6, 21))"#),
            ("how many countries use the euro", r#"countries().filter(|c| c.currency == "EUR").len"#),
            ("biggest cities in Japan", r#"cities().filter(|c| c.country == "Japan").map(|c| c.name).take(3)"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("names", &[
            ("countries by name or ISO code", r#"country("JPN").name"#),
            ("short names and missing accents work", r#"[city("NYC").name, city("Sao Paulo").name]"#),
            ("add a country or state to pick another", r#"city("Hyderabad, PK").country"#),
        ]),
        ("places", &[
            ("a place can also be a map with lat and lon", r#"great_circle("Paris", {lat: 0, lon: 0})"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    convert: Some(convert),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{CITIES, COUNTRIES, Place, bearing_deg, city_map, country_map, find_city, find_country, haversine, table, unknown, zipcode as lookup};
    use crate::modules::{Fail, units};
    use crate::signature::{As, Map, Qty};
    use crate::value::{Table, Value};

    /// facts about a country, found by name or ISO code
    ///
    /// Its name, two- and three-letter codes, capital, currency, time zones, phone calling code, population, area, continent, flag, languages and neighbours
    #[builtin(examples = [r#"country("Japan")"#, r#"country("UK").currency"#], see = ["city", "countries"])]
    fn country(name: &str) -> Result<As<Map>, Fail> {
        Ok(country_map(find_country(name).ok_or_else(|| Fail::Arg(0, unknown(name)))?)?.into())
    }

    /// facts about a city
    ///
    /// Its name, country, state or region (admin), population, latitude and longitude, and time zone
    #[builtin(examples = [r#"city("Tokyo")"#, r#"city("Seattle").zone"#], see = ["country", "cities", "nearest"])]
    fn city(name: &str) -> Result<As<Map>, Fail> {
        Ok(city_map(find_city(name).ok_or_else(|| Fail::Arg(0, unknown(name)))?).into())
    }

    /// every country, as a table with the same columns as country gives
    #[builtin(examples = ["countries().len", r#"countries().filter(|c| c.continent == "Oceania").map(|c| c.name).take(3)"#], see = ["country", "cities"])]
    fn countries() -> Result<As<Table>, String> {
        Ok(table(COUNTRIES.iter().map(country_map).collect::<Result<_, _>>()?).into())
    }

    /// every city, as a table with the same columns as city gives, biggest first
    #[builtin(examples = ["cities()[0].name"], see = ["city", "countries"])]
    fn cities() -> As<Table> {
        table(CITIES.iter().map(city_map).collect()).into()
    }

    type LatLon = (f64, f64);

    fn ends(from: &Place, to: &Place) -> Result<(LatLon, LatLon), Fail> {
        let a = from.coords().map_err(|m| Fail::Arg(0, m))?;
        let b = to.coords().map_err(|m| Fail::Arg(1, m))?;
        Ok(((a.0, a.1), (b.0, b.1)))
    }

    /// the distance between two places along the Earth's surface, the shortest route, which is roughly the one planes fly
    #[builtin(examples = [r#"great_circle("Tokyo", "Paris")"#, r#"great_circle("NYC", "LA") to mi"#], see = ["bearing", "nearest"])]
    fn great_circle(from: Place, to: Place) -> Result<As<Qty>, Fail> {
        let (a, b) = ends(&from, &to)?;
        Ok(Value::qty(haversine(a, b), units::unit("km")?).into())
    }

    /// the compass direction to set off in from one place to reach another, in degrees clockwise from north (90 is east)
    #[builtin(examples = [r#"bearing("NYC", "London")"#], see = ["great_circle"])]
    fn bearing(from: Place, to: Place) -> Result<As<Qty>, Fail> {
        let (a, b) = ends(&from, &to)?;
        Ok(Value::qty(bearing_deg(a, b), units::unit("deg")?).into())
    }

    /// the city closest to a latitude and longitude, out of those zil knows
    #[builtin(examples = ["nearest(48.9, 2.3).name"], see = ["city", "great_circle"])]
    fn nearest(lat: f64, lon: f64) -> As<Map> {
        let p = (lat, lon);
        city_map(CITIES.iter().min_by(|a, b| haversine(p, (a.lat, a.lon)).total_cmp(&haversine(p, (b.lat, b.lon)))).expect("cities")).into()
    }

    /// look up a postal code online
    ///
    /// Codes looked up before work offline. Gives its place name, state, country, latitude and longitude. The result works as a place for great_circle
    #[builtin(
        shown = [r#"zipcode("90210").place"#, r#"zipcode("10115", "Germany")"#, r#"great_circle(zipcode("10001"), zipcode("94103")) to mi"#],
        see = ["city"]
    )]
    fn zipcode(code: &str, #[default("US")] country: &str) -> Result<As<Map>, Fail> {
        let c = find_country(country).ok_or_else(|| Fail::Arg(1, unknown(country)))?;
        Ok(lookup(code, c.code).map_err(|m| Fail::Arg(0, m))?.into())
    }
}

pub struct Country {
    pub code: &'static str,
    code3: &'static str,
    pub name: &'static str,
    capital: &'static str,
    area: f64,
    population: i64,
    continent: &'static str,
    pub currency: &'static str,
    calling: &'static str,
    languages: &'static str,
    neighbours: &'static str,
    /// Biggest first, the capital's first of all.
    pub zones: Vec<&'static str>,
}

pub struct City {
    pub name: &'static str,
    ascii: &'static str,
    pub country: &'static str,
    admin_code: &'static str,
    admin: &'static str,
    population: i64,
    pub lat: f64,
    pub lon: f64,
    pub zone: &'static str,
}

fn rows(tsv: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    tsv.lines().filter(|l| !l.starts_with('#')).map(|l| l.split('\t').collect())
}

static COUNTRIES: LazyLock<Vec<Country>> = LazyLock::new(|| {
    rows(include_str!("geo/countries.tsv"))
        .map(|f| Country {
            code: f[0],
            code3: f[1],
            name: f[2],
            capital: f[3],
            area: f[4].parse().unwrap_or(0.0),
            population: f[5].parse().unwrap_or(0),
            continent: f[6],
            currency: f[7],
            calling: f[8],
            languages: f[9],
            neighbours: f[10],
            zones: f[11].split(' ').filter(|z| !z.is_empty()).collect(),
        })
        .collect()
});

/// Biggest first, so the first match of a name is the one people mean.
static CITIES: LazyLock<Vec<City>> = LazyLock::new(|| {
    rows(include_str!("geo/cities.tsv"))
        .map(|f| City {
            name: f[0],
            ascii: f[1],
            country: f[2],
            admin_code: f[3],
            admin: f[4],
            population: f[5].parse().unwrap_or(0),
            lat: f[6].parse().unwrap_or(0.0),
            lon: f[7].parse().unwrap_or(0.0),
            zone: f[8],
        })
        .collect()
});

/// Every city's name, biggest first.
pub fn city_names() -> impl Iterator<Item = &'static str> {
    CITIES.iter().map(|c| c.name)
}

#[rustfmt::skip]
const COUNTRY_ALIASES: &[(&str, &str)] = &[
    ("america", "US"), ("united states of america", "US"), ("the united states", "US"),
    ("uk", "GB"), ("britain", "GB"), ("great britain", "GB"), ("england", "GB"), ("scotland", "GB"), ("wales", "GB"), ("northern ireland", "GB"),
    ("uae", "AE"), ("emirates", "AE"), ("korea", "KR"), ("netherlands", "NL"), ("holland", "NL"), ("czech republic", "CZ"),
    ("cote d'ivoire", "CI"), ("burma", "MM"), ("turkiye", "TR"), ("vatican city", "VA"), ("drc", "CD"), ("congo", "CD"),
];

#[rustfmt::skip]
const CITY_ALIASES: &[(&str, &str)] = &[
    ("nyc", "New York City"), ("new york", "New York City"), ("la", "Los Angeles"), ("sf", "San Francisco"),
    ("dc", "Washington, US"), ("washington dc", "Washington, US"), ("washington, dc", "Washington, US"),
    ("bombay", "Mumbai"), ("calcutta", "Kolkata"), ("madras", "Chennai"), ("saigon", "Ho Chi Minh City"), ("hcmc", "Ho Chi Minh City"),
    ("kiev", "Kyiv"), ("peking", "Beijing"), ("rio", "Rio de Janeiro"), ("cdmx", "Mexico City"), ("mecca", "Makkah"),
];

fn alias(aliases: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    aliases.iter().find(|a| a.0 == key).map(|a| a.1)
}

fn key(s: &str) -> String {
    s.trim().to_lowercase()
}

pub fn find_country(name: &str) -> Option<&'static Country> {
    let k = key(name);
    let code = alias(COUNTRY_ALIASES, &k);
    COUNTRIES.iter().find(|c| code.map_or(c.code.eq_ignore_ascii_case(&k) || c.code3.eq_ignore_ascii_case(&k) || key(c.name) == k, |code| c.code == code))
}

/// `"Portland"` or `"Portland, US"`: the qualifier is a country (name or code) or a state/province (name or code).
pub fn find_city(name: &str) -> Option<&'static City> {
    let k = key(name);
    let k = alias(CITY_ALIASES, &k).map_or(k, key);
    let (base, qual) = match k.rsplit_once(',') {
        Some((b, q)) => (b.trim(), Some(q.trim())),
        None => (k.as_str(), None),
    };
    let fits = |c: &City| match qual {
        None => true,
        Some(q) => [c.admin_code, c.admin].iter().any(|a| key(a) == q) || find_country(q).is_some_and(|k| k.code == c.country),
    };
    CITIES.iter().find(|c| (key(c.name) == base || key(c.ascii) == base) && fits(c))
}

fn country_of(c: &City) -> &'static Country {
    COUNTRIES.iter().find(|k| k.code == c.country).expect("every city's country is listed")
}

fn capital(k: &Country) -> Option<&'static City> {
    CITIES.iter().find(|c| c.country == k.code && c.name == k.capital)
}

/// A city by name, else a country's capital.
pub fn find_place(name: &str) -> Option<&'static City> {
    find_city(name).or_else(|| find_country(name).and_then(capital))
}

pub fn unknown(name: &str) -> String {
    let names = COUNTRIES.iter().map(|c| c.name).chain(CITIES.iter().map(|c| c.ascii));
    let lower: Vec<String> = names.map(key).collect();
    format!("unknown city or country {name:?}{}", did_you_mean(&key(name), lower.iter().map(String::as_str)))
}

/// A city's time zone, or a country's capital's (or its biggest zone if the capital isn't listed).
pub fn zone_of(name: &str) -> Option<&'static str> {
    find_city(name).map(|c| c.zone).or_else(|| find_country(name).and_then(|k| k.zones.first().copied()))
}

/// A country's currency, or the currency of a city's country.
fn currency_of(name: &str) -> Result<&'static str, String> {
    let k = find_country(name).or_else(|| find_city(name).map(country_of)).ok_or_else(|| unknown(name))?;
    Some(k.currency).filter(|c| !c.is_empty()).ok_or_else(|| format!("{} has no currency", k.name))
}

/// A place argument: a city name, or a map with lat and lon, read by `coords`.
pub struct Place(Value);

impl<'a> crate::signature::Arg<'a> for Place {
    const TYPE: &'static str = "str|map";
    fn of(v: &'a Value) -> Option<Self> {
        matches!(v, Value::Str(_) | Value::Map(_)).then(|| Place(v.clone()))
    }
}

impl Place {
    pub fn coords(&self) -> Result<(f64, f64, Option<&'static str>), String> {
        place(&self.0)
    }
}

/// `(lat, lon, zone)` of a place argument: a name, or a map with `lat` and `lon` (and no zone).
pub fn place(v: &Value) -> Result<(f64, f64, Option<&'static str>), String> {
    match v {
        Value::Str(s) => find_place(s).map(|c| (c.lat, c.lon, Some(c.zone))).ok_or_else(|| unknown(s)),
        Value::Map(m) => {
            let m = m.borrow();
            let get = |k: &str| m.get(k).and_then(num).ok_or_else(|| format!("a place map needs numeric `lat` and `lon`, missing `{k}`"));
            Ok((get("lat")?, get("lon")?, None))
        }
        v => Err(format!("a place is a name or a map with lat and lon, got {}", v.type_name())),
    }
}

const RADIUS_KM: f64 = 6371.0088;

// ponytail: spherical earth, off by up to ~0.5% against the ellipsoid; Vincenty if survey accuracy ever matters.
fn haversine((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let (p1, p2, dl) = (lat1.to_radians(), lat2.to_radians(), (lon2 - lon1).to_radians());
    let h = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * RADIUS_KM * h.sqrt().asin()
}

fn bearing_deg((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let (p1, p2, dl) = (lat1.to_radians(), lat2.to_radians(), (lon2 - lon1).to_radians());
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

fn strs(s: &str, sep: char) -> Value {
    Value::list(s.split(sep).filter(|x| !x.is_empty()).map(Value::str).collect())
}

fn flag(code: &str) -> String {
    code.bytes().filter_map(|b| char::from_u32(0x1F1E6 + u32::from(b.wrapping_sub(b'A')))).collect()
}

fn continent(code: &str) -> &'static str {
    match code {
        "AF" => "Africa",
        "AN" => "Antarctica",
        "AS" => "Asia",
        "EU" => "Europe",
        "NA" => "North America",
        "OC" => "Oceania",
        "SA" => "South America",
        _ => "",
    }
}

fn country_map(k: &Country) -> Result<Value, String> {
    let s = |x: &str| if x.is_empty() { Value::Nil } else { Value::str(x) };
    let m = IndexMap::from([
        ("name".into(), Value::str(k.name)),
        ("code".into(), Value::str(k.code)),
        ("code3".into(), Value::str(k.code3)),
        ("capital".into(), s(k.capital)),
        ("currency".into(), s(k.currency)),
        ("zones".into(), Value::list(k.zones.iter().map(|z| Value::str(*z)).collect())),
        ("calling".into(), if k.calling.is_empty() { Value::Nil } else { Value::str(format!("+{}", k.calling)) }),
        ("population".into(), Value::int(k.population)),
        ("area".into(), Value::qty(k.area, units::terms("km^2")?)),
        ("continent".into(), Value::str(continent(k.continent))),
        ("flag".into(), Value::str(flag(k.code))),
        ("languages".into(), strs(k.languages, ',')),
        ("neighbours".into(), strs(k.neighbours, ',')),
    ]);
    Ok(Value::map(m))
}

fn city_map(c: &City) -> Value {
    Value::map(IndexMap::from([
        ("name".into(), Value::str(c.name)),
        ("country".into(), Value::str(country_of(c).name)),
        ("admin".into(), Value::str(c.admin)),
        ("population".into(), Value::int(c.population)),
        ("lat".into(), Value::Float(c.lat)),
        ("lon".into(), Value::Float(c.lon)),
        ("zone".into(), Value::str(c.zone)),
    ]))
}

fn table(maps: Vec<Value>) -> Value {
    Value::table(Table::from_maps(&maps).expect("all maps, few cells"))
}

/// A postal code's first place, from the cache or Zippopotam.us; `country` is an ISO code.
fn zipcode(code: &str, country: &str) -> Result<Value, String> {
    let code = code.trim();
    let body = crate::modules::sys::cached("zipcodes.json", &format!("{country}|{code}"), || {
        let url = format!("https://api.zippopotam.us/{country}/{}", code.replace([' ', '/', '?', '#'], ""));
        crate::modules::sys::get(&url, &[]).map_err(|e| if e.contains("404") { format!("unknown zip code {code:?} in {country}") } else { e })
    })?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let p = &json["places"][0];
    let s = |v: &serde_json::Value| Value::str(v.as_str().unwrap_or_default());
    let f = |v: &serde_json::Value| v.as_str().and_then(|x| x.parse().ok()).map(Value::Float).ok_or("unexpected response");
    Ok(Value::map(IndexMap::from([
        ("code".into(), s(&json["post code"])),
        ("place".into(), s(&p["place name"])),
        ("state".into(), s(&p["state"])),
        ("country".into(), s(&json["country"])),
        ("lat".into(), f(&p["latitude"])?),
        ("lon".into(), f(&p["longitude"])?),
    ])))
}

/// `$100 to "Japan"`: money into a place's currency.
fn convert(v: &Value, t: &Target) -> Claim {
    let (Value::Qty(x, u), Target::Str(name)) = (v, t) else {
        return None;
    };
    if u.dim() != MONEY {
        return None;
    }
    Some(currency_of(name).and_then(units::unit).and_then(|c| units::to_unit(*x, u, c)))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    fn err(src: &str) -> String {
        let e = try_eval(src).expect_err("an error");
        format!(
            "{}
{}",
            e.msg,
            e.help.join(
                "
"
            )
        )
    }

    #[test]
    fn lookups() {
        assert_eq!(show(r#"[country("JP").name, country("jpn").currency, country("USA").code, country("Britain").code]"#), r#"["Japan", "JPY", "US", "GB"]"#);
        assert_eq!(show(r#"country("France").flag"#), "🇫🇷");
        assert_eq!(
            show(r#"[city("Sao Paulo").name, city("NYC").country, city("Portland").admin, city("Hyderabad, PK").country]"#),
            r#"["São Paulo", "United States", "Oregon", "Pakistan"]"#
        );
        assert!(err(r#"city("Toyko")"#).contains("`tokyo`"));
        assert_eq!(show(r#"nearest(48.9, 2.3).name"#), "Paris");
        assert_eq!(show(r#"countries().filter(|c| c.code == "DE").len"#), "1");
    }

    #[test]
    fn distances() {
        // London to New York is about 5570 km.
        assert_eq!(show(r#"round(great_circle("London", "NYC"))"#), "5570 km");
        assert_eq!(show(r#"round(bearing({lat: 0, lon: 0}, {lat: 10, lon: 0}))"#), "0 deg");
        assert_eq!(show(r#"round(bearing({lat: 0, lon: 0}, {lat: 0, lon: 10}))"#), "90 deg");
        // A country means its capital.
        assert_eq!(show(r#"great_circle("Japan", "Tokyo")"#), "0 km");
    }

    #[test]
    fn integrations() {
        assert_eq!(show(r#"(date("2026-12-25T12:00Z") to "Seattle").format("%H:%M %Z")"#), "04:00 PST");
        assert_eq!(show(r#"(date("2026-12-25T12:00Z") to "India").format("%H:%M")"#), "17:30");
        assert!(err(r#"$1 to "Antarctica""#).contains("has no currency"));
        assert!(err(r#"$1 to "Narnia""#).contains("unknown city or country"));
    }

    #[test]
    fn zipcodes_offline() {
        assert!(err(r#"zipcode("zil-test-99999")"#).contains("network access is disabled"));
        assert!(err(r#"zipcode("10115", "Narnia")"#).contains("unknown city or country"));
    }
}
