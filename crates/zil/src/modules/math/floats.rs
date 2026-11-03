//! IEEE 754 floating-point formats: bit patterns, parts and rounding to a narrower format.

use crate::modules::{Fail, Module};
use crate::signature::Or;
use crate::value::{Value, exact};
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::ToPrimitive;

pub const MODULE: Module = Module {
    name: "floats",
    tags: &["binary", "numbers"],
    about: "how computers store decimals: the bits of 16, 32, 64, 80 and 128-bit floats, and rounding to a smaller format",
    #[rustfmt::skip]
    intro: &[
        ("", "\
How computers store numbers with decimals. A float is a sign, an exponent and a mantissa packed into 16, 32, 64, 80 or 128 bits, \
and most decimals like 0.1 can only be stored approximately."),
        ("good to know", "\
Name a format by its width: `16` (half), `32` (single), `64` (double, what zil uses), `80` (x87 extended) or `128` (quad), \
or `\"bf16\"` for the 16-bit brain float that machine learning uses. zil's own numbers are 64-bit, so 80 and 128-bit values \
are rounded to 64 bits when read back."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("floats", &[
            ("the bits of a 32-bit float", "float_bits(52.345, 32)"),
            ("the parts of a double", "float_parts(0.1)"),
            ("what 0.1 becomes in 32 bits", "nearest_float(0.1, 32)"),
            ("and how far off that is", "float_error(0.1, 32)"),
        ]),
    ],
    syntax: &[("to fp(32): rounded to the nearest 32-bit float", "0.1 to fp(32)")],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("fp", "nearest_float")],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{bits_value, decode, encode, layout};
    use crate::modules::Fail;
    use crate::signature::{Int, Or};
    use crate::value::Value;
    use indexmap::IndexMap;
    use num_traits::ToPrimitive;

    /// the bit pattern of x in a float format, shown in hex
    ///
    /// The format is a width: 16, 32, 64, 80 or 128, or "bf16"
    #[builtin(examples = ["float_bits(52.345, 32)", "float_bits(1, 16)", "float_bits(-2.5)"], see = ["from_float_bits", "float_parts"])]
    fn float_bits(x: f64, format: Option<Or<i64, &str>>) -> Result<Value, Fail> {
        let f = layout(format.as_ref())?;
        Ok(bits_value(encode(x, f), f.total(), 16))
    }

    /// the float with the given bit pattern, the reverse of float_bits
    #[builtin(examples = ["from_float_bits(0x42516148, 32)", "from_float_bits(0x3c00, 16)"], see = ["float_bits"])]
    fn from_float_bits(bits: Int, format: Option<Or<i64, &str>>) -> Result<f64, Fail> {
        let f = layout(format.as_ref())?;
        let n = match &*bits {
            Value::Int(n, _) => (*n as i128 & ((1i128 << 64) - 1)) as u128 & if f.total() >= 64 { u128::MAX } else { (1u128 << f.total()) - 1 },
            Value::Big(n, _) => {
                n.to_u128().filter(|n| f.total() == 128 || n >> f.total() == 0).ok_or(Fail::Arg(0, format!("does not fit in {} bits", f.total())))?
            }
            _ => unreachable!("an Int holds an int"),
        };
        if matches!(&*bits, Value::Int(n, _) if f.total() < 64 && (*n < 0 || (*n as u64) >> f.total() != 0)) {
            return Err(Fail::Arg(0, format!("{} does not fit in {} bits", *bits, f.total())));
        }
        Ok(decode(n, f))
    }

    /// the sign, exponent and mantissa fields of x as stored, as a map
    ///
    /// The exponent is the stored (biased) one, and the mantissa excludes the hidden leading 1
    #[builtin(examples = ["float_parts(0.1)", "float_parts(1.5, 32)"], see = ["float_bits"])]
    fn float_parts(x: f64, format: Option<Or<i64, &str>>) -> Result<IndexMap<String, Value>, Fail> {
        let f = layout(format.as_ref())?;
        let b = encode(x, f);
        let mantissa = b & ((1u128 << f.mb) - 1);
        let exponent = (b >> f.mfield()) & ((1u128 << f.eb) - 1);
        Ok(IndexMap::from([
            ("sign".to_string(), Value::int((b >> (f.total() - 1)) as i64)),
            ("exponent".to_string(), Value::int(exponent as i64)),
            ("mantissa".to_string(), bits_value(mantissa, 0, 16)),
        ]))
    }

    /// x rounded to the nearest number a float format can store
    ///
    /// Same as `x to fp(bits)`
    #[builtin(examples = ["nearest_float(0.1, 32)", "nearest_float(100000, 16)", "0.1 to fp(16)"], see = ["float_error"])]
    fn nearest_float(x: f64, format: Or<i64, &str>) -> Result<f64, Fail> {
        let f = layout(Some(&format))?;
        Ok(decode(encode(x, f), f))
    }

    /// how far x is from the nearest number a float format can store, as nearest minus x
    #[builtin(examples = ["float_error(0.1, 32)", "float_error(0.5, 16)"], see = ["nearest_float"])]
    fn float_error(x: f64, format: Or<i64, &str>) -> Result<f64, Fail> {
        let f = layout(Some(&format))?;
        Ok(decode(encode(x, f), f) - x)
    }
}

/// A float layout: exponent bits, mantissa bits (without a hidden bit), and whether the leading 1 is stored (x87 80-bit).
#[derive(Clone, Copy)]
struct Fmt {
    eb: u32,
    mb: u32,
    explicit: bool,
}

impl Fmt {
    fn mfield(self) -> u32 {
        self.mb + self.explicit as u32
    }
    fn total(self) -> u32 {
        1 + self.eb + self.mfield()
    }
}

fn layout(f: Option<&Or<i64, &str>>) -> Result<Fmt, Fail> {
    let (eb, mb, explicit) = match f.unwrap_or(&Or::A(64)) {
        Or::A(16) => (5, 10, false),
        Or::A(32) => (8, 23, false),
        Or::A(64) => (11, 52, false),
        Or::A(80) => (15, 63, true),
        Or::A(128) => (15, 112, false),
        Or::B("bf16") => (8, 7, false),
        _ => return Err(Fail::Arg(1, "the format is 16, 32, 64, 80, 128 or \"bf16\"".into())),
    };
    Ok(Fmt { eb, mb, explicit })
}

/// An unsigned bit pattern as an int: a plain `Int` of `width` bits when it fits in 64, else a big one.
fn bits_value(n: u128, width: u32, base: u32) -> Value {
    if width <= 64 {
        Value::Int(n as i64, crate::ast::Radix { base, width })
    } else {
        exact(BigRational::from_integer(BigInt::from(n)), crate::ast::Radix { base, width: 0 }, false)
    }
}

fn ldexp(mut x: f64, mut e: i32) -> f64 {
    while e > 1000 {
        x *= 2f64.powi(1000);
        e -= 1000;
    }
    while e < -1000 {
        x *= 2f64.powi(-1000);
        e += 1000;
    }
    x * 2f64.powi(e)
}

/// `x` rounded to nearest-even in format `f`, as its bit pattern.
fn encode(x: f64, f: Fmt) -> u128 {
    let sign = (x.is_sign_negative() as u128) << (f.total() - 1);
    let emax = (1u128 << f.eb) - 1;
    let int_bit = if f.explicit { 1u128 << f.mb } else { 0 };
    if x.is_nan() {
        return sign | emax << f.mfield() | int_bit | 1 << (f.mb - 1);
    }
    if x.is_infinite() {
        return sign | emax << f.mfield() | int_bit;
    }
    if x == 0.0 {
        return sign;
    }
    let b = x.abs().to_bits();
    let (m, ex) = match b >> 52 {
        0 => (b & ((1 << 52) - 1), -1074i32),
        e => ((b & ((1 << 52) - 1)) | 1 << 52, e as i32 - 1075),
    };
    let bias = (1i32 << (f.eb - 1)) - 1;
    let mut e = (ex + 63 - m.leading_zeros() as i32).max(1 - bias);
    let sh = ex + f.mb as i32 - e;
    let (num, den) = if sh >= 0 { (BigInt::from(m) << sh, BigInt::from(1)) } else { (BigInt::from(m), BigInt::from(1) << -sh) };
    let (mut q, r) = num.div_rem(&den);
    let twice = r * 2;
    if twice > den || (twice == den && q.is_odd()) {
        q += 1;
    }
    let mut q = q.to_u128().expect("the rounded mantissa fits in 113 bits");
    if q == 1 << (f.mb + 1) {
        q >>= 1;
        e += 1;
    }
    let normal = q >= 1 << f.mb;
    let biased = if normal { e + bias } else { 0 } as i128;
    if biased >= emax as i128 {
        return sign | emax << f.mfield() | int_bit;
    }
    let stored = if f.explicit || !normal { q } else { q - (1 << f.mb) };
    sign | (biased as u128) << f.mfield() | stored
}

fn decode(bits: u128, f: Fmt) -> f64 {
    let sign = if bits >> (f.total() - 1) & 1 == 1 { -1.0 } else { 1.0 };
    let emax = (1u128 << f.eb) - 1;
    let exp = (bits >> f.mfield()) & emax;
    let mant = bits & ((1u128 << f.mfield()) - 1);
    if exp == emax {
        return if mant & ((1u128 << f.mb) - 1) == 0 { sign * f64::INFINITY } else { f64::NAN };
    }
    let bias = (1i32 << (f.eb - 1)) - 1;
    let (sig, e) = match exp {
        0 => (mant, 1 - bias - f.mb as i32),
        _ => (if f.explicit { mant } else { mant | 1 << f.mb }, exp as i32 - bias - f.mb as i32),
    };
    sign * ldexp(sig as f64, e)
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn formats() {
        assert_eq!(show("float_bits(52.345, 32)"), "0x42516148");
        assert_eq!(
            show("[float_bits(1, 16), float_bits(1, \"bf16\"), float_bits(0.1), float_bits(-0.0, 32)]"),
            "[0x3c00, 0x3f80, 0x3fb999999999999a, 0x80000000]"
        );
        assert_eq!(show("float_bits(1, 80)"), "0x3fff8000000000000000");
        assert_eq!(show("float_bits(1, 128)"), "0x3fff0000000000000000000000000000");
        assert_eq!(
            show("[from_float_bits(0x42516148, 32), from_float_bits(float_bits(1.5, 80), 80), from_float_bits(float_bits(-3, 128), 128)]"),
            "[52.345, 1.5, -3]"
        );
        assert_eq!(show("[float_bits(52.345, 16), float_bits(1e10, 16), float_bits(1e-8, 16), float_bits(inf, 32)]"), "[0x528b, 0x7c00, 0x0000, 0x7f800000]");
        assert_eq!(show("float_parts(1.5, 32)"), "{sign: 0, exponent: 127, mantissa: 0x400000}");
        assert_eq!(show("[0.1 to fp(32) == nearest_float(0.1, 32), float_error(0.5, 16)]"), "[true, 0]");
        assert!(try_eval("float_bits(1, 12)").is_err());
        assert!(try_eval("from_float_bits(0x10000, 16)").is_err());
    }
}
