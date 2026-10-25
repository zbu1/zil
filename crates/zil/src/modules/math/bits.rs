//! Bit twiddling on integers, two's complement for negatives.

use super::int;
use crate::ast::{Radix, Target};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::value::{Value, exact, fits, mask, ratio};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "bits",
    about: "the individual bits of an integer: count, test, set, clear, rotate and swap them",
    #[rustfmt::skip]
    intro: &[
        ("", "\
The individual bits of an integer."),
        ("writing", "\
The operators `&`, `|`, `^` and `~` are bitwise and, or, xor and not, and `<<` and `>>` shift bits left and right."),
        ("good to know", "\
Bits are numbered from 0, the lowest (rightmost in binary), so bit 0 of `0b100` is 0 and bit 2 is 1."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("bits", &[
            ("xor: bits that differ", "0b1010 ^ 0b0110"),
            ("how many bits are 1", "popcount(0xff)"),
            ("rotate within 8 bits", "rotr(1, 1, 8)"),
            ("swap byte order", "byteswap(0x1234, 16)"),
            ("clear a bit", "clear_bit(0xff, 0)"),
        ]),
    ],
    guide: &[("conversions", &[("to bits shows binary, grouped in fours", "0xf0 to bits")])],
    fns: FNS,
    call,
    convert: Some(convert),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("popcount", "popcount(n: int) -> int", "how many bits are 1", &["popcount(0b1011)", "popcount(-1)"], &["bit"])
        .detail("A negative number counts as its two's complement"),
    doc("bit", "bit(n: int, i: int) -> int", "bit i of n, as 0 or 1, where bit 0 is the lowest", &["bit(0b100, 2)"], &["set_bit", "clear_bit", "popcount"]),
    doc("set_bit", "set_bit(n: int, i: int) -> int", "n with bit i turned on (set to 1)", &["set_bit(0b1, 4)"], &["clear_bit", "bit"]),
    doc("clear_bit", "clear_bit(n: int, i: int) -> int", "n with bit i turned off (set to 0)", &["clear_bit(0xff, 0)"], &["set_bit", "bit"]),
    doc("rotl", "rotl(n: int, k: int, width: int) -> int", "treat n as a width-bit number and rotate it left by k", &["rotl(0x81, 1, 8)", "rotl(0x80000000, 1, 32)"], &["rotr"])
        .detail("Bits pushed off the top come back in at the bottom, where << would drop them"),
    doc("rotr", "rotr(n: int, k: int, width: int) -> int", "treat n as a width-bit number and rotate it right by k", &["rotr(0x81, 1, 8)"], &["rotl"])
        .detail("Bits pushed off the bottom come back in at the top, where >> would drop them"),
    doc("byteswap", "byteswap(n: int, width: int) -> int", "reverse the order of the bytes in a width-bit number, converting between little- and big-endian", &["byteswap(0x1234, 16)", "byteswap(0x12345678, 32)"], &["rotl"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("popcount", [Int(n, _)]) => Value::int(n.count_ones() as i64),
        ("popcount", [Big(n, _)]) if !n.is_negative() => Value::int(n.magnitude().count_ones() as i64),
        ("popcount", [Big(..)]) => {
            return Err("negative big ints have infinitely many 1 bits".into());
        }
        // Through BigInt so bits past 63 work; two's complement, like the i64 ops.
        ("bit", [n @ (Int(..) | Big(..)), Int(i, _)]) if *i >= 0 => Value::int(int(n)?.bit(*i as u64) as i64),
        ("set_bit" | "clear_bit", [n @ (Int(_, r) | Big(_, r)), Int(i, _)]) if *i >= 0 => {
            let mut b = int(n)?;
            crate::state::check_bits(*i as u64 + 1)?;
            b.set_bit(*i as u64, name == "set_bit");
            exact(BigRational::from_integer(b), *r, false)
        }
        ("rotl" | "rotr", [Int(n, r), Int(k, _), Int(w, _)]) => {
            let (m, w) = low_bits(*n, *w)?;
            let k = k.rem_euclid(w as i64) as u32;
            let k = if name == "rotr" { (w - k) % w } else { k };
            let v = if k == 0 { m } else { (m << k | m >> (w - k)) & mask(w) };
            unsigned(v, *r)
        }
        ("byteswap", [Int(n, r), Int(w, _)]) if w % 8 == 0 => {
            let (m, w) = low_bits(*n, *w)?;
            unsigned(m.swap_bytes() >> (64 - w), *r)
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// `0xf0 to bits` -> `"0b1111_0000"`: binary in groups of four. `bits` is also a unit, so this hooks
/// `convert` rather than registering a target.
fn convert(v: &Value, t: &Target) -> Claim {
    let n: BigInt = match t {
        Target::Unit(spec) if *spec == [("bits".to_string(), 1)] => ratio(v).filter(|r| r.is_integer())?.to_integer(),
        _ => return None,
    };
    let d = n.magnitude().to_str_radix(2);
    let d = format!("{d:0>w$}", w = d.len().div_ceil(4) * 4);
    let groups: Vec<_> = d.as_bytes().chunks(4).map(|g| std::str::from_utf8(g).unwrap()).collect();
    Some(Ok(Value::str(format!("{}0b{}", if n.is_negative() { "-" } else { "" }, groups.join("_")))))
}

/// `n` as its low `w` bits, unsigned; `n` must fit in `w` bits, signed or unsigned.
fn low_bits(n: i64, w: i64) -> Result<(u64, u32), Fail> {
    if !(1..=64).contains(&w) {
        return Err(format!("width {w} is out of range\nnote: widths go from 1 to 64 bits").into());
    }
    let w = w as u32;
    if !fits(n, w) {
        return Err(Fail::Arg(0, format!("`{n}` does not fit in {w} bits")));
    }
    Ok((n as u64 & mask(w), w))
}

fn unsigned(v: u64, r: Radix) -> Value {
    let r = Radix { width: 0, ..r };
    i64::try_from(v).map_or_else(|_| Value::Big(Rc::new(v.into()), r), |n| Value::Int(n, r))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn bits() {
        assert_eq!(show("[popcount(0b1011), popcount(-1), bit(0b100, 2), bit(0b100, 1)]"), "[3, 64, 1, 0]");
        assert_eq!(show("[set_bit(0b1, 4), clear_bit(0xff, 0)]"), "[0b10001, 0xfe]");
        assert_eq!(show("[popcount(2 ** 64 - 1), bit(2 ** 70, 70), bit(-1, 100)]"), "[64, 1, 1]");
        assert_eq!(show("[set_bit(0x0, 64), clear_bit(2 ** 70 + 1, 70)]"), "[0x10000000000000000, 1]");
        assert!(try_eval("popcount(-(2 ** 70))").is_err());
        assert_eq!(
            show("[rotl(0x81, 1, 8), rotr(0x81, 1, 8), rotl(0x81, 9, 8), rotl(-1, 4, 8), rotr(1, 1, 64)]"),
            "[0x3, 0xc0, 0x3, 255, 9223372036854775808]"
        );
        assert_eq!(show("[byteswap(0x1234, 16), byteswap(0x12345678, 32), byteswap(1, 64)]"), "[0x3412, 0x78563412, 72057594037927936]");
        assert_eq!(show("[0xf0 to bits, 5 to bits, 0 to bits]"), r#"["0b1111_0000", "0b0101", "0b0000"]"#);
        assert_eq!(show("1 B to bits"), "8 bit");
        assert!(try_eval("rotl(256, 1, 8)").is_err());
        assert!(try_eval("byteswap(1, 12)").is_err());
    }
}
