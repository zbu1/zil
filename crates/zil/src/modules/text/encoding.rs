//! Encodings: base64/32/58, URL and hex, code points, UTF-8 bytes, hex dumps and entropy.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;
use base64::Engine;

pub const MODULE: Module = Module {
    name: "encoding",
    about: "text as bytes and back: base64, URL and hex encodings, Unicode code points, hex dumps",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Text as bytes and back. Every character has a number called its Unicode code point (A is 65, é is 233), and UTF-8, the \
encoding almost everything uses, stores that number in 1 to 4 bytes. So a string's length in characters and in bytes \
can differ: \"é\" is one character but two bytes."),
        ("writing", "\
The bytes type, written `b\"...\"`, holds raw bytes that aren't necessarily text. Inside, `\\x` and two hex digits \
gives any byte. Indexing gives a byte's value as an integer, `+` joins bytes and `*` repeats them. `.str` decodes \
UTF-8 bytes into a string and `.hex` shows them as hex digits. Bytes longer than 32 are shown as a hex dump."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("encoding", &[
            ("base64", r#""hi" to base64"#),
            ("URL encoding", r#""a b&c".encode("url")"#),
            ("base58, as in Bitcoin", r#""hello".encode("base58")"#),
            ("UTF-8 bytes", r#""héllo".bytes"#),
            ("the character for code point 9731", "chr(9731)"),
            ("an emoji is 4 bytes but 1 character", r#"["😀".byte_len, "😀".len]"#),
            ("every byte of a string", r#"hexdump("héllo\tworld\n")"#),
            ("how random is it? 0 is not at all", r#""aaaaaaaa".entropy"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("bytes", r#"b"\x7fELF\n""#)]),
        ("bytes", &[
            ("b[i] is one byte's value, b[a..b] a slice of bytes", r#"b"\x7fELF"[1..]"#),
            ("+ joins bytes, * repeats them", r#"b"ab" + b"\xff" * 2"#),
            (".hex shows bytes as hex digits", r#"b"hi".hex"#),
        ]),
        ("conversions", &[
            ("to base64, to base32, to base58, to hex or to url encodes", r#""hi" to base64"#),
            ("to unicode shows code points", r#""é" to unicode"#),
        ]),
    ],
    fns: FNS,
    call,
    targets: &[("base64", "base64"), ("base32", "base32"), ("base58", "base58"), ("url", "url_encode"), ("unicode", "unicode")],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("base64", "base64(s: str) -> str", "encode a string's UTF-8 bytes in base64, the most common way to send binary data as plain text, in emails, URLs or JSON", &[r#"base64("hi there")"#, r#""hi" to base64"#], &["encode", "decode"])
        .detail("Same as `s to base64`"),
    doc("base32", "base32(s: str) -> str", "encode a string's UTF-8 bytes in base32, which avoids mixing upper and lower case", &[r#""hi" to base32"#], &["encode", "decode"])
        .detail("Same as `s to base32`"),
    doc("base58", "base58(s: str) -> str", "encode a string's UTF-8 bytes in base58, which avoids look-alike characters such as 0 and O, as Bitcoin addresses do", &[r#""hi" to base58"#], &["encode", "decode"])
        .detail("Same as `s to base58`"),
    doc("url_encode", "url_encode(s: str) -> str", "make a string safe to put in a URL, replacing other characters with % and their hex code, like %20 for a space", &[r#""a b&c" to url"#], &["encode", "decode", "url_build"])
        .detail("Same as `s to url`"),
    doc("unicode", "unicode(v: str|int) -> str", "each character's code point in the standard U+ form, like U+00E9 for é", &[r#""é" to unicode"#, "233 to unicode"], &["ord", "chr"])
        .detail("Same as `v to unicode`"),
    doc("encode", "encode(s: str, fmt: str) -> str", "encode a string's UTF-8 bytes in the named format: \"base64\", \"base32\", \"base58\", \"url\" or \"hex\"", &[r#""hi there".encode("base64")"#, r#""a b&c".encode("url")"#, r#""hi".encode("base32")"#], &["decode"]),
    doc("decode", "decode(s: str, fmt: str) -> str|bytes", "decode text in the named format (\"base64\", \"base32\", \"base58\", \"url\" or \"hex\"), the reverse of encode", &[r#""aGk=".decode("base64")"#, r#""6869".decode("hex")"#, r#""Cn8eVZg".decode("base58")"#], &["encode"])
        .detail("If the result isn't valid text, it comes back as bytes"),
    doc("ord", "ord(c: str) -> int", "the Unicode code point (number) of a single character", &[r#""A".ord"#, r#""A".ord to hex"#], &["chr", "bytes"]),
    doc("chr", "chr(n: int) -> str", "the character with the given Unicode code point", &["97.chr", "(65..70).map(chr).join"], &["ord"]),
    doc("bytes", "bytes(v: str|list|bytes) -> bytes", "a string's UTF-8 bytes, or bytes made from a list of integers 0-255", &[r#""hé".bytes"#, "[0x7f, 69].bytes", r#""hi".bytes.str"#], &["from_bytes", "read_bytes", "ord"])
        .detail("`.str` turns UTF-8 bytes back into a string"),
    doc("from_bytes", "from_bytes(v: list|bytes) -> str", "a string from UTF-8 bytes, given as bytes or as a list of integers", &["[104, 105].from_bytes"], &["bytes", "chr"]),
    doc("byte_len", "byte_len(v: str|list|bytes) -> int", "how many bytes the text takes in UTF-8, rather than how many characters it has", &[r#""héllo".byte_len"#], &["len", "bytes"]),
    doc("hexdump", "hexdump(v: str|list|bytes) -> str", "bytes shown as a hex dump, like the xxd tool", &[r#"hexdump("hi\n")"#, "hexdump([0, 255, 65])"], &["bytes", "entropy"])
        .detail("Each line has the offset, 16 bytes in hex, and the same bytes as text with dots for unprintable ones"),
    doc("xxd", "xxd(v: str|list|bytes) -> str", "the same as hexdump", &[r#"xxd(b"\x7fELF")"#], &["hexdump", "read_bytes"])
        .detail("As a command, `xxd a.out` dumps a file"),
    doc("strings", "strings(v: bytes|str, min?: int) -> list", "the readable text hidden in binary data: every run of at least min printable characters (4 by default), like the Unix tool", &[r#"b"\x00\x01hello\xffab\x00world!".strings"#, r#"strings(b"ab\x00cd", 2)"#], &["xxd", "read_bytes"])
        .detail("As a command, `strings a.out` reads a file"),
    doc("entropy", "entropy(v: str|list|bytes) -> float", "how unpredictable the bytes are (Shannon entropy)", &[r#""aaaa".entropy"#, r#""abcd".entropy"#], &["hexdump"])
        .detail("It goes from 0 bits per byte when every byte is the same to 8 for random or encrypted data"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let (args, _) = crate::modules::flags(name, args)?;
    Ok(match (name, args) {
        ("base64", [Str(s)]) => Value::str(base64::engine::general_purpose::STANDARD.encode(s.as_bytes())),
        ("base32", [Str(s)]) => Value::str(base32(s.as_bytes())),
        ("base58", [Str(s)]) => Value::str(base58(s.as_bytes())),
        ("url_encode", [Str(s)]) => Value::str(url_encode(s)),
        ("unicode", [Str(s)]) => Value::str(s.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>().join(" ")),
        ("unicode", [Int(n, _)]) => match u32::try_from(*n).ok().and_then(char::from_u32) {
            Some(c) => Value::str(format!("U+{:04X}", c as u32)),
            None => return Err(Fail::Arg(0, format!("`{n}` is not a Unicode code point"))),
        },
        ("encode", [Str(s), Str(fmt)]) => Value::str(match &**fmt {
            "base64" => base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
            "base32" => base32(s.as_bytes()),
            "base58" => base58(s.as_bytes()),
            "url" => url_encode(s),
            "hex" => hex(s.as_bytes()),
            _ => return Err(Fail::Arg(1, unknown_encoding(fmt))),
        }),
        ("decode", [Str(s), Str(fmt)]) => {
            let bytes = match &**fmt {
                "base64" => base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| e.to_string())?,
                "base32" => unbase32(s.trim()).ok_or("invalid base32")?,
                "base58" => unbase58(s.trim()).ok_or("invalid base58")?,
                "url" => url_decode(s).ok_or("invalid percent-encoding")?,
                "hex" => unhex(s.trim()).ok_or("invalid hex")?,
                _ => return Err(Fail::Arg(1, unknown_encoding(fmt))),
            };
            match String::from_utf8(bytes) {
                Ok(s) => Value::str(s),
                Err(e) => Value::bytes(e.into_bytes()),
            }
        }
        ("ord", [Str(s)]) => match s.chars().collect::<Vec<_>>()[..] {
            [c] => Value::int(c as i64),
            ref cs => return Err(Fail::Arg(0, format!("expected a single character, got {}", cs.len()))),
        },
        ("chr", [Int(n, _)]) => {
            let c = u32::try_from(*n).ok().and_then(char::from_u32);
            Value::str(
                c.ok_or_else(|| Fail::Arg(0, format!("`{n}` is not a Unicode code point\nnote: code points go from 0 to 0x10ffff, skipping 0xd800-0xdfff")))?
                    .to_string(),
            )
        }
        ("bytes", [v]) => Value::bytes(bytes(v).ok_or_else(|| Fail::Arg(0, "expected a list of integers 0-255".into()))?),
        ("from_bytes", [v @ (List(_) | Bytes(_))]) => {
            let bytes = bytes(v).ok_or("expected a list of integers 0-255")?;
            Value::str(utf8(bytes)?)
        }
        ("byte_len", [v]) => Value::int(bytes(v).ok_or(Fail::BadArgs)?.len() as i64),
        ("strings", [v @ (Bytes(_) | Str(_))]) => strings(&bytes(v).unwrap(), 4),
        ("strings", [v @ (Bytes(_) | Str(_)), Int(n, _)]) if *n >= 1 => strings(&bytes(v).unwrap(), *n as usize),
        ("hexdump" | "xxd", [v]) => Value::str(hexdump(&bytes(v).ok_or(Fail::BadArgs)?)),
        ("entropy", [v]) if bytes(v).is_some() => {
            let b = bytes(v).unwrap();
            let mut counts = [0usize; 256];
            b.iter().for_each(|&x| counts[x as usize] += 1);
            let n = b.len() as f64;
            // `+ 0.0` turns the -0 from a single repeated byte into 0.
            Value::Float(counts.iter().filter(|&&c| c > 0).map(|&c| -(c as f64 / n) * (c as f64 / n).log2()).sum::<f64>() + 0.0)
        }
        _ => return Err(Fail::BadArgs),
    })
}

fn unknown_encoding(fmt: &str) -> String {
    let hint = crate::error::did_you_mean(fmt, ["base64", "base32", "base58", "url", "hex"]);
    if hint.is_empty() {
        format!("unknown encoding {fmt:?}\nnote: encodings are base64, base32, base58, url and hex")
    } else {
        format!("unknown encoding {fmt:?}{hint}")
    }
}

/// Runs of at least `min` printable ASCII characters (tabs count), as `strings` finds them in binaries.
fn strings(b: &[u8], min: usize) -> Value {
    let runs = b.split(|c| !(c.is_ascii_graphic() || *c == b' ' || *c == b'\t')).filter(|r| r.len() >= min);
    Value::list(runs.map(|r| Value::str(String::from_utf8_lossy(r))).collect())
}

/// Bytes as text, naming where they stop being UTF-8.
pub fn utf8(b: Vec<u8>) -> Result<String, String> {
    String::from_utf8(b).map_err(|e| format!("not UTF-8 at byte {}", e.utf8_error().valid_up_to()))
}

/// Bytes, a string's UTF-8 bytes, or a list of ints 0-255.
pub fn bytes(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Bytes(b) => Some(b.to_vec()),
        Value::Str(s) => Some(s.as_bytes().to_vec()),
        Value::List(l) => l.borrow().iter().map(|x| if let Value::Int(n, _) = x { u8::try_from(*n).ok() } else { None }).collect(),
        _ => None,
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// RFC 4648 base32, padded with `=` to a multiple of 8.
fn base32(b: &[u8]) -> String {
    let (mut out, mut acc, mut bits) = (String::new(), 0u32, 0);
    for &x in b {
        (acc, bits) = ((acc << 8 | x as u32) & 0xfff, bits + 8);
        while bits >= 5 {
            bits -= 5;
            out.push(B32[(acc >> bits & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[(acc << (5 - bits) & 31) as usize] as char);
    }
    while !out.len().is_multiple_of(8) {
        out.push('=');
    }
    out
}

/// Case-insensitive; padding is optional.
fn unbase32(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        let v = B32.iter().position(|&a| a == c.to_ascii_uppercase())? as u32;
        (acc, bits) = ((acc << 5 | v) & 0xfff, bits + 5);
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Bitcoin's alphabet; each leading zero byte becomes a `1`.
fn base58(b: &[u8]) -> String {
    let zeros = b.iter().take_while(|&&x| x == 0).count();
    let rest = &b[zeros..];
    let digits = if rest.is_empty() { vec![] } else { num_bigint::BigUint::from_bytes_be(rest).to_radix_be(58) };
    "1".repeat(zeros) + &digits.iter().map(|&d| B58[d as usize] as char).collect::<String>()
}

fn unbase58(s: &str) -> Option<Vec<u8>> {
    let zeros = s.bytes().take_while(|&c| c == b'1').count();
    let digits: Vec<u8> = s[zeros..].bytes().map(|c| B58.iter().position(|&a| a == c).map(|d| d as u8)).collect::<Option<_>>()?;
    let rest = if digits.is_empty() { vec![] } else { num_bigint::BigUint::from_radix_be(&digits, 58)?.to_bytes_be() };
    Some([vec![0; zeros], rest].concat())
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn url_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(out)
}

/// 16 bytes per line: offset, hex in pairs, printable ASCII.
pub fn hexdump(b: &[u8]) -> String {
    let mut out = Vec::new();
    for (i, row) in b.chunks(16).enumerate() {
        let hex: Vec<String> = row.chunks(2).map(|p| p.iter().map(|x| format!("{x:02x}")).collect()).collect();
        let ascii: String = row.iter().map(|&x| if x.is_ascii_graphic() || x == b' ' { x as char } else { '.' }).collect();
        out.push(format!("{:08x}: {:<39}  {ascii}", i * 16, hex.join(" ")));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn encodings() {
        assert_eq!(show(r#""hi there".encode("base64")"#), "aGkgdGhlcmU=");
        assert_eq!(show(r#""aGkgdGhlcmU=".decode("base64")"#), "hi there");
        assert_eq!(show(r#""a b&c".encode("url")"#), "a%20b%26c");
        assert_eq!(show(r#""a%20b%26c".decode("url")"#), "a b&c");
        assert_eq!(show(r#""hi".encode("hex").decode("hex")"#), "hi");
        assert_eq!(show(r#""A".ord"#), "65");
        assert_eq!(show("0x1f600.chr"), "😀");
        assert_eq!(show(r#""hé".bytes"#), r#"b"h\xc3\xa9""#);
        assert_eq!(show(r#""hé".bytes.from_bytes"#), "hé");
        assert_eq!(show(r#""héllo".byte_len"#), "6");
        assert!(try_eval(r#""ab".ord"#).is_err());
        assert_eq!(show(r#""foobar".encode("base32")"#), "MZXW6YTBOI======");
        assert_eq!(show(r#""fo".encode("base32")"#), "MZXQ====");
        assert_eq!(show(r#""mzxw6ytboi".decode("base32")"#), "foobar");
        assert_eq!(show(r#""hello world".encode("base58")"#), "StV1DL6CwTryKyV");
        assert_eq!(show(r#""StV1DL6CwTryKyV".decode("base58")"#), "hello world");
        assert_eq!(show(r#""11".decode("base58").bytes"#), r#"b"\0\0""#);
        assert_eq!(show(r#""/w==".decode("base64")"#), r#"b"\xff""#);
        assert!(try_eval(r#""0OIl".decode("base58")"#).is_err());
        assert!(try_eval("[256].from_bytes").is_err());
        assert_eq!(show(r#""foobar" to base32"#), "MZXW6YTBOI======");
        assert_eq!(show(r#""hello world" to base58"#), "StV1DL6CwTryKyV");
        assert_eq!(show(r#""a b&c" to url"#), "a%20b%26c");
        assert_eq!(show(r#""hi" to hex"#), "6869");
        assert_eq!(show(r#""hi" to unicode"#), "U+0068 U+0069");
        assert_eq!(show("0x1f600 to unicode"), "U+1F600");
        assert!(try_eval("-1 to unicode").is_err());
    }

    #[test]
    fn binary() {
        assert_eq!(show(r#""hi\n".hexdump"#), format!("00000000: {:<39}  hi.", "6869 0a"));
        assert_eq!(show("hexdump([])"), "");
        assert_eq!(show(r#""aaaa".entropy"#), "0");
        assert_eq!(show(r#""abcd".entropy"#), "2");
    }

    #[test]
    fn bytes() {
        assert_eq!(show(r#"b"\x7fELF\n\"\{\x00\xFF""#), r#"b"\x7fELF\n\"\{\0\xff""#);
        assert_eq!(
            show(r#"b = b"\x7fELF"; [b[0], b[-1], b[1..], b.len, b.hex, b to hex, type(b), b"é".len]"#),
            r#"[127, 70, b"ELF", 4, "7f454c46", "7f454c46", "bytes", 2]"#
        );
        assert_eq!(
            show(r#"[b"ab" + b"c", b"ab" * 2, 2 * b"\0", b"a" == b"a", b"a" == "a", b"a" < b"b", set(b"a", b"a").len]"#),
            r#"[b"abc", b"abab", b"\0\0", true, false, true, 1]"#
        );
        assert_eq!(show(r#"[[0x7f, 69].bytes, "hi".bytes.str, b"hi".from_bytes, [*b"AB"]]"#), r#"[b"\x7fE", "hi", "hi", [65, 66]]"#);
        assert_eq!(show(r#"t = 0; for x in b"\x01\x02" { t += x }; t"#), "3");
        let zeros = format!("{}  ................", ["0000"; 8].join(" "));
        assert_eq!(show(r#"b"\0" * 33"#), format!("00000000: {zeros}\n00000010: {zeros}\n00000020: {:<39}  .", "00"));
        assert!(show(r#"b"a" * 300"#).ends_with("\n... 300 bytes in all"));
        assert!(try_eval(r#"b"\xc3".str"#).unwrap_err().msg.contains("not UTF-8 at byte 0"));
        for bad in [r#""a" + b"b""#, r#"b"a" + 1"#, r#"b"\x4""#, r#"b"\q""#, "[256].bytes", r#"b"a"[1]"#] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }
}
