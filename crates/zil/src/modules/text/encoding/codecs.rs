//! Text codecs for `encode` and `decode`: base62, base85, ascii85, base91, uuencode, quoted-printable and punycode.

const B62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const B85: &[u8; 85] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
const B91: &[u8; 91] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&()*+,./:;<=>?@[]^_`{|}~\"";

/// `b` as one big number in base `alphabet.len()`, with each leading zero byte as the alphabet's first character, as base58 does.
pub fn radix(b: &[u8], alphabet: &[u8]) -> String {
    let zeros = b.iter().take_while(|&&x| x == 0).count();
    let rest = &b[zeros..];
    let digits = if rest.is_empty() { vec![] } else { num_bigint::BigUint::from_bytes_be(rest).to_radix_be(alphabet.len() as u32) };
    (alphabet[0] as char).to_string().repeat(zeros) + &digits.iter().map(|&d| alphabet[d as usize] as char).collect::<String>()
}

pub fn unradix(s: &str, alphabet: &[u8]) -> Option<Vec<u8>> {
    let zeros = s.bytes().take_while(|&c| c == alphabet[0]).count();
    let digits: Vec<u8> = s[zeros..].bytes().map(|c| alphabet.iter().position(|&a| a == c).map(|d| d as u8)).collect::<Option<_>>()?;
    let rest = if digits.is_empty() { vec![] } else { num_bigint::BigUint::from_radix_be(&digits, alphabet.len() as u32)?.to_bytes_be() };
    Some([vec![0; zeros], rest].concat())
}

pub fn base62(b: &[u8]) -> String {
    radix(b, B62)
}

pub fn unbase62(s: &str) -> Option<Vec<u8>> {
    unradix(s, B62)
}

/// Each 4 bytes as 5 base-85 digits; a short last group drops as many digits as it lacks bytes. Ascii85 writes an all-zero group as `z`.
fn b85(b: &[u8], digit: impl Fn(u32) -> char, z: bool) -> String {
    let mut out = String::new();
    for chunk in b.chunks(4) {
        let mut n = [0; 4];
        n[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from_be_bytes(n);
        if z && n == 0 && chunk.len() == 4 {
            out.push('z');
            continue;
        }
        out.extend((0..5).rev().map(|i| digit(n / 85u32.pow(i) % 85)).take(chunk.len() + 1));
    }
    out
}

fn unb85(s: &[u8], value: impl Fn(u8) -> Option<u32>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for group in s.chunks(5) {
        let mut n: u64 = 0;
        for i in 0..5 {
            n = n * 85 + group.get(i).map_or(Some(84), |&c| value(c))? as u64;
        }
        out.extend_from_slice(&u32::try_from(n).ok()?.to_be_bytes()[..group.len() - 1]);
    }
    Some(out)
}

/// RFC 1924's alphabet, as Python's `b85encode` and git use.
pub fn base85(b: &[u8]) -> String {
    b85(b, |d| B85[d as usize] as char, false)
}

pub fn unbase85(s: &str) -> Option<Vec<u8>> {
    let s: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if s.len() % 5 == 1 {
        return None;
    }
    unb85(&s, |c| B85.iter().position(|&a| a == c).map(|d| d as u32))
}

/// Adobe's Ascii85, without the `<~ ~>` delimiters.
pub fn ascii85(b: &[u8]) -> String {
    b85(b, |d| (b'!' + d as u8) as char, true)
}

/// The `<~ ~>` delimiters are optional.
pub fn unascii85(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    let s = s.strip_prefix("<~").unwrap_or(s);
    let s = s.strip_suffix("~>").unwrap_or(s);
    let s: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).flat_map(|c| if c == b'z' { b"!!!!!".to_vec() } else { vec![c] }).collect();
    if s.len() % 5 == 1 {
        return None;
    }
    unb85(&s, |c| (b'!'..=b'u').contains(&c).then(|| (c - b'!') as u32))
}

/// Joachim Henke's basE91: 13 or 14 bits at a time as two characters.
pub fn base91(b: &[u8]) -> String {
    let (mut out, mut acc, mut bits) = (String::new(), 0u32, 0);
    for &x in b {
        acc |= (x as u32) << bits;
        bits += 8;
        if bits > 13 {
            let mut v = acc & 8191;
            if v > 88 {
                (acc, bits) = (acc >> 13, bits - 13);
            } else {
                v = acc & 16383;
                (acc, bits) = (acc >> 14, bits - 14);
            }
            out.extend([B91[(v % 91) as usize] as char, B91[(v / 91) as usize] as char]);
        }
    }
    if bits > 0 {
        out.push(B91[(acc % 91) as usize] as char);
        if bits > 7 || acc > 90 {
            out.push(B91[(acc / 91) as usize] as char);
        }
    }
    out
}

pub fn unbase91(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut acc, mut bits, mut v) = (Vec::new(), 0u32, 0, None);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let d = B91.iter().position(|&a| a == c)? as u32;
        let Some(lo) = v.take() else {
            v = Some(d);
            continue;
        };
        let n = lo + d * 91;
        acc |= n << bits;
        bits += if n & 8191 > 88 { 13 } else { 14 };
        while bits > 7 {
            out.push(acc as u8);
            (acc, bits) = (acc >> 8, bits - 8);
        }
    }
    if let Some(lo) = v {
        out.push((acc | lo << bits) as u8);
    }
    Some(out)
}

/// A `begin 644 data` block of 45-byte lines, as the `uuencode` command writes.
pub fn uuencode(b: &[u8]) -> String {
    let ch = |n: u8| if n == 0 { '`' } else { (b' ' + n) as char };
    let mut out = String::from("begin 644 data\n");
    for line in b.chunks(45) {
        out.push(ch(line.len() as u8));
        for g in line.chunks(3) {
            let n = u32::from_be_bytes([0, g[0], *g.get(1).unwrap_or(&0), *g.get(2).unwrap_or(&0)]);
            out.extend((0..4).rev().map(|i| ch((n >> (6 * i) & 63) as u8)));
        }
        out.push('\n');
    }
    out + "`\nend"
}

/// The `begin` and `end` lines are optional.
pub fn unuuencode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for line in s.lines().map(str::trim_end).filter(|l| !l.is_empty() && !l.starts_with("begin ") && *l != "end") {
        let b: Vec<u8> = line.bytes().map(|c| c.wrapping_sub(b' ') & 63).collect();
        let len = b[0] as usize;
        let data: Vec<u8> = b[1..]
            .chunks(4)
            .flat_map(|g| {
                let n = g.iter().chain([0, 0, 0].iter()).take(4).fold(0u32, |n, &d| n << 6 | d as u32);
                n.to_be_bytes()[1..].to_vec()
            })
            .collect();
        out.extend(data.get(..len)?);
    }
    Some(out)
}

/// RFC 2045: bytes other than printable ASCII as `=XX`, with soft `=` line breaks keeping lines to 76 characters.
pub fn quoted_printable(b: &[u8]) -> String {
    let mut out = String::new();
    for (i, line) in b.split(|&c| c == b'\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut col = 0;
        for (j, &c) in line.iter().enumerate() {
            let last = j + 1 == line.len();
            let lit = matches!(c, b'!'..=b'<' | b'>'..=b'~') || (matches!(c, b' ' | b'\t') && !last);
            let tok = if lit { (c as char).to_string() } else { format!("={c:02X}") };
            if col + tok.len() > if last { 76 } else { 75 } {
                out.push_str("=\n");
                col = 0;
            }
            col += tok.len();
            out.push_str(&tok);
        }
    }
    out
}

pub fn unquoted_printable(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] != b'=' {
            out.push(b[i]);
            i += 1;
        } else if b[i + 1..].starts_with(b"\r\n") {
            i += 3;
        } else if b[i + 1..].starts_with(b"\n") {
            i += 2;
        } else {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        }
    }
    Some(out)
}

const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;

fn adapt(delta: u32, points: u32, first: bool) -> u32 {
    let mut delta = if first { delta / 700 } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > (BASE - TMIN) * TMAX / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (BASE - TMIN + 1) * delta / (delta + 38)
}

fn threshold(k: u32, bias: u32) -> u32 {
    k.saturating_sub(bias).clamp(TMIN, TMAX)
}

/// RFC 3492's Bootstring for one label.
fn puny_label(label: &str) -> Option<String> {
    let cs: Vec<u32> = label.chars().map(|c| c as u32).collect();
    let mut out: String = label.chars().filter(char::is_ascii).collect();
    let (b, mut h) = (out.len() as u32, out.len() as u32);
    if b > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias) = (128u32, 0u32, 72);
    let digit = |d: u32| (if d < 26 { b'a' + d as u8 } else { b'0' + d as u8 - 26 }) as char;
    while (h as usize) < cs.len() {
        let m = cs.iter().copied().filter(|&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(h + 1)?)?;
        n = m;
        for &c in &cs {
            if c < n {
                delta = delta.checked_add(1)?;
            }
            if c == n {
                let (mut q, mut k) = (delta, BASE);
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, h + 1, h == b);
                (delta, h) = (0, h + 1);
            }
        }
        (delta, n) = (delta + 1, n + 1);
    }
    Some(out)
}

fn unpuny_label(s: &str) -> Option<String> {
    let (basic, rest) = s.rsplit_once('-').unwrap_or(("", s));
    let mut out: Vec<char> = basic.chars().collect();
    if !basic.is_ascii() {
        return None;
    }
    let digits: Vec<u32> = rest
        .bytes()
        .map(|c| match c.to_ascii_lowercase() {
            c @ b'a'..=b'z' => Some((c - b'a') as u32),
            c @ b'0'..=b'9' => Some((c - b'0') as u32 + 26),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let (mut n, mut i, mut bias, mut pos) = (128u32, 0u32, 72, 0);
    while pos < digits.len() {
        let (old, mut w, mut k) = (i, 1u32, BASE);
        loop {
            let d = *digits.get(pos)?;
            pos += 1;
            i = i.checked_add(d.checked_mul(w)?)?;
            let t = threshold(k, bias);
            if d < t {
                break;
            }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let len = out.len() as u32 + 1;
        bias = adapt(i - old, len, old == 0);
        n = n.checked_add(i / len)?;
        i %= len;
        out.insert(i as usize, char::from_u32(n)?);
        i += 1;
    }
    Some(out.into_iter().collect())
}

/// A domain name with each non-ASCII label as `xn--` and its punycode, as browsers send it to DNS.
pub fn punycode(s: &str) -> Option<String> {
    let labels: Option<Vec<String>> =
        s.split('.').map(|l| if l.is_ascii() { Some(l.to_string()) } else { puny_label(l).map(|p| format!("xn--{p}")) }).collect();
    Some(labels?.join("."))
}

/// Labels without `xn--` pass through; a bare punycode string without dots or `xn--` is decoded as one label.
pub fn unpunycode(s: &str) -> Option<String> {
    let label = |l: &str| match l.get(..4) {
        Some(p) if p.eq_ignore_ascii_case("xn--") => unpuny_label(&l[4..]),
        _ => Some(l.to_string()),
    };
    if !s.contains('.') && !s.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("xn--")) {
        return unpuny_label(s);
    }
    Some(s.split('.').map(label).collect::<Option<Vec<_>>>()?.join("."))
}
