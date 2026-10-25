//! Packing: ints to bytes and back, xor, and cyclic patterns for finding offsets, as in pwntools.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::{Value, fits, mask};
use num_traits::ToPrimitive;
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "packing",
    about: "integers as raw bytes and back, xor, and crash-offset patterns, as in pwntools",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Integers as the raw bytes a CPU stores for them, and back, as when writing exploits for CTF challenges. `p32(x)` gives \
4 bytes and `p64(x)` 8, and `u8` to `u64` read bytes back into an integer."),
        ("good to know", "\
Most CPUs, including x86 and ARM, store the lowest byte first, which is called little-endian, so that's the default. \
Pass `big` for big-endian, the order network protocols use. Negative numbers come out in two's complement, the way \
CPUs store them, so `p8(-1)` is the byte 0xff."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("packing", &[
            ("an address, lowest byte first", "p32(0xdeadbeef)"),
            ("and back", r#"u32(b"\xef\xbe\xad\xde") to hex"#),
            ("a port in network byte order", "p16(443, big)"),
            ("how far into the input did the crash land?", "cyclic_find(0x6161616c)"),
        ]),
    ],
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("pack", &["p8", "p16", "p32", "p64"]),
        ("unpack", &["u8", "u16", "u32", "u64"]),
        ("patterns", &["xor", "cyclic", "cyclic_find"]),
    ],
    call,
    ident: Some(ident),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("p8", "p8(x: int, order?: str) -> bytes", "x as 1 byte, with -128 to -1 come out in two's complement", &["p8(0x41)", "p8(-1)"], &["u8", "p32"]),
    doc("p16", "p16(x: int, order?: str) -> bytes", "x as 2 bytes, lowest byte first unless order is big", &["p16(0x4142)", "p16(443, big)"], &["u16", "p32"]),
    doc("p32", "p32(x: int, order?: str) -> bytes", "x as 4 bytes, lowest byte first unless order is big", &["p32(0xdeadbeef)", "p32(-2)"], &["u32", "p64"]),
    doc("p64", "p64(x: int, order?: str) -> bytes", "x as 8 bytes, lowest byte first unless order is big", &["p64(0x401000)"], &["u64", "p32"]),
    doc("u8", "u8(b: bytes, order?: str) -> int", "1 byte back to an integer from 0 to 255", &[r#"u8(b"A")"#], &["p8"]),
    doc("u16", "u16(b: bytes, order?: str) -> int", "2 bytes back to a non-negative integer, lowest byte first unless order is big", &[r#"u16(b"\x01\xbb", big)"#], &["p16"]),
    doc("u32", "u32(b: bytes, order?: str) -> int", "4 bytes back to a non-negative integer, lowest byte first unless order is big", &[r#"u32(b"\xef\xbe\xad\xde") to hex"#], &["p32"]),
    doc("u64", "u64(b: bytes, order?: str) -> int", "8 bytes back to a non-negative integer, lowest byte first unless order is big", &["u64(p64(-1))"], &["p64"]),
    doc("xor", "xor(b: bytes|str, key: bytes|str|int) -> bytes", "xor each byte with the key, repeating the key as often as needed", &[r#"b"hello".xor(0x20)"#, r#""secret".xor("k").xor("k").str"#], &["bytes"])
        .detail("An int key is a single byte. Xor-ing again with the same key gives back the original"),
    doc("cyclic", "cyclic(n: int) -> bytes", "n bytes of a pattern in which every run of 4 bytes is different (a de Bruijn sequence), to send to a program with a buffer overflow and see which part ends up where", &["cyclic(16)"], &["cyclic_find"]),
    doc("cyclic_find", "cyclic_find(sub: bytes|str|int) -> int|nil", "how many bytes into the cyclic pattern sub appears, or nil",
        &[r#"cyclic_find(b"baaa")"#, "cyclic_find(0x6161616c)"], &["cyclic"])
        .detail("After a crash, give it the value a register ended up holding to learn how many bytes into your input that was. An int is taken as the bytes it would be in memory, lowest first: 4 of them, or 8 if it's too big for 4, as when you copy a register's value from a debugger"),
];

/// `big` and `little`, so `p32(x, big)` needs no quotes; a variable of either name wins.
fn ident(_: &mut Interp, name: &str) -> Claim {
    matches!(name, "big" | "little").then(|| Ok(Value::str(name)))
}

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let width = |n: &str| n[1..].parse::<u32>().ok();
    Ok(match (name, args) {
        ("p8" | "p16" | "p32" | "p64", [x, rest @ ..]) if rest.len() <= 1 => {
            let w = width(name).unwrap();
            let n = match x {
                Int(n, _) if fits(*n, w) => *n as u64 & mask(w),
                Big(n, _) if w == 64 && n.to_u64().is_some() => n.to_u64().unwrap(),
                Int(..) | Big(..) => return Err(Fail::Arg(0, format!("{x} does not fit in {w} bits"))),
                _ => return Err(Fail::BadArgs),
            };
            let le = n.to_le_bytes();
            let mut b = le[..w as usize / 8].to_vec();
            if big_endian(rest)? {
                b.reverse();
            }
            Value::bytes(b)
        }
        ("u8" | "u16" | "u32" | "u64", [Bytes(b), rest @ ..]) if rest.len() <= 1 => {
            let n = width(name).unwrap() as usize / 8;
            if b.len() != n {
                return Err(Fail::Arg(0, format!("expected {n} byte{}, got {}", if n == 1 { "" } else { "s" }, b.len())));
            }
            let mut le = [0; 8];
            le[..n].copy_from_slice(b);
            if big_endian(rest)? {
                le[..n].reverse();
            }
            let x = u64::from_le_bytes(le);
            i64::try_from(x).map_or_else(|_| Big(Rc::new(x.into()), crate::ast::Radix::DEC), Value::int)
        }
        ("xor", [b @ (Bytes(_) | Str(_)), key]) => {
            let key = match key {
                Int(k, _) => vec![u8::try_from(*k).map_err(|_| Fail::Arg(1, format!("a key byte goes from 0 to 255, got {k}")))?],
                Bytes(_) | Str(_) => crate::modules::text::encoding::bytes(key).unwrap(),
                _ => return Err(Fail::BadArgs),
            };
            if key.is_empty() {
                return Err(Fail::Arg(1, "the key is empty".into()));
            }
            let b = crate::modules::text::encoding::bytes(b).unwrap();
            Value::bytes(b.iter().zip(key.iter().cycle()).map(|(x, k)| x ^ k).collect::<Vec<u8>>())
        }
        ("cyclic", [Int(n, _)]) => {
            let all = de_bruijn();
            let n = usize::try_from(*n).ok().filter(|n| *n <= all.len()).ok_or_else(|| Fail::Arg(0, format!("n goes from 0 to {}", all.len())))?;
            Value::bytes(&all[..n])
        }
        ("cyclic_find", [sub]) => {
            let sub = match sub {
                Int(n, _) => match u32::try_from(*n) {
                    Ok(n) => n.to_le_bytes().to_vec(),
                    Err(_) => n.to_le_bytes().to_vec(),
                },
                Bytes(_) | Str(_) => crate::modules::text::encoding::bytes(sub).unwrap(),
                _ => return Err(Fail::BadArgs),
            };
            if sub.is_empty() {
                return Err(Fail::Arg(0, "nothing to find".into()));
            }
            de_bruijn().windows(sub.len()).position(|w| w == sub).map_or(Nil, |i| Value::int(i as i64))
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// The optional byte order argument: `big` or `little`.
fn big_endian(rest: &[Value]) -> Result<bool, Fail> {
    match rest {
        [] => Ok(false),
        [Value::Str(s)] if &**s == "little" => Ok(false),
        [Value::Str(s)] if &**s == "big" => Ok(true),
        [v] => Err(Fail::Arg(1, format!("byte order is big or little, not {v:?}"))),
        _ => Err(Fail::BadArgs),
    }
}

/// The de Bruijn sequence over a-z with unique 4-byte windows: 26^4 bytes, the same one pwntools' `cyclic` makes.
fn de_bruijn() -> Vec<u8> {
    const K: u8 = 26;
    const N: usize = 4;
    fn db(t: usize, p: usize, a: &mut [u8; N + 1], out: &mut Vec<u8>) {
        if t > N {
            if N.is_multiple_of(p) {
                out.extend(a[1..=p].iter().map(|j| b'a' + j));
            }
            return;
        }
        a[t] = a[t - p];
        db(t + 1, p, a, out);
        for j in a[t - p] + 1..K {
            a[t] = j;
            db(t + 1, t, a, out);
        }
    }
    let mut out = Vec::with_capacity(26usize.pow(N as u32));
    db(1, 1, &mut [0; N + 1], &mut out);
    out
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn packing() {
        assert_eq!(show("[p32(0xdeadbeef), p16(443, big), p8(-1), p64(-1).len]"), r#"[b"\xef\xbe\xad\xde", b"\x01\xbb", b"\xff", 8]"#);
        assert_eq!(show("[u32(p32(0x41424344)), u16(p16(1, big), big), u64(p64(-1)), u8(b\"A\")]"), "[1094861636, 1, 18446744073709551615, 65]");
        assert_eq!(show("p64(u64(p64(-1)))"), r#"b"\xff\xff\xff\xff\xff\xff\xff\xff""#);
        assert_eq!(show(r#"[b"hello".xor(0x20), "ab".xor(b"\x01\x02"), b"secret".xor("key").xor("key").str]"#), r#"[b"HELLO", b"``", "secret"]"#);
        assert_eq!(show("cyclic(16)"), r#"b"aaaabaaacaaadaaa""#);
        assert_eq!(show(r#"[cyclic_find(b"baaa"), cyclic_find(0x6161616c), cyclic_find("zzzz!")]"#), "[4, 44, nil]");
        assert_eq!(show("c = cyclic(26 ** 4); [c.len, [*c].set.len]"), "[456976, 26]");
        assert_eq!(show("big = 5; big"), "5");
        for bad in ["p8(256)", "p32(2 ** 32)", "u32(b\"abc\")", "p32(1, \"middle\")", "b\"a\".xor(256)", "b\"a\".xor(\"\")", "cyclic(-1)"] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }
}
