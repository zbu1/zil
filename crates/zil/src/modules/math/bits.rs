//! Bit twiddling on integers, two's complement for negatives.

use crate::ast::{Radix, Target};
use crate::modules::{Claim, Fail, Module};
use crate::value::{Value, fits, mask, ratio};
use num_bigint::BigInt;
use num_traits::Signed;
use std::rc::Rc;

pub const MODULE: Module = Module {
    name: "bits",
    about: "the individual bits of an integer: count, test, set, clear, rotate and swap them, plus fixed-point and binary-coded decimal",
    #[rustfmt::skip]
    intro: &[
        ("", "\
The individual bits of an integer, and the number formats built on them: fixed point, which keeps a fixed number of bits after the point, and binary-coded decimal, which spends four bits on each decimal digit."),
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
    syntax: &[("to bits shows binary, grouped in fours", "0xf0 to bits")],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("bcd", "bcd")],
    convert: Some(convert),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{fixed_width, low_bits, unsigned};
    use crate::ast::Radix;
    use crate::modules::Fail;
    use crate::modules::math::int;
    use crate::signature::{As, Int};
    use crate::value::{Value, exact, mask};
    use num_rational::BigRational;
    use num_traits::Signed;

    /// A small int and the base it shows in.
    fn small(n: &Int) -> Result<(i64, Radix), Fail> {
        if let Value::Int(n, r) = **n { Ok((n, r)) } else { Err(Fail::BadArgs) }
    }

    /// how many bits are 1
    ///
    /// A negative number counts as its two's complement
    #[builtin(examples = ["popcount(0b1011)", "popcount(-1)"], see = ["bit"])]
    fn popcount(n: Int) -> Result<i64, String> {
        match &*n {
            Value::Int(n, _) => Ok(n.count_ones() as i64),
            Value::Big(n, _) if !n.is_negative() => Ok(n.magnitude().count_ones() as i64),
            _ => Err("negative big ints have infinitely many 1 bits".into()),
        }
    }

    /// bit i of n, as 0 or 1, where bit 0 is the lowest
    #[builtin(examples = ["bit(0b100, 2)"], see = ["set_bit", "clear_bit", "popcount"])]
    fn bit(n: Int, i: i64) -> Result<i64, Fail> {
        // Through BigInt so bits past 63 work; two's complement, like the i64 ops.
        if i < 0 {
            return Err(Fail::BadArgs);
        }
        Ok(int(&n)?.bit(i as u64) as i64)
    }

    fn set(n: Int, i: i64, on: bool) -> Result<As<Int>, Fail> {
        if i < 0 {
            return Err(Fail::BadArgs);
        }
        let r = match &*n {
            Value::Int(_, r) | Value::Big(_, r) => *r,
            _ => unreachable!("an Int holds an int"),
        };
        let mut b = int(&n)?;
        crate::state::check_bits(i as u64 + 1)?;
        b.set_bit(i as u64, on);
        Ok(exact(BigRational::from_integer(b), r, false).into())
    }

    /// n with bit i turned on (set to 1)
    #[builtin(examples = ["set_bit(0b1, 4)"], see = ["clear_bit", "bit"])]
    fn set_bit(n: Int, i: i64) -> Result<As<Int>, Fail> {
        set(n, i, true)
    }

    /// n with bit i turned off (set to 0)
    #[builtin(examples = ["clear_bit(0xff, 0)"], see = ["set_bit", "bit"])]
    fn clear_bit(n: Int, i: i64) -> Result<As<Int>, Fail> {
        set(n, i, false)
    }

    fn rotate(n: Int, k: i64, width: i64, right: bool) -> Result<As<Int>, Fail> {
        let (n, r) = small(&n)?;
        let (m, w) = low_bits(n, width)?;
        let k = k.rem_euclid(w as i64) as u32;
        let k = if right { (w - k) % w } else { k };
        let v = if k == 0 { m } else { (m << k | m >> (w - k)) & mask(w) };
        Ok(unsigned(v, r).into())
    }

    /// treat n as a width-bit number and rotate it left by k
    ///
    /// Bits pushed off the top come back in at the bottom, where << would drop them
    #[builtin(examples = ["rotl(0x81, 1, 8)", "rotl(0x80000000, 1, 32)"], see = ["rotr"])]
    fn rotl(n: Int, k: i64, width: i64) -> Result<As<Int>, Fail> {
        rotate(n, k, width, false)
    }

    /// treat n as a width-bit number and rotate it right by k
    ///
    /// Bits pushed off the bottom come back in at the top, where >> would drop them
    #[builtin(examples = ["rotr(0x81, 1, 8)"], see = ["rotl"])]
    fn rotr(n: Int, k: i64, width: i64) -> Result<As<Int>, Fail> {
        rotate(n, k, width, true)
    }

    /// reverse the order of the bytes in a width-bit number, converting between little- and big-endian
    #[builtin(examples = ["byteswap(0x1234, 16)", "byteswap(0x12345678, 32)"], see = ["rotl"])]
    fn byteswap(n: Int, width: i64) -> Result<As<Int>, Fail> {
        let (n, r) = small(&n)?;
        if width % 8 != 0 {
            return Err(Fail::BadArgs);
        }
        let (m, w) = low_bits(n, width)?;
        Ok(unsigned(m.swap_bytes() >> (64 - w), r).into())
    }

    /// x as a fixed-point integer with int_bits whole bits and frac_bits fractional ones, rounded, shown in hex
    ///
    /// The whole bits include the sign, so `to_fixed(1.5, 8, 8)` is the 16-bit number 0x0180
    #[builtin(examples = ["to_fixed(1.5, 8, 8)", "to_fixed(-0.25, 4, 4)"], see = ["from_fixed"])]
    fn to_fixed(x: f64, int_bits: i64, frac_bits: i64) -> Result<As<Int>, Fail> {
        let w = fixed_width(int_bits, frac_bits)?;
        let n = (x * 2f64.powi(frac_bits as i32)).round_ties_even();
        if !n.is_finite() || n < -(2f64.powi(w as i32 - 1)) || n >= 2f64.powi(w as i32 - 1) {
            return Err(Fail::Arg(0, format!("{x} does not fit in {int_bits}.{frac_bits} fixed point")));
        }
        Ok(Value::Int(n as i64, Radix { base: 16, width: w }).into())
    }

    /// the number a fixed-point integer with int_bits whole bits and frac_bits fractional ones stands for
    #[builtin(examples = ["from_fixed(0x0180, 8, 8)", "from_fixed(0xf0, 4, 4)"], see = ["to_fixed"])]
    fn from_fixed(n: Int, int_bits: i64, frac_bits: i64) -> Result<f64, Fail> {
        let w = fixed_width(int_bits, frac_bits)?;
        let (n, _) = small(&n)?;
        let (m, _) = low_bits(n, w as i64)?;
        let signed = if m >> (w - 1) == 1 { m as i64 - (1i128 << w) as i64 } else { m as i64 };
        Ok(signed as f64 / 2f64.powi(frac_bits as i32))
    }

    /// a non-negative integer in binary-coded decimal, where each decimal digit takes one hex digit, so 1234 is 0x1234
    #[builtin(examples = ["1234 to bcd", "bcd(59)"], see = ["from_bcd"])]
    fn bcd(n: i64) -> Result<As<Int>, Fail> {
        if !(0..=999_999_999_999_999).contains(&n) {
            return Err(Fail::Arg(0, "BCD takes a number from 0 to 15 digits".into()));
        }
        Ok(Value::Int(i64::from_str_radix(&n.to_string(), 16).expect("decimal digits are hex digits"), Radix::HEX).into())
    }

    /// the number a binary-coded decimal integer stands for, the reverse of bcd
    #[builtin(examples = ["from_bcd(0x1234)"], see = ["bcd"])]
    fn from_bcd(n: Int) -> Result<As<Int>, Fail> {
        let (n, _) = small(&n)?;
        let text = format!("{n:x}");
        if n < 0 || text.bytes().any(|b| !b.is_ascii_digit()) {
            return Err(Fail::Arg(
                0,
                format!(
                    "{n:#x} is not binary-coded decimal
note: every hex digit must be 0 to 9"
                ),
            ));
        }
        Ok(Value::int(text.parse().expect("decimal digits")).into())
    }
}

fn fixed_width(int_bits: i64, frac_bits: i64) -> Result<u32, Fail> {
    if int_bits < 1 || frac_bits < 0 || int_bits + frac_bits > 64 {
        return Err(Fail::BadArgs);
    }
    Ok((int_bits + frac_bits) as u32)
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
    let groups: Vec<_> = d.as_bytes().chunks(4).map(|g| std::str::from_utf8(g).unwrap_or_default()).collect();
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
    fn fixed_and_bcd() {
        assert_eq!(show("[to_fixed(1.5, 8, 8), to_fixed(-0.25, 4, 4), from_fixed(0x0180, 8, 8), from_fixed(0xf0, 4, 4)]"), "[0x0180, 0xfc, 1.5, -1]");
        assert_eq!(show("[1234 to bcd, from_bcd(0x1234), from_bcd(1234 to bcd)]"), "[0x1234, 1234, 1234]");
        assert!(try_eval("to_fixed(200, 8, 8)").is_err());
        assert!(try_eval("from_bcd(0x1a)").is_err());
    }

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
