//! Encodings: base64/32/58/62/85/91, Ascii85, uuencode, quoted-printable, punycode, URL and hex, code points, UTF-8 bytes, hex dumps and entropy.

use crate::ansi::{RESET, Role};
use crate::modules::{Fail, Module};
use crate::signature::Or;
use crate::value::Value;
use std::ops::Range;

mod codecs;

pub const MODULE: Module = Module {
    name: "encoding",
    tags: &["encoding"],
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
            ("an internationalized domain name as DNS sees it", r#""münchen.de".encode("punycode")"#),
            ("bytes written out in binary", r#"from_byte_text("01101000 01101001")"#),
            ("UTF-8 bytes", r#""héllo".bytes"#),
            ("the character for code point 9731", "chr(9731)"),
            ("an emoji is 4 bytes but 1 character", r#"["😀".byte_len, "😀".len]"#),
            ("every byte of a string", r#"hexdump("héllo\tworld\n")"#),
            ("how random is it? 0 is not at all", r#""aaaaaaaa".entropy"#),
        ]),
    ],
    #[rustfmt::skip]
    make: &[
        ("bytes", r#"b"\x7fELF\n""#),
    ],
    syntax: &[
        ("b[i] is one byte's value, b[a..b] a slice of bytes", r#"b"\x7fELF"[1..]"#),
        ("+ joins bytes, * repeats them", r#"b"ab" + b"\xff" * 2"#),
        (".hex shows bytes as hex digits", r#"b"hi".hex"#),
        ("to base64, to base32, to base58, to hex or to url encodes", r#""hi" to base64"#),
        ("to unicode shows code points", r#""é" to unicode"#),
    ],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("base64", "base64"), ("base32", "base32"), ("base58", "base58"), ("url", "url_encode"), ("unicode", "unicode")],
    ..Module::EMPTY
};

/// Bytes as a str (its UTF-8), a list of ints 0-255, or bytes.
pub struct Data(pub Value);

impl<'a> crate::signature::Arg<'a> for Data {
    const TYPE: &'static str = "str|list|bytes";
    fn of(v: &'a Value) -> Option<Self> {
        matches!(v, Value::Str(_) | Value::List(_) | Value::Bytes(_)).then(|| Data(v.clone()))
    }
}

impl Data {
    pub fn bytes(&self) -> Result<Vec<u8>, Fail> {
        bytes(&self.0).ok_or_else(|| Fail::Arg(0, "expected a list of integers 0-255".into()))
    }
}

#[zil_macros::builtins]
mod fns {
    use super::{
        Data, base_arg, base32 as b32, base58 as b58, big_endian, block_size, byte_diff, byte_runs, byte_table, bytes as to_bytes, dump_rows, hex, hex_int,
        hexdump_around, matches, number, pattern, shannon, unknown_encoding, url_encode as percent, utf8,
    };
    use super::{ENCODINGS, codecs, decoded, english, printable};
    use super::{layers, tokens};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::{List, Or};
    use crate::value::{Table, Value};
    use base64::Engine;
    use num_bigint::{BigInt, Sign};
    use std::collections::HashSet;

    /// encode a string's UTF-8 bytes in base64, the most common way to send binary data as plain text, in emails, URLs or JSON
    ///
    /// Same as `s to base64`
    #[builtin(examples = [r#"base64("hi there")"#, r#""hi" to base64"#], see = ["encode", "decode"])]
    fn base64(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
    }

    /// encode a string's UTF-8 bytes in base32, which avoids mixing upper and lower case
    ///
    /// Same as `s to base32`
    #[builtin(examples = [r#""hi" to base32"#], see = ["encode", "decode"])]
    fn base32(s: &str) -> String {
        b32(s.as_bytes())
    }

    /// encode a string's UTF-8 bytes in base58, which avoids look-alike characters such as 0 and O, as Bitcoin addresses do
    ///
    /// Same as `s to base58`
    #[builtin(examples = [r#""hi" to base58"#], see = ["encode", "decode"])]
    fn base58(s: &str) -> String {
        b58(s.as_bytes())
    }

    /// make a string safe to put in a URL, replacing other characters with % and their hex code, like %20 for a space
    ///
    /// Same as `s to url`
    #[builtin(examples = [r#""a b&c" to url"#], see = ["encode", "decode", "url_build"])]
    fn url_encode(s: &str) -> String {
        percent(s)
    }

    /// each character's code point in the standard U+ form, like U+00E9 for é
    ///
    /// Same as `v to unicode`
    #[builtin(examples = [r#""é" to unicode"#, "233 to unicode"], see = ["ord", "chr"])]
    fn unicode(v: Or<&str, i64>) -> Result<String, Fail> {
        match v {
            Or::A(s) => Ok(s.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>().join(" ")),
            Or::B(n) => match u32::try_from(n).ok().and_then(char::from_u32) {
                Some(c) => Ok(format!("U+{:04X}", c as u32)),
                None => Err(Fail::Arg(0, format!("`{n}` is not a Unicode code point"))),
            },
        }
    }

    /// encode bytes, or a string's UTF-8 bytes, in the named format
    ///
    /// The formats are "base64", "base32", "base58", "base62", "base85" (RFC 1924, as Python and git write it), "ascii85" (Adobe's), "base91", "uuencode", "quoted_printable", "punycode" (a domain name), "url" and "hex"
    #[builtin(examples = [r#""hi there".encode("base64")"#, r#""a b&c".encode("url")"#, r#"b"\xff\0".encode("ascii85")"#, r#""bücher.de".encode("punycode")"#], see = ["decode"])]
    fn encode(s: Or<&str, &[u8]>, fmt: &str) -> Result<String, Fail> {
        let b = match s {
            Or::A(s) => s.as_bytes(),
            Or::B(b) => b,
        };
        let text = || std::str::from_utf8(b).map_err(|e| Fail::Arg(0, format!("{fmt} needs text, but these bytes are not UTF-8 at byte {}", e.valid_up_to())));
        Ok(match fmt {
            "base64" => base64::engine::general_purpose::STANDARD.encode(b),
            "base32" => b32(b),
            "base58" => b58(b),
            "base62" => codecs::base62(b),
            "base85" => codecs::base85(b),
            "ascii85" => codecs::ascii85(b),
            "base91" => codecs::base91(b),
            "uuencode" => codecs::uuencode(b),
            "quoted_printable" => codecs::quoted_printable(b),
            "punycode" => codecs::punycode(text()?).ok_or_else(|| Fail::Arg(0, "too long to punycode".into()))?,
            "url" => percent(text()?),
            "hex" => hex(b),
            _ => return Err(Fail::Arg(1, unknown_encoding(fmt))),
        })
    }

    /// decode text in a format encode writes, the reverse of encode
    ///
    /// fmt is one of the formats listed under encode. If the result isn't valid text, it comes back as bytes
    #[builtin(examples = [r#""aGk=".decode("base64")"#, r#""6869".decode("hex")"#, r#""Cn8eVZg".decode("base58")"#, r#""xn--bcher-kva.de".decode("punycode")"#], see = ["encode", "from_byte_text"])]
    fn decode(s: &str, fmt: &str) -> Result<Or<String, Vec<u8>>, Fail> {
        if !ENCODINGS.contains(&fmt) {
            return Err(Fail::Arg(1, unknown_encoding(fmt)));
        }
        Ok(String::from_utf8(decoded(s, fmt)?).map_or_else(|e| Or::B(e.into_bytes()), Or::A))
    }

    /// try every format decode knows, plus rot13, bytes written as binary, octal or decimal numbers, and single-byte XOR, layer on layer up to depth deep, and list the text that comes out, the most English-looking first
    ///
    /// Returns a table of chain, the decodings in the order applied, and result. Results that aren't printable text are left out, and a result found again by a longer chain is listed once
    #[builtin(examples = [r#"decode_auto("Njg2OTIwNzQ2ODY1NzI2NQ==")[0]"#, r#"decode_auto("uryyb jbeyq").result"#], see = ["decode", "from_byte_text", "xor_solve"])]
    fn decode_auto(s: Or<&str, &[u8]>, #[default(3)] depth: i64) -> Result<Table, Fail> {
        let depth = usize::try_from(depth).ok().filter(|d| (1..=5).contains(d)).ok_or_else(|| Fail::Arg(1, format!("depth goes from 1 to 5, got {depth}")))?;
        let start = match s {
            Or::A(s) => s.as_bytes().to_vec(),
            Or::B(b) => b.to_vec(),
        };
        let mut found: Vec<(String, String)> = layers(start, depth).into_iter().filter_map(|(c, b)| Some((c, printable(&b)?))).collect();
        // Each extra layer is a little less likely, which settles short results the letter counts can't tell apart.
        let rank = |(c, r): &(String, String)| english(r.as_bytes()) - 0.1 * c.matches('→').count() as f64;
        found.sort_by(|a, b| rank(b).total_cmp(&rank(a)));
        Ok(Table { cols: vec!["chain".into(), "result".into()], rows: found.into_iter().map(|(c, r)| vec![Value::str(c), Value::str(r)]).collect() })
    }

    /// find CTF flags, prefix up to the next }, in bytes or text: as they are, and in what decode_auto's decodings make of each run of printable text, up to 3 layers deep
    ///
    /// Returns a table of offset (where the flag, or the encoded text it came from, starts), chain (the decodings, empty for a flag found as it is) and flag. Each flag is listed once. With base, offsets start at that address instead of 0, and base: true means 0x400000
    #[builtin(examples = [r#"flag_find("junk ZmxhZ3toaWRkZW59 junk")"#, r#"flag_find(b"\0\0CTF{raw}\0", "CTF\{")"#], see = ["decode_auto", "strings"])]
    fn flag_find(
        it: &mut Interp,
        b: Or<&str, &[u8]>,
        #[default("flag{")] prefix: &str,
        span: &Span,
        /// the address the first byte is at, true for 0x400000
        #[kw]
        base: Option<Or<bool, i64>>,
    ) -> Result<Table, Fail> {
        let shift = base_arg(base)?;
        if prefix.is_empty() {
            return Err(Fail::Arg(1, "the prefix is empty".into()));
        }
        let b = match b {
            Or::A(s) => s.as_bytes(),
            Or::B(b) => b,
        };
        let (mut rows, mut seen) = (vec![], HashSet::new());
        let mut find = |at: Option<usize>, chain: &str, data: &[u8]| {
            for (i, _) in data.windows(prefix.len()).enumerate().filter(|(_, w)| *w == prefix.as_bytes()) {
                // ponytail: a flag runs to the first } within 256 bytes; nested braces cut it short
                if let Some(end) = data[i..].iter().take(256).position(|&c| c == b'}') {
                    let flag = String::from_utf8_lossy(&data[i..=i + end]).into_owned();
                    if seen.insert(flag.clone()) {
                        rows.push(vec![hex_int(at.unwrap_or(i) + shift), Value::str(chain), Value::str(flag)]);
                    }
                }
            }
        };
        find(None, "", b);
        for (at, token) in tokens(b) {
            it.charge(token.len() as u64, span)?;
            for (chain, out) in layers(token.to_vec(), 3) {
                find(Some(at), &chain, &out);
            }
        }
        Ok(Table { cols: vec!["offset".into(), "chain".into(), "flag".into()], rows })
    }

    /// bytes written out as numbers separated by spaces or commas, in binary, octal, decimal or hex
    ///
    /// Without a radix, it tries each one the numbers fit and keeps the result that reads most like English. If the result isn't valid text, it comes back as bytes
    #[builtin(examples = [r#"from_byte_text("01001000 01101001")"#, r#"from_byte_text("110 151")"#, r#"from_byte_text("72, 105", 10)"#], see = ["decode", "bytes"])]
    fn from_byte_text(s: &str, radix: Option<i64>) -> Result<Or<String, Vec<u8>>, Fail> {
        let tokens: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
        let parse = |r: u32| tokens.iter().map(|t| u8::from_str_radix(t, r).ok()).collect::<Option<Vec<u8>>>();
        let bytes = match radix {
            Some(r @ (2 | 8 | 10 | 16)) => parse(r as u32).ok_or_else(|| Fail::Arg(0, format!("not bytes in base {r}")))?,
            Some(r) => return Err(Fail::Arg(1, format!("the radix is 2, 8, 10 or 16, got {r}"))),
            None => [2, 8, 10, 16]
                .into_iter()
                .filter_map(parse)
                .reduce(|a, b| if english(&b) > english(&a) { b } else { a })
                .ok_or_else(|| Fail::Arg(0, "not bytes in binary, octal, decimal or hex".into()))?,
        };
        Ok(String::from_utf8(bytes).map_or_else(|e| Or::B(e.into_bytes()), Or::A))
    }

    /// the Unicode code point (number) of a single character
    #[builtin(examples = [r#""A".ord"#, r#""A".ord to hex"#], see = ["chr", "bytes"])]
    fn ord(c: &str) -> Result<i64, Fail> {
        match c.chars().collect::<Vec<_>>()[..] {
            [c] => Ok(c as i64),
            ref cs => Err(Fail::Arg(0, format!("expected a single character, got {}", cs.len()))),
        }
    }

    /// the character with the given Unicode code point
    #[builtin(examples = ["97.chr", "(65..70).map(chr).join"], see = ["ord"])]
    fn chr(n: i64) -> Result<String, Fail> {
        let c = u32::try_from(n).ok().and_then(char::from_u32);
        Ok(c.ok_or_else(|| Fail::Arg(0, format!("`{n}` is not a Unicode code point\nnote: code points go from 0 to 0x10ffff, skipping 0xd800-0xdfff")))?
            .to_string())
    }

    /// a string's UTF-8 bytes, bytes made from a list of integers 0-255, or an integer's bytes
    ///
    /// `.str` turns UTF-8 bytes back into a string. An integer gives as few bytes as it needs, big-endian (most significant first) as cryptography writes them, or little-endian with "little", and `int` turns them back. For fixed-width little-endian values, like addresses, use p32 or p64
    #[builtin(examples = [r#""hé".bytes"#, "[0x7f, 69].bytes", r#""hi".bytes.str"#], see = ["from_bytes", "read_bytes", "ord", "int", "p64"])]
    fn bytes(v: Data) -> Result<Vec<u8>, Fail> {
        v.bytes()
    }

    #[builtin(name = "bytes", examples = ["bytes(0x414243)", r#"bytes(258, "little")"#, "bytes(2 ** 64)"])]
    fn int_bytes(n: BigInt, #[default("big")] order: &str) -> Result<Vec<u8>, Fail> {
        let (sign, b) = if big_endian(order)? { n.to_bytes_be() } else { n.to_bytes_le() };
        if sign == Sign::Minus {
            return Err(Fail::Arg(0, format!("expected 0 or more, got {n}")));
        }
        Ok(b)
    }

    /// bytes read as one unsigned integer of any length, big-endian (most significant first) unless "little"
    ///
    /// The reverse of `bytes(n)`. For fixed-width little-endian values use u32 or u64
    #[builtin(name = "int", examples = [r#"int(b"ABC")"#, r#"int(b"\x02\x01", "little")"#, r#"int("flag".bytes) to hex"#], see = ["bytes", "u64"])]
    fn bytes_int(b: &[u8], #[default("big")] order: &str) -> Result<BigInt, Fail> {
        Ok(if big_endian(order)? { BigInt::from_bytes_be(Sign::Plus, b) } else { BigInt::from_bytes_le(Sign::Plus, b) })
    }

    /// bytes padded to a multiple of size, 16 by default, with PKCS#7: n copies of the byte n, from 1 to size
    ///
    /// A block cipher like AES needs whole blocks, so a full block of padding is added when the length already fits
    #[builtin(examples = [r#"pkcs7_pad(b"YELLOW SUBMARINE", 20)"#, r#"pkcs7_pad(b"hi").len"#], see = ["pkcs7_unpad"])]
    fn pkcs7_pad(b: &[u8], size: Option<i64>) -> Result<Vec<u8>, Fail> {
        let size = block_size(size)?;
        let n = size - b.len() % size;
        Ok([b, &vec![n as u8; n]].concat())
    }

    /// bytes with their PKCS#7 padding removed, checking it's valid for blocks of size, 16 by default
    ///
    /// Bad padding is an error, the check a padding oracle attack relies on
    #[builtin(examples = [r#"pkcs7_unpad(b"YELLOW SUBMARINE\x04\x04\x04\x04", 20)"#], see = ["pkcs7_pad"])]
    fn pkcs7_unpad(b: &[u8], size: Option<i64>) -> Result<Vec<u8>, Fail> {
        let size = block_size(size)?;
        let n = b.last().map_or(0, |&n| n as usize);
        let ok = !b.is_empty() && b.len().is_multiple_of(size) && (1..=size).contains(&n) && b[b.len() - n..].iter().all(|&x| x as usize == n);
        if !ok {
            return Err(Fail::Arg(0, "bad PKCS#7 padding".into()));
        }
        Ok(b[..b.len() - n].to_vec())
    }

    /// a string from UTF-8 bytes, given as bytes or as a list of integers
    #[builtin(examples = ["[104, 105].from_bytes"], see = ["bytes", "chr"])]
    fn from_bytes(v: Or<&List, &[u8]>) -> Result<String, String> {
        let v = match v {
            Or::A(l) => Value::List(l.clone()),
            Or::B(b) => Value::bytes(b),
        };
        utf8(to_bytes(&v).ok_or("expected a list of integers 0-255")?)
    }

    /// how many bytes the text takes in UTF-8, rather than how many characters it has
    #[builtin(examples = [r#""héllo".byte_len"#], see = ["len", "bytes"])]
    fn byte_len(v: Data) -> Result<i64, Fail> {
        to_bytes(&v.0).map(|b| b.len() as i64).ok_or(Fail::BadArgs)
    }

    /// bytes shown as a hex dump, like the xxd tool, or just n bytes around address at
    ///
    /// Each line has the offset, 16 bytes in hex, and the same bytes as text with dots for unprintable ones. With at,
    /// the lines keep their real offsets and the byte at at is highlighted, with n bytes shown (64 if not given). Bytes
    /// are colored by kind on a terminal: null, printable, whitespace and control, and non-ASCII
    ///
    /// With text, only the text column is shown, as one line, so it can be selected and copied cleanly
    ///
    /// With base, the offsets shown start at that address instead of 0, and base: true means 0x400000. at and n still count bytes of the data
    #[builtin(
        aliases = ["xxd"],
        examples = [r#"hexdump("hi\n")"#, "hexdump([0, 255, 65])", r#"hexdump(b"\x00" * 40 + b"ELF", 41, 16)"#, r#"hexdump("hello world", text: true)"#],
        see = ["bytes", "entropy", "read_bytes"],
        command = "xxd a.out 0x1000"
    )]
    fn hexdump(
        v: Data,
        at: Option<Or<i64, &str>>,
        n: Option<Or<i64, &str>>,
        /// no colors, even on a terminal
        #[flag('p')]
        plain: bool,
        /// only the text column, as one line
        #[flag('t')]
        text: bool,
        /// the address the first byte is shown at, true for 0x400000
        #[flag('B')]
        base: Option<Or<bool, i64>>,
    ) -> Result<String, Fail> {
        let b = to_bytes(&v.0).ok_or(Fail::BadArgs)?;
        let color = !plain && crate::ansi::color_on();
        let shift = base_arg(base)?;
        match at {
            Some(at) => hexdump_around(&b, number(at, 1)?, n.map_or(Ok(64), |n| number(n, 2))?, color, text, shift),
            None => Ok(dump_rows(&b, 0, None, color, text, shift)),
        }
    }

    /// what changed from bytes a to b, by hex dump row, or nil if nothing did
    ///
    /// Changes come in hunks headed @@ with their byte offset and length, with 3 unchanged rows around them. Removed
    /// rows start with - and added rows with +, colored on a terminal with the changed bytes picked out. With the table
    /// flag, it is instead a table with a row per byte that differs, comparing position by position with nil past the end
    /// of the shorter. With base, offsets start at that address instead of 0, and base: true means 0x400000
    #[builtin(
        examples = [r#"diff(b"\x7fELF\x02\xff", b"\x7fELF\x01\xff")"#, r#"diff(b"\x7fELF", b"\x7fELG\x00", table: true)"#, r#"diff(b"ELF", b"ELG", table: true, base: true)"#],
        see = ["hexdump"],
        command = "diff -b a.out b.out"
    )]
    fn diff(
        a: &[u8],
        b: &[u8],
        /// no colors, even on a terminal
        #[flag('p')]
        plain: bool,
        /// a table of differing bytes instead of hunks
        #[flag('t')]
        table: bool,
        /// the address the first byte is at, true for 0x400000
        #[flag('B')]
        base: Option<Or<bool, i64>>,
    ) -> Result<Value, Fail> {
        let shift = base_arg(base)?;
        Ok(if table { Value::table(byte_table(a, b, shift)) } else { byte_diff(a, b, !plain && crate::ansi::color_on(), shift).map_or(Value::Nil, Value::str) })
    }

    /// what changed from bytes a to b as a table of offset, size, old and new, a row per run of changed bytes
    ///
    /// Bytes compare position by position, and the extra bytes of the longer one are one more run. With base, offsets start at that address instead of 0, and base: true means 0x400000
    #[builtin(examples = [r#"bindiff(b"\x7fELF\x01\x01", b"\x7fELG\x02\x01\x00")"#], see = ["diff", "patch"])]
    fn bindiff(
        a: &[u8],
        b: &[u8],
        /// the address the first byte is at, true for 0x400000
        #[kw]
        base: Option<Or<bool, i64>>,
    ) -> Result<Table, Fail> {
        Ok(byte_runs(a, b, base_arg(base)?))
    }

    /// the offset of the first match of needle in bytes, or nil
    ///
    /// The needle is bytes, or a hex pattern where ?? matches any byte, like "55 48 89 e5 ?? ??"
    #[builtin(name = "find", examples = [r#"b"\x00\x7fELF".find(b"ELF")"#, r#"b"\x55\x48\x89\xe5".find("48 ?? e5")"#], see = ["find_all", "patch"])]
    fn find_bytes(b: &[u8], needle: Or<&[u8], &str>) -> Result<Option<Value>, Fail> {
        Ok(matches(b, &pattern(needle)?).next().map(hex_int))
    }

    /// the offset of every match of needle in bytes, overlapping ones too
    ///
    /// The needle is bytes, or a hex pattern where ?? matches any byte
    #[builtin(name = "find_all", examples = [r#"b"abcabc".find_all(b"bc")"#, r#"b"\x01\x02\x01\x03".find_all("01 ??")"#], see = ["find", "patch"])]
    fn find_all_bytes(b: &[u8], needle: Or<&[u8], &str>) -> Result<Vec<Value>, Fail> {
        Ok(matches(b, &pattern(needle)?).map(hex_int).collect())
    }

    /// a copy of bytes with new written over them at offset
    ///
    /// It's an error if new runs past the end. Save the result with write_file
    #[builtin(examples = [r#"patch(b"\x74\x05\x90", 0, b"\xeb")"#], see = ["find", "bindiff"])]
    fn patch(b: &[u8], offset: i64, new: &[u8]) -> Result<Value, Fail> {
        let at = usize::try_from(offset).map_err(|_| Fail::Arg(1, "offsets can't be negative".into()))?;
        let end = at.checked_add(new.len()).filter(|&e| e <= b.len());
        let end = end.ok_or_else(|| Fail::Arg(2, format!("{} bytes at offset {at:#x} run past the end at {:#x}", new.len(), b.len())))?;
        let mut out = b.to_vec();
        out[at..end].copy_from_slice(new);
        Ok(Value::bytes(out))
    }

    /// the readable text hidden in binary data: every run of at least min printable characters, like the Unix tool
    ///
    /// With the table flag, it is a table of offset and text. With base, offsets start at that address instead of 0, and base: true means 0x400000
    #[builtin(
        examples = [r#"b"\x00\x01hello\xffab\x00world!".strings"#, r#"strings(b"ab\x00cd", 2)"#, r#"strings(b"\x00hello\x00world", table: true)"#],
        see = ["hexdump", "read_bytes"],
        command = "strings -t a.out"
    )]
    fn strings(
        v: Or<&[u8], &str>,
        #[default(4)] min: i64,
        /// a table of offset and text instead of a list
        #[flag('t')]
        table: bool,
        /// the address the first byte is at, true for 0x400000
        #[flag('B')]
        base: Option<Or<bool, i64>>,
    ) -> Result<Value, Fail> {
        if min < 1 {
            return Err(Fail::BadArgs);
        }
        let b = match v {
            Or::A(b) => b,
            Or::B(s) => s.as_bytes(),
        };
        let shift = base_arg(base)?;
        let mut rows = vec![];
        let mut at = 0;
        for r in b.split(|c| !(c.is_ascii_graphic() || *c == b' ' || *c == b'\t')) {
            if r.len() >= min as usize {
                rows.push(vec![hex_int(at + shift), Value::str(String::from_utf8_lossy(r))]);
            }
            at += r.len() + 1;
        }
        if !table {
            return Ok(Value::list(rows.into_iter().map(|mut r| r.remove(1)).collect()));
        }
        Ok(Value::table(Table { cols: vec!["offset".into(), "text".into()], rows }))
    }

    /// how unpredictable the bytes are (Shannon entropy)
    ///
    /// It goes from 0 bits per byte when every byte is the same to 8 for random or encrypted data
    #[builtin(examples = [r#""aaaa".entropy"#, r#""abcd".entropy"#], see = ["hexdump"])]
    fn entropy(v: Data) -> Result<f64, Fail> {
        Ok(shannon(&to_bytes(&v.0).ok_or(Fail::BadArgs)?))
    }
}

/// Whether a byte order argument, "big" or "little", is big-endian.
pub(crate) fn big_endian(order: &str) -> Result<bool, Fail> {
    match order {
        "big" => Ok(true),
        "little" => Ok(false),
        _ => Err(Fail::Arg(1, format!("the byte order is \"big\" or \"little\", got {order:?}"))),
    }
}

fn block_size(size: Option<i64>) -> Result<usize, Fail> {
    let size = size.unwrap_or(16);
    usize::try_from(size).ok().filter(|n| (1..=255).contains(n)).ok_or_else(|| Fail::Arg(1, format!("the block size goes from 1 to 255, got {size}")))
}

/// Shannon entropy in bits per byte.
pub(crate) fn shannon(b: &[u8]) -> f64 {
    let mut counts = [0usize; 256];
    b.iter().for_each(|&x| counts[x as usize] += 1);
    let n = b.len() as f64;
    // `+ 0.0` turns the -0 from a single repeated byte into 0.
    counts.iter().filter(|&&c| c > 0).map(|&c| -(c as f64 / n) * (c as f64 / n).log2()).sum::<f64>() + 0.0
}

/// How much bytes look like English, from 0 up to about 1: how closely their letters and spaces match English's
/// frequencies (the Bhattacharyya coefficient), times the share of bytes that are letters or spaces, less the share
/// that are unprintable. Capitals count a little less, so XOR keys that flip case lose.
pub(crate) fn english(b: &[u8]) -> f64 {
    english_hist(&histogram(b), |c| c)
}

/// Each byte value in `b` with how often it appears.
fn histogram(b: &[u8]) -> Vec<(u8, usize)> {
    let mut hist = [0usize; 256];
    b.iter().for_each(|&c| hist[c as usize] += 1);
    (0..=255u8).zip(hist).filter(|(_, h)| *h > 0).collect()
}

/// `english` from a `histogram`, each byte first mapped through `f`, so trying 255 XOR keys doesn't rescan the bytes.
fn english_hist(hist: &[(u8, usize)], f: impl Fn(u8) -> u8) -> f64 {
    let (mut counts, mut weight, mut junk, mut len) = ([0usize; 27], 0.0, 0, 0);
    for &(c, h) in hist {
        len += h;
        let c = f(c);
        let (slot, w) = match c {
            b'a'..=b'z' => ((c - b'a') as usize, 1.0),
            b'A'..=b'Z' => ((c - b'A') as usize, 0.9),
            b' ' => (26, 1.0),
            b'\n' | b'\r' | b'\t' | b'!'..=b'~' => continue,
            _ => {
                junk += h;
                continue;
            }
        };
        counts[slot] += h;
        weight += w * h as f64;
    }
    let n: usize = counts.iter().sum();
    if n == 0 {
        return -(junk as f64) / len.max(1) as f64;
    }
    // Spaces are about 18% of English text and letters the rest.
    let expected = |i: usize| if i == 26 { 0.18 } else { FREQ[i] / 100.0 * 0.82 };
    let bc: f64 = counts.iter().enumerate().map(|(i, &k)| (k as f64 / n as f64 * expected(i)).sqrt()).sum();
    (bc * weight - junk as f64) / len as f64
}

const ENCODINGS: [&str; 12] = ["base64", "base32", "base58", "base62", "base85", "ascii85", "base91", "uuencode", "quoted_printable", "punycode", "url", "hex"];

/// `s` decoded from one of `ENCODINGS`.
fn decoded(s: &str, fmt: &str) -> Result<Vec<u8>, String> {
    let bad = || format!("invalid {fmt}");
    match fmt {
        "base64" => base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s.trim()).map_err(|e| e.to_string()),
        "base32" => unbase32(s.trim()).ok_or_else(bad),
        "base58" => unbase58(s.trim()).ok_or_else(bad),
        "base62" => codecs::unbase62(s.trim()).ok_or_else(bad),
        "base85" => codecs::unbase85(s).ok_or_else(bad),
        "ascii85" => codecs::unascii85(s).ok_or_else(bad),
        "base91" => codecs::unbase91(s).ok_or_else(bad),
        "uuencode" => codecs::unuuencode(s).ok_or_else(bad),
        "quoted_printable" => codecs::unquoted_printable(s).ok_or_else(bad),
        "punycode" => codecs::unpunycode(s.trim()).map(String::into_bytes).ok_or_else(bad),
        "url" => url_decode(s).ok_or_else(|| "invalid percent-encoding".into()),
        _ => unhex(s.trim()).ok_or_else(bad),
    }
}

/// Everything `decodings` makes of `start`, layer on layer up to `depth` deep, with the chain of decodings that made it; each result once.
fn layers(start: Vec<u8>, depth: usize) -> Vec<(String, Vec<u8>)> {
    let mut seen = std::collections::HashSet::from([start.clone()]);
    let (mut layer, mut found) = (vec![(String::new(), start)], vec![]);
    for _ in 0..depth {
        let mut next = vec![];
        for (chain, b) in &layer {
            for (name, out) in decodings(b) {
                if seen.insert(out.clone()) {
                    next.push((if chain.is_empty() { name } else { format!("{chain} → {name}") }, out));
                }
            }
        }
        found.extend(next.iter().cloned());
        layer = next;
    }
    found
}

/// Where encoded text might hide in `b`: runs of 8 or more printable characters, and the words of 8 or more in them.
fn tokens(b: &[u8]) -> Vec<(usize, &[u8])> {
    let (mut out, mut at) = (vec![], 0);
    for run in b.split(|c| !(c.is_ascii_graphic() || *c == b' ' || *c == b'\t')) {
        if run.len() >= 8 {
            out.push((at, run));
            let mut i = at;
            for word in run.split(|c| c.is_ascii_whitespace()) {
                if word.len() >= 8 && word.len() < run.len() {
                    out.push((i, word));
                }
                i += word.len() + 1;
            }
        }
        at += run.len() + 1;
    }
    out
}

/// Every one-step decoding of `b` that `decode_auto` tries, named, skipping those that fail or change nothing.
fn decodings(b: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let hist = histogram(b);
    let score = |k: u8| english_hist(&hist, |c| c ^ k);
    // Only a key byte that makes the bytes read clearly more like English counts.
    if let Some(k) = (1..=255u8).max_by(|&x, &y| score(x).total_cmp(&score(y))).filter(|_| b.len() >= 8)
        && score(k) > (score(0) + 0.1).max(0.5)
    {
        out.push((format!("xor 0x{k:02x}"), b.iter().map(|c| c ^ k).collect()));
    }
    let Ok(s) = std::str::from_utf8(b) else { return out };
    for fmt in ENCODINGS {
        let fits = match fmt {
            "url" => s.contains('%'),
            "punycode" => s.to_ascii_lowercase().contains("xn--"),
            "quoted_printable" => s.contains('='),
            // Big-number decoding takes time quadratic in the length, and these encode short IDs anyway.
            "base58" | "base62" => s.len() <= 1000,
            _ => true,
        };
        if let Some(d) = fits.then(|| decoded(s, fmt).ok()).flatten().filter(|d| !d.is_empty() && d != b) {
            out.push((fmt.to_string(), d));
        }
    }
    if s.bytes().any(|c| c.is_ascii_alphabetic()) {
        let rot = s.bytes().map(|c| if c.is_ascii_alphabetic() { (c - (c & 0x20 | 0x41) + 13) % 26 + (c & 0x20 | 0x41) } else { c }).collect();
        out.push(("rot13".into(), rot));
    }
    let tokens: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    for (name, radix) in [("binary", 2), ("octal", 8), ("decimal", 10), ("hex", 16)] {
        if tokens.len() > 1
            && let Some(d) = tokens.iter().map(|t| u8::from_str_radix(t, radix).ok()).collect::<Option<Vec<u8>>>()
        {
            out.push((name.into(), d));
        }
    }
    out
}

/// `b` as text when it's UTF-8 with a letter or digit and no control characters but line breaks and tabs.
fn printable(b: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(b).ok()?;
    (s.chars().any(char::is_alphanumeric) && s.chars().all(|c| !c.is_control() || "\n\r\t".contains(c))).then(|| s.to_string())
}

fn unknown_encoding(fmt: &str) -> String {
    const NAMES: [&str; 12] = ENCODINGS;
    let hint = crate::error::did_you_mean(fmt, NAMES);
    if hint.is_empty() { format!("unknown encoding {fmt:?}\nnote: encodings are {}", NAMES.join(", ")) } else { format!("unknown encoding {fmt:?}{hint}") }
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

#[rustfmt::skip]
pub(crate) const FREQ: [f64; 26] = [
    8.2, 1.5, 2.8, 4.3, 12.7, 2.2, 2.0, 6.1, 7.0, 0.15, 0.77, 4.0, 2.4, 6.7, 7.5, 1.9, 0.095, 6.0, 6.3, 9.1, 2.8, 0.98, 2.4, 0.15, 2.0, 0.074,
];

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
    codecs::radix(b, B58)
}

fn unbase58(s: &str) -> Option<Vec<u8>> {
    codecs::unradix(s, B58)
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
    dump_rows(b, 0, None, false, false, 0)
}

/// `hexdump` of `b`, whose first byte is at `base` (a multiple of 16), with bytes colored by kind if `color` and the one at `mark` picked out.
fn dump_rows(b: &[u8], base: usize, mark: Option<usize>, color: bool, text: bool, shift: usize) -> String {
    let cell = |x: u8, s: String, at: usize| {
        if !color {
            return s;
        }
        let role = match x {
            0 => Role::Dim,
            _ if x.is_ascii_graphic() => Role::Str,
            _ if x.is_ascii() => Role::Keyword,
            _ => Role::Group,
        };
        format!("{}{}{s}{RESET}", role.code(), if mark == Some(at) { Role::Changed.code() } else { "" })
    };
    let (mut out, mut texts) = (Vec::new(), String::new());
    for (r, row) in b.chunks(16).enumerate() {
        let off = base + r * 16;
        let hex: Vec<String> =
            row.chunks(2).enumerate().map(|(p, pair)| pair.iter().enumerate().map(|(q, &x)| cell(x, format!("{x:02x}"), off + 2 * p + q)).collect()).collect();
        let ascii: String =
            row.iter().enumerate().map(|(k, &x)| cell(x, (if x.is_ascii_graphic() || x == b' ' { x as char } else { '.' }).into(), off + k)).collect();
        let pad = 39 - (row.len() * 2 + row.len().div_ceil(2) - 1);
        let offset = format!("{:08x}", off + shift);
        let offset = if color { format!("{}{offset}{RESET}", Role::Dim.code()) } else { offset };
        if text {
            texts += &ascii;
        } else {
            out.push(format!("{offset}: {}{}  {ascii}", hex.join(" "), " ".repeat(pad)));
        }
    }
    if text {
        return texts;
    }
    out.join("\n")
}

/// The `base` option: no offset shift if absent or false, 0x400000 for true, else the given address.
pub(crate) fn base_arg(base: Option<Or<bool, i64>>) -> Result<usize, Fail> {
    match base {
        None | Some(Or::A(false)) => Ok(0),
        Some(Or::A(true)) => Ok(0x400000),
        Some(Or::B(n)) => usize::try_from(n).map_err(|_| Fail::BadArgs),
    }
}

/// An address or size given as a number, or as command-line text like `0x1000`.
fn number(v: Or<i64, &str>, arg: usize) -> Result<i64, Fail> {
    match v {
        Or::A(n) => Ok(n),
        Or::B(s) => match s.strip_prefix("0x") {
            Some(h) => i64::from_str_radix(h, 16),
            None => s.parse(),
        }
        .map_err(|_| Fail::Arg(arg, format!("expected a number like 4096 or 0x1000, got {s:?}"))),
    }
}

/// `n` bytes of `b` around address `at`, as colored (if `color`) hex dump rows with real offsets and `at` picked out.
fn hexdump_around(b: &[u8], at: i64, n: i64, color: bool, text: bool, shift: usize) -> Result<String, Fail> {
    let at = usize::try_from(at).ok().filter(|&a| a < b.len()).ok_or_else(|| Fail::Arg(1, format!("address {at:#x} is outside the {} bytes", b.len())))?;
    let n = usize::try_from(n).ok().filter(|&n| n > 0).ok_or_else(|| Fail::Arg(2, "n must be positive".into()))?;
    let start = at.saturating_sub(n / 2) & !15;
    let end = b.len().min((at + n / 2 + 1).next_multiple_of(16));
    Ok(dump_rows(&b[start..end], start, Some(at), color, text, shift))
}

/// The hex dump rows that changed from `a` to `b`, with the changed bytes picked out, in hunks headed by byte offset
/// and length; `None` if they're the same.
fn byte_diff(a: &[u8], b: &[u8], color: bool, shift: usize) -> Option<String> {
    let (da, db) = (dump_rows(a, 0, None, false, false, shift), dump_rows(b, 0, None, false, false, shift));
    let (ra, rb): (Vec<_>, Vec<_>) = (a.chunks(16).collect(), b.chunks(16).collect());
    let at = |rows: Range<usize>, len: usize| {
        let start = (rows.start * 16).min(len);
        format!("0x{:x},{}", start + shift, (rows.end * 16).min(len) - start)
    };
    // A byte k of a row is hex digits at 10 + 2k + k/2 and a character at 51 + k, as `hexdump` lays them out.
    let changed = |x: &[u8], y: &[u8]| {
        let mut spans: Vec<_> =
            (0..x.len()).filter(|&k| y.get(k) != Some(&x[k])).flat_map(|k| [10 + 2 * k + k / 2..12 + 2 * k + k / 2, 51 + k..52 + k]).collect();
        spans.sort_by_key(|r| r.start);
        spans
    };
    super::compare::unified(
        &da.lines().collect::<Vec<_>>(),
        &db.lines().collect::<Vec<_>>(),
        color,
        |x, y| format!("@@ -{} +{} @@", at(x, a.len()), at(y, b.len())),
        |i, j| (changed(ra[i], rb[j]), changed(rb[j], ra[i])),
    )
}

/// A row of offset, size and the bytes on each side for every run where `a` and `b` differ.
fn byte_runs(a: &[u8], b: &[u8], shift: usize) -> crate::value::Table {
    let n = a.len().max(b.len());
    let mut rows = vec![];
    let mut k = 0;
    while k < n {
        if a.get(k) == b.get(k) {
            k += 1;
            continue;
        }
        let start = k;
        while k < n && a.get(k) != b.get(k) {
            k += 1;
        }
        let side = |x: &[u8]| Value::bytes(&x[start.min(x.len())..k.min(x.len())]);
        rows.push(vec![hex_int(start + shift), Value::int((k - start) as i64), side(a), side(b)]);
    }
    crate::value::Table { cols: ["offset", "size", "old", "new"].map(String::from).to_vec(), rows }
}

/// A byte search needle: each byte, or `None` for a `??` wildcard.
fn pattern(needle: Or<&[u8], &str>) -> Result<Vec<Option<u8>>, Fail> {
    let s = match needle {
        Or::A(b) => return Ok(b.iter().map(|&x| Some(x)).collect()),
        Or::B(s) => s.split_whitespace().collect::<String>(),
    };
    let bad = || Fail::Arg(1, format!("expected a hex pattern like \"55 48 ?? e5\", got {s:?}"));
    if s.is_empty() || s.len() % 2 != 0 {
        return Err(bad());
    }
    let pairs = s.as_bytes().chunks(2).map(|p| std::str::from_utf8(p).ok());
    pairs.map(|p| if p == Some("??") { Ok(None) } else { p.and_then(|p| u8::from_str_radix(p, 16).ok()).map(Some).ok_or_else(bad) }).collect()
}

fn matches<'a>(b: &'a [u8], pat: &'a [Option<u8>]) -> impl Iterator<Item = usize> + 'a {
    let fits = move |w: &[u8]| w.iter().zip(pat).all(|(x, p)| p.is_none_or(|p| p == *x));
    b.windows(pat.len().max(1)).enumerate().filter(move |(_, w)| !pat.is_empty() && fits(w)).map(|(i, _)| i)
}

pub(crate) fn hex_int(n: usize) -> Value {
    Value::Int(n as i64, crate::ast::Radix::HEX)
}

/// A row of offset and the byte on each side for every position where `a` and `b` differ, nil past either's end.
fn byte_table(a: &[u8], b: &[u8], shift: usize) -> crate::value::Table {
    let byte = |x: Option<&u8>| x.map_or(Value::Nil, |&x| hex_int(x.into()));
    let rows = (0..a.len().max(b.len())).filter(|&k| a.get(k) != b.get(k)).map(|k| vec![hex_int(k + shift), byte(a.get(k)), byte(b.get(k))]).collect();
    crate::value::Table { cols: ["offset", "a", "b"].map(String::from).to_vec(), rows }
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
    fn codecs() {
        let round = |fmt: &str, s: &str, enc: &str| {
            assert_eq!(show(&format!("{s:?}.encode({fmt:?})")), enc, "{fmt}");
            let lit = format!("{enc:?}").replace('{', r"\{");
            assert_eq!(show(&format!("{lit}.decode({fmt:?})")), s, "{fmt}");
        };
        round("base62", "hello", "7tQLFHz");
        round("base85", "hello", "Xk~0{Zv");
        round("ascii85", "hello", "BOu!rDZ");
        round("base91", "hello world", "TPwJh>Io2Tv!lE");
        round("punycode", "münchen.de", "xn--mnchen-3ya.de");
        round("punycode", "例え.テスト", "xn--r8jz45g.xn--zckzah");
        round("quoted_printable", "café = 1\n", "caf=C3=A9 =3D 1\n");
        round("uuencode", "Cat", "begin 644 data\n#0V%T\n`\nend");
        assert_eq!(show(r#""<~87cURD]i,\"Ebo80~>".decode("ascii85")"#), "Hello World!");
        assert_eq!(show(r#"b"\0\0\0\0".encode("ascii85")"#), "z");
        assert_eq!(show(r#""z".decode("ascii85").bytes"#), r#"b"\0\0\0\0""#);
        assert_eq!(show(r#""mnchen-3ya".decode("punycode")"#), "münchen");
        assert_eq!(show(r#"("x" * 80).encode("quoted_printable").decode("quoted_printable").len"#), "80");
        assert!(show(r#"("x" * 80).encode("quoted_printable")"#).contains("=\n"));
        assert!(try_eval(r#""~".decode("ascii85")"#).is_err());
        assert!(try_eval(r#"b"\xff".encode("punycode")"#).is_err());
        assert!(try_eval(r#""x".encode("base99")"#).is_err());
    }

    #[test]
    fn decode_auto() {
        assert_eq!(show(r#"decode_auto("Njg2OTIwNzQ2ODY1NzI2NQ==")[0]"#), r#"{chain: "base64 → hex", result: "hi there"}"#);
        assert_eq!(show(r#"decode_auto("uryyb jbeyq")[0].result"#), "hello world");
        assert_eq!(show(r#"decode_auto("01101000 01101001")[0]"#), r#"{chain: "binary", result: "hi"}"#);
        assert_eq!(show(r#"decode_auto(xor(b"attack at dawn", 0x41).hex)[0]"#), r#"{chain: "hex → xor 0x41", result: "attack at dawn"}"#);
        assert!(try_eval(r#"decode_auto("x", 9)"#).is_err());
    }

    #[test]
    fn flag_find() {
        assert_eq!(show(r#"flag_find("junk ZmxhZ3toaWRkZW59 junk")[0]"#), r#"{offset: 0x5, chain: "base64", flag: "flag{hidden}"}"#);
        assert_eq!(show(r#"flag_find(b"\0\0CTF{raw}\0", "CTF\{")[0]"#), r#"{offset: 0x2, chain: "", flag: "CTF{raw}"}"#);
        assert_eq!(show(r#"flag_find(b"\xff" + "synt\{ebg}".bytes + b"\0")[0].chain"#), "rot13");
        assert_eq!(show(r#"flag_find("nothing here").len"#), "0");
        assert!(try_eval(r#"flag_find("x", "")"#).is_err());
    }

    #[test]
    fn byte_text() {
        assert_eq!(show(r#"from_byte_text("01001000 01101001")"#), "Hi");
        assert_eq!(show(r#"from_byte_text("110 151 041")"#), "Hi!");
        assert_eq!(show(r#"from_byte_text("72, 105, 33")"#), "Hi!");
        assert_eq!(show(r#"from_byte_text("48 69 21")"#), "Hi!");
        assert!(try_eval(r#"from_byte_text("999")"#).is_err());
        assert!(try_eval(r#"from_byte_text("1", 3)"#).is_err());
    }

    #[test]
    fn binary() {
        assert_eq!(show(r#""hi\n".hexdump"#), format!("00000000: {:<39}  hi.", "6869 0a"));
        assert_eq!(show("hexdump([])"), "");
        let around = show(r#"hexdump(b"\0" * 100, 50, 16)"#);
        assert!(around.starts_with("00000020: ") && around.lines().count() == 2);
        assert!(try_eval(r#"hexdump(b"ab", 5)"#).is_err());
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

    #[test]
    fn byte_search() {
        assert_eq!(show(r#"b"\x00\x7fELF".find(b"ELF")"#), "0x2");
        assert_eq!(show(r#"b"\x55\x48\x89\xe5".find("48 ?? e5")"#), "0x1");
        assert_eq!(show(r#"b"\x55".find("aa")"#), "nil");
        assert_eq!(show(r#"b"abcabc".find_all(b"bc")"#), "[0x1, 0x4]");
        assert_eq!(show(r#"b"aaa".find_all("61 61")"#), "[0x0, 0x1]");
        assert!(try_eval(r#"b"x".find("4")"#).unwrap_err().msg.contains("expected a hex pattern"));
        assert!(try_eval(r#"b"x".find("zz")"#).is_err());
    }

    #[test]
    fn bytes_and_ints() {
        assert_eq!(show(r#"[bytes(0x414243), bytes(258, "little"), bytes(0)]"#), r#"[b"ABC", b"\x02\x01", b"\0"]"#);
        assert_eq!(show("bytes(2 ** 64).len"), "9");
        assert_eq!(show(r#"[int(b"ABC"), int(b"\x02\x01", "little"), int(b"")]"#), "[4276803, 258, 0]");
        assert_eq!(show("int(bytes(3 ** 100)) == 3 ** 100"), "true");
        assert_eq!(show("[0x7f, 69].bytes"), r#"b"\x7fE""#);
        assert!(try_eval("bytes(-1)").is_err());
        assert!(try_eval(r#"int(b"A", "middle")"#).unwrap_err().msg.contains("big"));
    }

    #[test]
    fn pkcs7() {
        assert_eq!(show(r#"pkcs7_pad(b"YELLOW SUBMARINE", 20)"#), r#"b"YELLOW SUBMARINE\x04\x04\x04\x04""#);
        assert_eq!(show(r#"[pkcs7_pad(b"").len, pkcs7_pad(b"x" * 16).len, pkcs7_pad(b"hi")[-1]]"#), "[16, 32, 14]");
        assert_eq!(show(r#"pkcs7_unpad(pkcs7_pad(b"hello"))"#), r#"b"hello""#);
        assert!(try_eval(r#"pkcs7_unpad(b"x" * 15 + b"\x02")"#).unwrap_err().msg.contains("bad PKCS#7"));
        assert!(try_eval(r#"pkcs7_unpad(b"x" * 15 + b"\x00")"#).is_err());
        assert!(try_eval(r#"pkcs7_unpad(b"abc\x01")"#).is_err());
        assert!(try_eval(r#"pkcs7_pad(b"a", 0)"#).is_err());
    }

    #[test]
    fn patch() {
        assert_eq!(show(r#"patch(b"\x74\x05\x90", 0, b"\xeb")"#), r#"b"\xeb\x05\x90""#);
        assert_eq!(show(r#"patch(b"ab", 1, b"z")"#), r#"b"az""#);
        assert!(try_eval(r#"patch(b"ab", 1, b"yz")"#).unwrap_err().msg.contains("run past the end"));
        assert!(try_eval(r#"patch(b"ab", -1, b"z")"#).is_err());
    }

    #[test]
    fn base() {
        assert!(show(r#"hexdump(b"hi", base: true)"#).starts_with("00400000:"));
        assert!(show(r#"hexdump(b"hi", base: 0x1000)"#).starts_with("00001000:"));
        assert!(show(r#"hexdump(b"hi", base: false)"#).starts_with("00000000:"));
        assert_eq!(show(r#"diff(b"ab", b"ac", table: true, base: 0x10).offset"#), "[0x11]");
        assert_eq!(show(r#"bindiff(b"ab", b"ac", base: 0x10).offset"#), "[0x11]");
        assert_eq!(show(r#"strings(b" hello", table: true, base: 0x10).offset"#), "[0x11]");
        assert_eq!(show(r#"flag_find(b" flag{x}", base: 0x10)[0].offset"#), "0x11");
        assert!(try_eval(r#"hexdump(b"hi", base: -1)"#).is_err());
    }

    #[test]
    fn bindiff() {
        let t = r#"bindiff(b"\x7fELF\x01\x01", b"\x7fELG\x02\x01\x00")"#;
        assert_eq!(show(&format!("{t}.offset")), "[0x3, 0x6]");
        assert_eq!(show(&format!("{t}.size")), "[2, 1]");
        assert_eq!(show(&format!("{t}.old")), r#"[b"F\x01", b""]"#);
        assert_eq!(show(&format!("{t}.new")), r#"[b"G\x02", b"\0"]"#);
        assert_eq!(show(r#"bindiff(b"ab", b"ab").len"#), "0");
    }

    #[test]
    fn byte_diff() {
        let (a, b) = ([&[0u8; 16][..], b"\x7fELF\xff"].concat(), [&[0u8; 16][..], b"\x7fELG\xff\x00"].concat());
        let plain = super::byte_diff(&a, &b, false, 0).unwrap();
        assert!(plain.starts_with("@@ -0x0,21 +0x0,22 @@\n  00000000: 0000"), "{plain}");
        assert!(plain.contains("\n- 00000010: 7f45 4c46 ff") && plain.contains("\n+ 00000010: 7f45 4c47 ff00"), "{plain}");
        let painted = super::byte_diff(&a, &b, true, 0).unwrap();
        assert!(painted.contains("7f45 4c\x1b[7m47\x1b[27m ff\x1b[7m00\x1b[27m"), "{painted:?}");
        assert!(painted.contains(".EL\x1b[7mG\x1b[27m.\x1b[7m.\x1b[27m"), "{painted:?}");
        assert_eq!(super::byte_diff(b"ab", b"ab", true, 0), None);
    }
}
