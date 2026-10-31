//! Hashes and checksums of strings and byte lists.

use crate::modules::Module;
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
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{ALGS, crc32 as checksum, digest, guess, hmac as mac};
    use crate::modules::Fail;
    use crate::modules::text::encoding::{Data, hex};
    use crate::signature::List;
    use crate::value::Value;

    fn hash(alg: &str, s: &str) -> String {
        hex(&digest(alg, s.as_bytes()).expect("alg checked by its type"))
    }

    /// the SHA-1 hash, in hex
    #[builtin(examples = [r#""hello".sha1"#], see = ["sha256"])]
    fn sha1(s: &str) -> String {
        hash("sha1", s)
    }

    /// the SHA-256 hash, in hex
    #[builtin(examples = [r#""hello".sha256[..16]"#], see = ["md5", "hmac"])]
    fn sha256(s: &str) -> String {
        hash("sha256", s)
    }

    /// the SHA-512 hash, in hex
    #[builtin(examples = [r#""hello".sha512[..16]"#], see = ["sha256"])]
    fn sha512(s: &str) -> String {
        hash("sha512", s)
    }

    /// the MD5 hash, in hex
    #[builtin(examples = [r#""hello".md5"#], see = ["sha256"])]
    fn md5(s: &str) -> String {
        hash("md5", s)
    }

    /// the HMAC of s with the secret key, in hex
    ///
    /// A hash mixed with the key, so only someone with the key can produce it, which is how APIs sign messages and webhooks. alg is the hash to use: "sha256", "sha1", "sha384", "sha512" or "md5"
    #[builtin(examples = [r#"hmac("msg", "secret")"#, r#"hmac("msg", "secret", "sha1")"#], see = ["sha256"])]
    fn hmac(s: &str, key: &str, #[default("sha256")] alg: &str) -> Result<String, Fail> {
        let m = mac(alg, key.as_bytes(), s.as_bytes());
        Ok(hex(&m.ok_or_else(|| Fail::Arg(2, format!("unknown hash {alg:?}{}", crate::error::did_you_mean(alg, ALGS))))?))
    }

    /// what kind of hash this looks like, from its length and format, likeliest first
    ///
    /// Hex digests are told apart only by length, so 32 hex digits could be MD5, NTLM or MD4. Crypt-style hashes like $2b$... (bcrypt) and $6$... (SHA-512 crypt) are recognized by their prefix. Empty when nothing fits
    #[builtin(examples = [r#"hash_id("5d41402abc4b2a76b9719d911017c592")"#, r#"hash_id("$2b$12$abcdefghijklmnopqrstuuJ2WBHnL3eYDKZJ6XtAlr3yXlJ9z8u2G")"#], see = ["hash_match", "md5"])]
    fn hash_id(hash: &str) -> Vec<Value> {
        guess(hash.trim()).into_iter().map(Value::str).collect()
    }

    /// the candidate whose hash is this one, or nil, hashing each with every algorithm hash_id suggests that zil has
    ///
    /// Works for MD5, SHA-1, SHA-256, SHA-384 and SHA-512 hex digests, upper or lower case. A small dictionary attack, for when a puzzle hands you a hash and a word list
    #[builtin(examples = [r#"hash_match("5d41402abc4b2a76b9719d911017c592", ["hi", "hello", "hey"])"#], see = ["hash_id", "md5", "sha256"])]
    fn hash_match(hash: &str, candidates: &List) -> Result<Option<String>, Fail> {
        let hash = hash.trim().to_ascii_lowercase();
        let algs: Vec<&str> = guess(&hash).into_iter().filter(|a| ALGS.contains(a)).collect();
        if algs.is_empty() {
            return Err(Fail::Arg(0, "not an MD5, SHA-1, SHA-256, SHA-384 or SHA-512 hex digest".into()));
        }
        Ok(candidates.borrow().iter().map(|c| c.to_string()).find(|c| algs.iter().any(|a| digest(a, c.as_bytes()).is_some_and(|d| hex(&d) == hash))))
    }

    /// the CRC-32 checksum, as an integer, the same one zip files and PNG images use
    #[builtin(examples = [r#""hello".crc32"#, r#""hello".crc32 to hex"#], see = ["sha256", "md5"])]
    fn crc32(v: Data) -> Result<i64, Fail> {
        Ok(checksum(&v.bytes().map_err(|_| Fail::BadArgs)?) as i64)
    }
}

const ALGS: [&str; 5] = ["sha256", "sha1", "sha384", "sha512", "md5"];

fn digest(alg: &str, b: &[u8]) -> Option<Vec<u8>> {
    Some(match alg {
        "sha1" => sha1::Sha1::digest(b).to_vec(),
        "sha256" => sha2::Sha256::digest(b).to_vec(),
        "sha384" => sha2::Sha384::digest(b).to_vec(),
        "sha512" => sha2::Sha512::digest(b).to_vec(),
        "md5" => md5::Md5::digest(b).to_vec(),
        _ => return None,
    })
}

/// RFC 2104: H((K ^ opad) || H((K ^ ipad) || msg)), with K padded to the hash's block size.
pub(crate) fn hmac(alg: &str, key: &[u8], msg: &[u8]) -> Option<Vec<u8>> {
    let block = if matches!(alg, "sha384" | "sha512") { 128 } else { 64 };
    let mut k = if key.len() > block { digest(alg, key)? } else { key.to_vec() };
    k.resize(block, 0);
    let pad = |x: u8| k.iter().map(|b| b ^ x).collect::<Vec<u8>>();
    let inner = digest(alg, &[pad(0x36), msg.to_vec()].concat())?;
    digest(alg, &[pad(0x5c), inner].concat())
}

/// The algorithms `hash` could have come from, likeliest first.
fn guess(hash: &str) -> Vec<&'static str> {
    #[rustfmt::skip]
    const PREFIXES: [(&str, &str); 7] = [
        ("$2a$", "bcrypt"), ("$2b$", "bcrypt"), ("$2y$", "bcrypt"), ("$1$", "md5crypt"), ("$5$", "sha256crypt"),
        ("$6$", "sha512crypt"), ("$argon2", "argon2"),
    ];
    if let Some((_, name)) = PREFIXES.iter().find(|(p, _)| hash.starts_with(p)) {
        return vec![name];
    }
    if !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return vec![];
    }
    match hash.len() {
        8 => vec!["crc32"],
        32 => vec!["md5", "ntlm", "md4"],
        40 => vec!["sha1", "ripemd160"],
        64 => vec!["sha256", "sha3-256", "blake2s"],
        96 => vec!["sha384", "sha3-384"],
        128 => vec!["sha512", "sha3-512", "blake2b"],
        _ => vec![],
    }
}

pub(crate) fn crc32(b: &[u8]) -> u32 {
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
        assert_eq!(show(r#"hash_id("abc".sha1)"#), r#"["sha1", "ripemd160"]"#);
        assert_eq!(show(r#"hash_id("$6$salt$xyz")"#), r#"["sha512crypt"]"#);
        assert_eq!(show(r#"hash_id("not a hash")"#), "[]");
        assert_eq!(show(r#"hash_match("abc".sha256.upper, ["a", "abc"])"#), "abc");
        assert_eq!(show(r#"hash_match("abc".md5, ["x"])"#), "nil");
        assert_eq!(show(r#"hash_match(hmac("a", "b", "sha384"), ["q"])"#), "nil");
        assert!(try_eval(r#"hash_match("$2b$xyz", ["a"])"#).is_err());
    }
}
