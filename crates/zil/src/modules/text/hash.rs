//! Hashes and checksums of strings and byte lists.

use super::encoding::{bytes, hex};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;
use sha2::Digest;

pub const MODULE: Module = Module {
    name: "hash",
    about: "fingerprints of text: SHA and MD5 hashes, HMAC signatures, CRC-32 checksums",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Fingerprints of text. A hash turns any text into a short fixed-length value, shown as hex. The same input always gives \
the same hash, and the smallest change gives a completely different one, so hashes are used to check that data hasn't \
changed."),
        ("good to know", "\
SHA-256 and SHA-512 are secure. MD5 and SHA-1 are broken for security purposes but still fine as checksums, and CRC-32 \
only catches accidental damage."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("hash", &[
            ("the MD5 hash of hello", r#""hello".md5"#),
            ("a CRC-32 checksum in hex", r#""hello".crc32 to hex"#),
            ("sign a message with a secret key", r#"hmac("msg", "secret")"#),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("sha1", "sha1(s: str) -> str", "the SHA-1 hash, in hex", &[r#""hello".sha1"#], &["sha256"]),
    doc("sha256", "sha256(s: str) -> str", "the SHA-256 hash, in hex", &[r#""hello".sha256[..16]"#], &["md5", "hmac"]),
    doc("sha512", "sha512(s: str) -> str", "the SHA-512 hash, in hex", &[r#""hello".sha512[..16]"#], &["sha256"]),
    doc("md5", "md5(s: str) -> str", "the MD5 hash, in hex", &[r#""hello".md5"#], &["sha256"]),
    doc("hmac", "hmac(s: str, key: str, alg?: str) -> str", "the HMAC of s with the secret key, in hex", &[r#"hmac("msg", "secret")"#, r#"hmac("msg", "secret", "sha1")"#], &["sha256"])
        .detail("A hash mixed with the key, so only someone with the key can produce it, which is how APIs sign messages and webhooks. alg is the hash to use: \"sha256\" (the default), \"sha1\", \"sha512\" or \"md5\""),
    doc("crc32", "crc32(v: str|list) -> int", "the CRC-32 checksum, as an integer, the same one zip files and PNG images use", &[r#""hello".crc32"#, r#""hello".crc32 to hex"#], &["sha256", "md5"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    Ok(match (name, args) {
        ("sha1" | "sha256" | "sha512" | "md5", [Value::Str(s)]) => Value::str(hex(&digest(name, s.as_bytes()).unwrap())),
        ("hmac", [Value::Str(s), Value::Str(key)]) => Value::str(hex(&hmac("sha256", key.as_bytes(), s.as_bytes()).unwrap())),
        ("hmac", [Value::Str(s), Value::Str(key), Value::Str(alg)]) => {
            let mac = hmac(alg, key.as_bytes(), s.as_bytes());
            Value::str(hex(&mac.ok_or_else(|| Fail::Arg(2, format!("unknown hash {alg:?}{}", crate::error::did_you_mean(alg, ALGS))))?))
        }
        ("crc32", [v]) => Value::int(crc32(&bytes(v).ok_or(Fail::BadArgs)?) as i64),
        _ => return Err(Fail::BadArgs),
    })
}

const ALGS: [&str; 4] = ["sha256", "sha1", "sha512", "md5"];

fn digest(alg: &str, b: &[u8]) -> Option<Vec<u8>> {
    Some(match alg {
        "sha1" => sha1::Sha1::digest(b).to_vec(),
        "sha256" => sha2::Sha256::digest(b).to_vec(),
        "sha512" => sha2::Sha512::digest(b).to_vec(),
        "md5" => md5::Md5::digest(b).to_vec(),
        _ => return None,
    })
}

/// RFC 2104: H((K ^ opad) || H((K ^ ipad) || msg)), with K padded to the hash's block size.
fn hmac(alg: &str, key: &[u8], msg: &[u8]) -> Option<Vec<u8>> {
    let block = if alg == "sha512" { 128 } else { 64 };
    let mut k = if key.len() > block { digest(alg, key)? } else { key.to_vec() };
    k.resize(block, 0);
    let pad = |x: u8| k.iter().map(|b| b ^ x).collect::<Vec<u8>>();
    let inner = digest(alg, &[pad(0x36), msg.to_vec()].concat())?;
    digest(alg, &[pad(0x5c), inner].concat())
}

fn crc32(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn hashes() {
        assert_eq!(show(r#""abc".sha256"#), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(show(r#""abc".md5"#), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(show(r#""123456789".crc32 to hex"#), "0xcbf43926");
        assert!(try_eval("[256].crc32").is_err());
        assert_eq!(show(r#""abc".sha1"#), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(show(r#""abc".sha512[..16]"#), "ddaf35a193617aba");
        // RFC 4231 test case 2
        assert_eq!(show(r#"hmac("what do ya want for nothing?", "Jefe")"#), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        assert_eq!(show(r#"hmac("what do ya want for nothing?", "Jefe", "md5")"#), "750c783e6ab0b503eaa86e310a5db738");
        assert_eq!(show(r#"hmac("what do ya want for nothing?", "Jefe", "sha512")[..16]"#), "164b7a7bfcf819e2");
        assert!(try_eval(r#"hmac("a", "b", "sha3")"#).is_err());
    }
}
