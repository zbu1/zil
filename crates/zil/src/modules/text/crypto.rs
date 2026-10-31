//! Modern ciphers: AES, DES and Triple DES, ChaCha20 and RC4.

use crate::modules::text::encoding::Data;
use crate::modules::{Fail, Module};
use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit, KeyIvInit, StreamCipher};

pub const MODULE: Module = Module {
    name: "crypto",
    about: "encrypt and decrypt with AES, DES, ChaCha20 and RC4",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Real ciphers, for puzzles and for reading data other programs encrypted. Data, keys and IVs can each be bytes or a \
string, which counts as its UTF-8 bytes."),
        ("good to know", "\
A block cipher encrypts one block at a time, 16 bytes for AES and 8 for DES, and the mode chains them. \"ecb\" \
encrypts each block alone, so equal blocks give equal output. \"cbc\" mixes each block with the one before, starting \
from an IV. Both pad the data with PKCS#7. \"ctr\" turns the cipher into a stream, so nothing is padded, and \"gcm\" \
(AES only) is ctr plus a 16-byte tag at the end that decrypting checks.

Leave out the IV when encrypting and a random one is made and put in front of the output, which is where decrypting \
looks for it when you leave it out there too."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("crypto", &[
            ("encrypt and back", r#"aes_decrypt(aes_encrypt("secret", "YELLOW SUBMARINE"), "YELLOW SUBMARINE").str"#),
            ("ECB leaks repeated blocks", r#"aes_encrypt(b"A" * 32, "YELLOW SUBMARINE", "ecb").hex"#),
            ("RC4 undoes itself", r#"rc4(rc4("attack at dawn", "Key"), "Key").str"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Data, Fail, KeyIvInit, StreamCipher, crypt, raw};

    /// encrypt with AES, the standard block cipher, in mode "ecb", "cbc", "ctr" or "gcm"
    ///
    /// The key is 16, 24 or 32 bytes for AES-128, -192 or -256. The IV is 16 bytes, or 12 for gcm, and without one a random IV goes in front of the output. gcm adds a 16-byte tag at the end
    #[builtin(examples = [r#"aes_encrypt("hi", "YELLOW SUBMARINE", "ecb")"#, r#"aes_encrypt("hi", "YELLOW SUBMARINE", "cbc", b"\x00" * 16).hex"#], see = ["aes_decrypt", "des_encrypt", "pkcs7_pad"])]
    fn aes_encrypt(data: Data, key: Data, #[default("cbc")] mode: &str, iv: Option<Data>) -> Result<Vec<u8>, Fail> {
        crypt("aes", &raw(&data, 0)?, &raw(&key, 1)?, mode, iv.map(|v| raw(&v, 3)).transpose()?, false)
    }

    /// decrypt AES, the reverse of aes_encrypt with the same key, mode and IV
    ///
    /// Without an IV it's read from the front of the data. A wrong key or IV usually shows up as bad padding in ecb and cbc, or a tag that doesn't match in gcm
    #[builtin(examples = [r#"aes_decrypt(aes_encrypt("hi", "YELLOW SUBMARINE", "ctr"), "YELLOW SUBMARINE", "ctr").str"#], see = ["aes_encrypt", "pkcs7_unpad"])]
    fn aes_decrypt(data: Data, key: Data, #[default("cbc")] mode: &str, iv: Option<Data>) -> Result<Vec<u8>, Fail> {
        crypt("aes", &raw(&data, 0)?, &raw(&key, 1)?, mode, iv.map(|v| raw(&v, 3)).transpose()?, true)
    }

    /// encrypt with DES, or Triple DES given a 16- or 24-byte key, in mode "ecb", "cbc" or "ctr"
    ///
    /// DES is long broken, since its 8-byte key has only 56 bits that count, but old systems and puzzles still use it. The IV is 8 bytes
    #[builtin(examples = [r#"des_encrypt("hi", "8bytekey", "ecb").hex"#], see = ["des_decrypt", "aes_encrypt"])]
    fn des_encrypt(data: Data, key: Data, #[default("cbc")] mode: &str, iv: Option<Data>) -> Result<Vec<u8>, Fail> {
        crypt("des", &raw(&data, 0)?, &raw(&key, 1)?, mode, iv.map(|v| raw(&v, 3)).transpose()?, false)
    }

    /// decrypt DES or Triple DES, the reverse of des_encrypt
    #[builtin(examples = [r#"des_decrypt(des_encrypt("hi", "8bytekey"), "8bytekey").str"#], see = ["des_encrypt"])]
    fn des_decrypt(data: Data, key: Data, #[default("cbc")] mode: &str, iv: Option<Data>) -> Result<Vec<u8>, Fail> {
        crypt("des", &raw(&data, 0)?, &raw(&key, 1)?, mode, iv.map(|v| raw(&v, 3)).transpose()?, true)
    }

    /// encrypt or decrypt with ChaCha20 (RFC 8439), a 32-byte key and a 12-byte nonce, as the same call does both
    ///
    /// A stream cipher: it XORs the data with a keystream, so the output is as long as the input. Never reuse a key and nonce pair
    #[builtin(examples = [r#"chacha20("hi", "k" * 32, "n" * 12).hex"#, r#"chacha20(chacha20("hi", "k" * 32, "n" * 12), "k" * 32, "n" * 12).str"#], see = ["rc4", "aes_encrypt"])]
    fn chacha20(data: Data, key: Data, nonce: Data) -> Result<Vec<u8>, Fail> {
        let mut b = raw(&data, 0)?;
        let mut c = chacha20::ChaCha20::new_from_slices(&raw(&key, 1)?, &raw(&nonce, 2)?)
            .map_err(|_| Fail::Msg("ChaCha20 takes a 32-byte key and a 12-byte nonce".into()))?;
        c.apply_keystream(&mut b);
        Ok(b)
    }

    /// encrypt or decrypt with RC4, as the same call does both
    ///
    /// An old stream cipher, broken but still found in CTFs and old protocols. The key is 1 to 256 bytes
    #[builtin(examples = [r#"rc4("Plaintext", "Key").hex"#, r#"rc4(rc4("hi", "Key"), "Key").str"#], see = ["chacha20", "xor"])]
    fn rc4(data: Data, key: Data) -> Result<Vec<u8>, Fail> {
        let key = raw(&key, 1)?;
        if !(1..=256).contains(&key.len()) {
            return Err(Fail::Arg(1, format!("RC4 keys are 1 to 256 bytes, got {}", key.len())));
        }
        let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        let (mut i, mut j) = (0u8, 0u8);
        let out = raw(&data, 0)?.into_iter().map(|x| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            x ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        });
        Ok(out.collect())
    }
}

fn raw(d: &Data, i: usize) -> Result<Vec<u8>, Fail> {
    d.bytes().map_err(|e| match e {
        Fail::Arg(_, m) => Fail::Arg(i, m),
        e => e,
    })
}

/// One block encrypted (false) or decrypted (true) in place.
type Block = Box<dyn Fn(&mut [u8], bool)>;

fn block<C: KeyInit + BlockCipherEncrypt + BlockCipherDecrypt + 'static>(key: &[u8]) -> Block {
    let c = C::new_from_slice(key).expect("key length checked");
    Box::new(move |b, dec| {
        let b = b.try_into().expect("one block");
        if dec { c.decrypt_block(b) } else { c.encrypt_block(b) }
    })
}

/// `data` encrypted or decrypted with `cipher` ("aes" or "des") in `mode`, the IV taken from or put in front of the data when not given.
fn crypt(cipher: &str, data: &[u8], key: &[u8], mode: &str, iv: Option<Vec<u8>>, decrypt: bool) -> Result<Vec<u8>, Fail> {
    let (f, n): (Block, usize) = match (cipher, key.len()) {
        ("aes", 16) => (block::<aes::Aes128>(key), 16),
        ("aes", 24) => (block::<aes::Aes192>(key), 16),
        ("aes", 32) => (block::<aes::Aes256>(key), 16),
        ("aes", k) => return Err(Fail::Arg(1, format!("AES keys are 16, 24 or 32 bytes, got {k}"))),
        (_, 8) => (block::<des::Des>(key), 8),
        (_, 16) => (block::<des::TdesEde2>(key), 8),
        (_, 24) => (block::<des::TdesEde3>(key), 8),
        (_, k) => return Err(Fail::Arg(1, format!("DES keys are 8 bytes, or 16 or 24 for Triple DES, got {k}"))),
    };
    let modes: &[&str] = if cipher == "aes" { &["ecb", "cbc", "ctr", "gcm"] } else { &["ecb", "cbc", "ctr"] };
    if !modes.contains(&mode) {
        return Err(Fail::Arg(2, format!("unknown mode {mode:?}{}", crate::error::did_you_mean(mode, modes.iter().copied()))));
    }
    let iv_len = match mode {
        "ecb" => 0,
        "gcm" => 12,
        _ => n,
    };
    let (iv, data, mut out) = match iv {
        Some(_) if mode == "ecb" => return Err(Fail::Arg(3, "ecb mode takes no IV".into())),
        Some(iv) => (iv, data, vec![]),
        None if decrypt => {
            let (iv, rest) = data.split_at_checked(iv_len).ok_or_else(|| Fail::Arg(0, format!("too short to start with a {iv_len}-byte IV")))?;
            (iv.to_vec(), rest, vec![])
        }
        None => {
            let mut iv = vec![0; iv_len];
            getrandom::getrandom(&mut iv).map_err(|e| Fail::Msg(e.to_string()))?;
            (iv.clone(), data, iv)
        }
    };
    if iv.len() != iv_len {
        return Err(Fail::Arg(3, format!("the IV for {mode} is {iv_len} bytes, got {}", iv.len())));
    }
    if mode == "gcm" {
        let r = match key.len() {
            16 => gcm::<aes::Aes128>(key, &iv, data, decrypt),
            24 => gcm::<aes::Aes192>(key, &iv, data, decrypt),
            _ => gcm::<aes::Aes256>(key, &iv, data, decrypt),
        };
        out.extend(r.map_err(|_| Fail::Msg("the GCM tag doesn't match, so the key, IV or data is wrong".into()))?);
        return Ok(out);
    }
    let mut b = data.to_vec();
    if mode == "ctr" {
        let mut counter = iv;
        for chunk in b.chunks_mut(n) {
            let mut ks = counter.clone();
            f(&mut ks, false);
            chunk.iter_mut().zip(ks).for_each(|(x, k)| *x ^= k);
            for c in counter.iter_mut().rev() {
                *c = c.wrapping_add(1);
                if *c != 0 {
                    break;
                }
            }
        }
        out.extend(b);
        return Ok(out);
    }
    if decrypt {
        if b.is_empty() || !b.len().is_multiple_of(n) {
            return Err(Fail::Arg(0, format!("{mode} data is whole {n}-byte blocks, got {} bytes", b.len())));
        }
        let mut prev = iv;
        for chunk in b.chunks_mut(n) {
            let next = chunk.to_vec();
            f(chunk, true);
            if mode == "cbc" {
                chunk.iter_mut().zip(&prev).for_each(|(x, p)| *x ^= p);
            }
            prev = next;
        }
        let pad = b[b.len() - 1] as usize;
        if !(1..=n).contains(&pad) || !b[b.len() - pad..].iter().all(|&x| x as usize == pad) {
            return Err(Fail::Msg("bad PKCS#7 padding after decrypting, so the key or IV is probably wrong".into()));
        }
        b.truncate(b.len() - pad);
    } else {
        let pad = n - b.len() % n;
        b.extend(std::iter::repeat_n(pad as u8, pad));
        let mut prev = iv;
        for chunk in b.chunks_mut(n) {
            if mode == "cbc" {
                chunk.iter_mut().zip(&prev).for_each(|(x, p)| *x ^= p);
            }
            f(chunk, false);
            prev = chunk.to_vec();
        }
    }
    out.extend(b);
    Ok(out)
}

/// AES-GCM with a 12-byte nonce; the output ends with the tag, which decrypting checks.
fn gcm<C>(key: &[u8], nonce: &[u8], data: &[u8], decrypt: bool) -> aes_gcm::aead::Result<Vec<u8>>
where
    aes_gcm::AesGcm<C, aes_gcm::aead::consts::U12>: KeyInit + aes_gcm::aead::Aead<NonceSize = aes_gcm::aead::consts::U12>,
{
    use aes_gcm::aead::Aead;
    let c = aes_gcm::AesGcm::<C, aes_gcm::aead::consts::U12>::new_from_slice(key).expect("key length checked");
    let nonce = nonce.try_into().expect("12 bytes");
    if decrypt { c.decrypt(nonce, data) } else { c.encrypt(nonce, data) }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    /// Bytes from hex, as zil source.
    fn h(s: &str) -> String {
        format!(r#""{s}".decode("hex")"#)
    }

    #[test]
    fn aes() {
        // FIPS-197 C.1 and NIST SP 800-38A F.2.1, F.5.1.
        let (p, k) = (h("00112233445566778899aabbccddeeff"), h("000102030405060708090a0b0c0d0e0f"));
        assert_eq!(show(&format!(r#"aes_encrypt({p}, {k}, "ecb")[..16].hex"#)), "69c4e0d86a7b0430d8cdb78070b4c55a");
        let (p, k, iv) = (h("6bc1bee22e409f96e93d7e117393172a"), h("2b7e151628aed2a6abf7158809cf4f3c"), h("000102030405060708090a0b0c0d0e0f"));
        assert_eq!(show(&format!(r#"aes_encrypt({p}, {k}, "cbc", {iv})[..16].hex"#)), "7649abac8119b246cee98e9b12e9197d");
        let ctr = h("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
        assert_eq!(show(&format!(r#"aes_encrypt({p}, {k}, "ctr", {ctr}).hex"#)), "874d6191b620e3261bef6864990db6ce");
        for mode in ["ecb", "cbc", "ctr", "gcm"] {
            assert_eq!(show(&format!(r#"aes_decrypt(aes_encrypt("hello world", "k" * 32, "{mode}"), "k" * 32, "{mode}").str"#)), "hello world", "{mode}");
        }
        assert_eq!(show(r#"aes_encrypt("x", "k" * 16, "gcm").len"#), "29");
        assert!(try_eval(r#"aes_decrypt(aes_encrypt("x", "k" * 16, "gcm") + b"!", "k" * 16, "gcm")"#).unwrap_err().msg.contains("tag"));
        assert!(try_eval(r#"aes_decrypt(aes_encrypt("x", "k" * 16), "j" * 16)"#).is_err());
        assert!(try_eval(r#"aes_encrypt("x", "short")"#).unwrap_err().msg.contains("16, 24 or 32"));
        assert!(try_eval(r#"aes_encrypt("x", "k" * 16, "cbc", "short")"#).unwrap_err().msg.contains("16 bytes"));
        assert!(try_eval(r#"aes_encrypt("x", "k" * 16, "ofb")"#).unwrap_err().msg.contains("unknown mode"));
        assert!(try_eval(r#"aes_encrypt("x", "k" * 16, "ecb", "i" * 16)"#).is_err());
    }

    #[test]
    fn gcm() {
        // NIST GCM test case 3.
        let (k, iv) = (h("feffe9928665731c6d6a8f9467308308"), h("cafebabefacedbaddecaf888"));
        let p = h("d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255");
        assert_eq!(show(&format!(r#"aes_encrypt({p}, {k}, "gcm", {iv})[-16..].hex"#)), "4d5c2af327cd64a62cf35abd2ba6fab4");
    }

    #[test]
    fn des() {
        assert_eq!(show(&format!(r#"des_encrypt({}, {}, "ecb")[..8].hex"#, h("0123456789abcde7"), h("0123456789abcdef"))), "c95744256a5ed31d");
        for key in [r#""k" * 8"#, r#""k" * 16"#, r#""k" * 24"#] {
            assert_eq!(show(&format!(r#"des_decrypt(des_encrypt("hello", {key}), {key}).str"#)), "hello");
        }
        assert!(try_eval(r#"des_encrypt("x", "k" * 8, "gcm")"#).is_err());
    }

    #[test]
    fn stream() {
        assert_eq!(show(r#"rc4("Plaintext", "Key").hex"#), "bbf316e8d940af0ad3");
        assert_eq!(show(r#"rc4(rc4(b"\x00\xff", "Wiki"), "Wiki")"#), r#"b"\0\xff""#);
        assert!(try_eval(r#"rc4("x", "")"#).is_err());
        // RFC 8439 A.2 #1, block counter 0.
        assert_eq!(show(r#"chacha20(b"\x00" * 64, b"\x00" * 32, b"\x00" * 12)[..16].hex"#), "76b8e0ada0f13d90405d6ae55386bd28");
        assert!(try_eval(r#"chacha20("x", "short", "n" * 12)"#).is_err());
    }
}
