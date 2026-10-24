//! Developer tools: JWTs, URLs, semver, file permissions; networks, IDs, colors and check digits below.

pub mod binary;
pub mod codes;
pub mod colors;
pub mod ids;
pub mod net;
pub mod packing;

use crate::ast::Radix;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::text::encoding::{url_decode, url_encode};
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;
use base64::Engine;
use indexmap::IndexMap;
use jiff::Timestamp;
use std::cmp::Ordering;

pub const MODULE: Module = Module {
    name: "dev",
    about: "JWTs, URLs, semver, file permissions; networks, IDs, colors and codes below",
    #[rustfmt::skip]
    examples: &[
        ("dev", &[
            ("what's in this token?", r#"jwt("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJpYXQiOjE1MTYyMzkwMjJ9.c2ln").payload"#),
            ("query string of a URL", r#"url_parse("https://example.com/search?q=zil&page=2").query"#),
            ("is 1.10 newer than 1.9?", r#"semver("1.10.0") > "1.9.3""#),
            ("chmod 640 means", "chmod(640)"),
        ]),
    ],
    fns: FNS,
    call,
    compare: Some(compare),
    children: &[net::MODULE, ids::MODULE, colors::MODULE, codes::MODULE, packing::MODULE, binary::MODULE],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("jwt", "jwt(s: str)", "decode a JWT into {header, payload, signature} without verifying it; exp, iat and nbf become dates",
        &[r#"jwt("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJleHAiOjE5MjQ5OTIwMDB9.c2ln").payload.exp"#], &["decode", "hmac"]),
    doc("url_parse", "url_parse(s: str)", "split a URL into {scheme, host, port, path, query, fragment}; the query becomes a map, and repeated keys a list",
        &[r#"url_parse("https://example.com:8080/a/b?x=1&y=two%20words#top")"#], &["url_build", "encode"]),
    doc("url_build", "url_build(m: map)", "the URL for a map like the one url_parse makes; missing parts are left out",
        &[r#"url_build({scheme: "https", host: "example.com", path: "/search", query: {q: "a b"}})"#], &["url_parse"]),
    doc("semver", "semver(s: str)", "parse a semantic version into {major, minor, patch, pre, build}; compares by version precedence, also against strings",
        &[r#"semver("1.2.3") < semver("1.10.0")"#, r#"semver("1.0.0-beta") < "1.0.0""#, r#"semver("v2.1.0-rc.1+build5")"#], &["bump"]),
    doc("bump", "bump(v: str|map, part: str)", "next \"major\", \"minor\" or \"patch\" version; a pre-release bumps to its own release",
        &[r#"bump("1.2.3", "minor")"#, r#"bump("2.0.0-rc.1", "major")"#], &["semver"]),
    doc("chmod", "chmod(mode: int|str)", "Unix permissions between octal and rwx: 755 or 0o755 gives \"rwxr-xr-x\", and back",
        &["chmod(755)", r#"chmod("rw-r--r--")"#, "chmod(0o4755)"], &[]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    Ok(match (name, args) {
        ("jwt", [Value::Str(s)]) => jwt(s).map_err(|e| Fail::Arg(0, e.into()))?,
        ("url_parse", [Value::Str(s)]) => url_parse(s),
        ("url_build", [Value::Map(m)]) => Value::str(url_build(&m.borrow())),
        ("semver", [Value::Str(s)]) => Semver::parse(s).ok_or_else(|| Fail::Arg(0, not_semver(s)))?.value(),
        ("bump", [v @ (Value::Str(_) | Value::Map(_)), Value::Str(part)]) => {
            let s = Semver::of(v).ok_or_else(|| Fail::Arg(0, not_semver(&v.to_string())))?;
            let b = s.bump(part).ok_or_else(|| Fail::Arg(1, format!("unknown part {part:?}\nnote: parts are major, minor and patch")))?;
            if matches!(v, Value::Str(_)) { Value::str(b.to_string()) } else { b.value() }
        }
        ("chmod", [Value::Int(n, r)]) => {
            let mode = if r.base == 8 { Some(*n) } else { i64::from_str_radix(&n.to_string(), 8).ok() };
            Value::str(perm_str(mode.filter(|m| (0..=0o7777).contains(m)).ok_or_else(|| Fail::Arg(0, format!("`{n}` is not an octal mode like 755")))?))
        }
        ("chmod", [Value::Str(s)]) => {
            let mode = i64::from_str_radix(s, 8).ok().or_else(|| perm_mode(s)).filter(|m| (0..=0o7777).contains(m));
            Value::Int(mode.ok_or_else(|| Fail::Arg(0, format!("`{s}` is not a mode like \"rwxr-xr-x\" or \"755\"")))?, Radix { base: 8, width: 0 })
        }
        _ => return Err(Fail::BadArgs),
    })
}

fn jwt(s: &str) -> Result<Value, &'static str> {
    let [header, payload, signature] = s.trim().split('.').collect::<Vec<_>>()[..] else {
        return Err("expected a JWT: three base64url parts separated by `.`");
    };
    let part = |p: &str| -> Result<Value, &'static str> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).map_err(|_| "invalid base64url in JWT")?;
        Ok(super::fs::from_json(serde_json::from_slice(&bytes).map_err(|_| "JWT part is not JSON")?))
    };
    let payload = part(payload)?;
    if let Value::Map(m) = &payload {
        for (k, v) in m.borrow_mut().iter_mut() {
            if let ("exp" | "iat" | "nbf", Value::Int(n, _)) = (k.as_str(), &*v)
                && let Ok(ts) = Timestamp::from_second(*n)
            {
                *v = Value::date(ts.to_zoned(crate::host::tz()));
            }
        }
    }
    let fields = [("header", part(header)?), ("payload", payload), ("signature", Value::str(signature))];
    Ok(Value::map(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
}

fn url_parse(s: &str) -> Value {
    let (rest, fragment) = s.split_once('#').map_or((s, None), |(a, b)| (a, Some(b)));
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (scheme, rest) = match rest.split_once(':') {
        Some((sc, r)) if sc.starts_with(|c: char| c.is_ascii_alphabetic()) && sc.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) => {
            (Some(sc), r)
        }
        _ => (None, rest),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(r) => r.find('/').map_or((Some(r), ""), |i| (Some(&r[..i]), &r[i..])),
        None => (None, rest),
    };
    let (user, hostport) = match authority.map(|a| a.rsplit_once('@')) {
        Some(Some((u, h))) => (Some(u), Some(h)),
        _ => (None, authority),
    };
    // `[::1]:8080` keeps the brackets' colons out of the port split.
    let (host, port) = match hostport.map(|h| (h, h.rfind(':').filter(|&i| !h[i..].contains(']')))) {
        Some((h, Some(i))) => (Some(&h[..i]), h[i + 1..].parse::<i64>().ok()),
        Some((h, None)) => (Some(h), None),
        None => (None, None),
    };
    let dec = |x: &str| String::from_utf8_lossy(&url_decode(x).unwrap_or_else(|| x.into())).into_owned();
    let mut q: IndexMap<String, Value> = IndexMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = Value::str(dec(v));
        match q.get_mut(&dec(k)) {
            Some(Value::List(l)) => l.borrow_mut().push(v),
            Some(old) => *old = Value::list(vec![old.clone(), v]),
            None => _ = q.insert(dec(k), v),
        }
    }
    let opt = |x: Option<&str>| x.map_or(Value::Nil, Value::str);
    let mut m = IndexMap::from([("scheme".to_string(), opt(scheme))]);
    if user.is_some() {
        m.insert("user".into(), opt(user));
    }
    let rest =
        [("host", opt(host)), ("port", port.map_or(Value::Nil, Value::int)), ("path", Value::str(path)), ("query", Value::map(q)), ("fragment", opt(fragment))];
    m.extend(rest.map(|(k, v)| (k.to_string(), v)));
    Value::map(m)
}

fn url_build(m: &IndexMap<String, Value>) -> String {
    let part = |k: &str| m.get(k).filter(|v| !matches!(v, Value::Nil)).map(Value::to_string);
    let mut s = String::new();
    if let Some(sc) = part("scheme") {
        s += &format!("{sc}:");
    }
    if let Some(h) = part("host") {
        s += "//";
        if let Some(u) = part("user") {
            s += &format!("{u}@");
        }
        s += &h;
        if let Some(p) = part("port") {
            s += &format!(":{p}");
        }
    }
    s += &part("path").unwrap_or_default();
    if let Some(Value::Map(q)) = m.get("query")
        && !q.borrow().is_empty()
    {
        let mut pairs = vec![];
        for (k, v) in q.borrow().iter() {
            let vs = match v {
                Value::List(l) => l.borrow().clone(),
                v => vec![v.clone()],
            };
            pairs.extend(vs.iter().map(|v| format!("{}={}", url_encode(k), url_encode(&v.to_string()))));
        }
        s += &format!("?{}", pairs.join("&"));
    }
    if let Some(f) = part("fragment") {
        s += &format!("#{f}");
    }
    s
}

struct Semver {
    nums: [i64; 3],
    pre: String,
    build: String,
}

const SEMVER_KEYS: [&str; 5] = ["major", "minor", "patch", "pre", "build"];

fn not_semver(s: &str) -> String {
    format!("`{s}` is not a semantic version\nnote: versions look like 1.2.3, 1.2.3-beta.1 or 1.2.3+build")
}

impl Semver {
    fn parse(s: &str) -> Option<Semver> {
        let s = s.trim().trim_start_matches('v');
        let (s, build) = s.split_once('+').unwrap_or((s, ""));
        let (s, pre) = s.split_once('-').unwrap_or((s, ""));
        let nums: Vec<i64> = s.split('.').map(|n| n.parse().ok().filter(|_| n.bytes().all(|b| b.is_ascii_digit()))).collect::<Option<_>>()?;
        Some(Semver { nums: nums.try_into().ok()?, pre: pre.into(), build: build.into() })
    }

    /// From a version string or a map `semver` made.
    fn of(v: &Value) -> Option<Semver> {
        match v {
            Value::Str(s) => Semver::parse(s),
            Value::Map(m) if m.borrow().keys().eq(SEMVER_KEYS) => {
                let m = m.borrow();
                let num = |k| if let Some(Value::Int(n, _)) = m.get(k) { Some(*n) } else { None };
                let text = |k| m.get(k).map(Value::to_string);
                Some(Semver { nums: [num("major")?, num("minor")?, num("patch")?], pre: text("pre")?, build: text("build")? })
            }
            _ => None,
        }
    }

    fn value(&self) -> Value {
        let [a, b, c] = self.nums.map(Value::int);
        let vals = [a, b, c, Value::str(&*self.pre), Value::str(&*self.build)];
        Value::map(SEMVER_KEYS.iter().map(|k| k.to_string()).zip(vals).collect())
    }

    /// npm's rule: a pre-release of exactly the next version bumps to that release.
    fn bump(&self, part: &str) -> Option<Semver> {
        let i = ["major", "minor", "patch"].iter().position(|&p| p == part)?;
        let mut nums = self.nums;
        if self.pre.is_empty() || nums[i + 1..].iter().any(|&n| n != 0) {
            nums[i] += 1;
            nums[i + 1..].fill(0);
        }
        Some(Semver { nums, pre: String::new(), build: String::new() })
    }

    /// Precedence: numbers, then a pre-release sorts before its release; build metadata is ignored.
    fn cmp(&self, o: &Semver) -> Ordering {
        // Ok (numeric) sorts before Err (alphanumeric), as semver wants.
        let ids = |p: &str| p.split('.').map(|id| id.parse::<u64>().map_err(|_| id.to_string())).collect::<Vec<_>>();
        self.nums.cmp(&o.nums).then_with(|| match (self.pre.is_empty(), o.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => ids(&self.pre).cmp(&ids(&o.pre)),
        })
    }
}

impl std::fmt::Display for Semver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let [a, b, c] = self.nums;
        write!(f, "{a}.{b}.{c}")?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build)?;
        }
        Ok(())
    }
}

/// Orders a `semver` map against another or a version string.
fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    if !matches!((a, b), (Value::Map(_), _) | (_, Value::Map(_))) {
        return None;
    }
    Some(Semver::of(a)?.cmp(&Semver::of(b)?))
}

const PERM: [(char, i64); 3] = [('r', 4), ('w', 2), ('x', 1)];

fn perm_str(mode: i64) -> String {
    let mut s: Vec<char> = (0..9).map(|i| if mode >> (8 - i) & 1 == 1 { PERM[i % 3].0 } else { '-' }).collect();
    // setuid, setgid and sticky show in the x slots: lowercase when x is set too.
    for (bit, at, c) in [(0o4000, 2, 's'), (0o2000, 5, 's'), (0o1000, 8, 't')] {
        if mode & bit != 0 {
            s[at] = if s[at] == 'x' { c } else { c.to_ascii_uppercase() };
        }
    }
    s.into_iter().collect()
}

fn perm_mode(s: &str) -> Option<i64> {
    // `ls -l` puts the file type first: `drwxr-xr-x`.
    let s: Vec<char> = s.chars().collect();
    let s = if s.len() == 10 { &s[1..] } else { &s[..] };
    if s.len() != 9 {
        return None;
    }
    let mut mode = 0;
    for (i, &c) in s.iter().enumerate() {
        let (want, bit) = PERM[i % 3];
        let x = 1 << (3 * (2 - i / 3));
        mode |= match (c, i) {
            ('-', _) => 0,
            _ if c == want => bit << (3 * (2 - i / 3)),
            ('s' | 'S', 2 | 5) => (if i == 2 { 0o4000 } else { 0o2000 }) | if c == 's' { x } else { 0 },
            ('t' | 'T', 8) => 0o1000 | if c == 't' { x } else { 0 },
            _ => return None,
        };
    }
    Some(mode)
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn jwt() {
        let t = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert_eq!(show(&format!(r#"jwt("{t}").header"#)), r#"{alg: "HS256", typ: "JWT"}"#);
        assert_eq!(show(&format!(r#"jwt("{t}").payload.name"#)), "John Doe");
        assert_eq!(show(&format!(r#"jwt("{t}").payload.iat.unix"#)), "1516239022");
        assert_eq!(show(&format!(r#"jwt("{t}").signature"#)), "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c");
        assert!(try_eval(r#"jwt("abc")"#).is_err());
    }

    #[test]
    fn url() {
        let u = r#"url_parse("https://bob@example.com:8080/a/b?x=1&y=two%20words&x=2#top")"#;
        assert_eq!(
            show(u),
            r#"{scheme: "https", user: "bob", host: "example.com", port: 8080, path: "/a/b", query: {x: ["1", "2"], y: "two words"}, fragment: "top"}"#
        );
        assert_eq!(show(&format!("url_build({u})")), "https://bob@example.com:8080/a/b?x=1&x=2&y=two%20words#top");
        assert_eq!(show(r#"url_parse("http://[::1]:80/").host"#), "[::1]");
        assert_eq!(show(r#"url_parse("mailto:a@b.c").path"#), "a@b.c");
        assert_eq!(show(r#"url_build(url_parse("mailto:a@b.c"))"#), "mailto:a@b.c");
    }

    #[test]
    fn semver() {
        assert_eq!(show(r#"semver("1.2.3") < semver("1.10.0")"#), "true");
        assert_eq!(show(r#"semver("1.0.0-alpha") < "1.0.0-alpha.1""#), "true");
        assert_eq!(show(r#"semver("1.0.0-alpha.beta") < "1.0.0-beta.2""#), "true");
        assert_eq!(show(r#"semver("1.0.0-beta.11") < "1.0.0-rc.1""#), "true");
        assert_eq!(show(r#"semver("1.0.0-rc.1") < "1.0.0""#), "true");
        assert_eq!(show(r#"bump("1.2.3", "minor")"#), "1.3.0");
        assert_eq!(show(r#"bump("1.2.3-rc.1", "patch")"#), "1.2.3");
        assert_eq!(show(r#"bump("1.2.3-rc.1", "major")"#), "2.0.0");
        assert_eq!(show(r#"bump(semver("1.2.3"), "major").major"#), "2");
        assert!(try_eval(r#"semver("1.2")"#).is_err());
    }

    #[test]
    fn chmod() {
        assert_eq!(show("chmod(755)"), "rwxr-xr-x");
        assert_eq!(show("chmod(0o644)"), "rw-r--r--");
        assert_eq!(show("chmod(4755)"), "rwsr-xr-x");
        assert_eq!(show("chmod(1777)"), "rwxrwxrwt");
        assert_eq!(show(r#"chmod("rwxr-xr-x")"#), "0o755");
        assert_eq!(show(r#"chmod("drwsr-Sr-T")"#), "0o7744");
        assert_eq!(show("chmod(chmod(640))"), "0o640");
        assert!(try_eval("chmod(789)").is_err());
    }
}
