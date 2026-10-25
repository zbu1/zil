//! Number bases: integers keep the base they were written or converted in.

use crate::ast::Radix;
use crate::modules::Module;
use crate::modules::text::encoding::hex;
use crate::value::{Value, exact, fits};
use num_bigint::BigInt;
use num_rational::BigRational;

pub const MODULE: Module = Module {
    name: "bases",
    about: "integers in hex, binary, octal or any base from 2 to 36, and their digits",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Integers written in other bases: binary, octal, hex, or any base from 2 to 36."),
        ("writing", "\
Type a number in another base with a prefix: `0b1011` for binary, `0o17` for octal, `0xff` for hex, or `36#zz` for any \
base up to 36. A number remembers the base it was written in, so `0xff + 1` shows as `0x100`."),
        ("good to know", "\
Computers store negative numbers in two's complement, where with a fixed number of bits -1 is all ones. Give that \
number of bits, as in `-1 to hex(16)`, to see a negative number that way."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("bases", &[
            ("255 in binary", "255 to bin"),
            ("a base-36 number in decimal", "36#zz to dec"),
            ("hex of a big int", "factorial(20) to hex"),
            ("results keep the base they were written in", "0xff + 1"),
            ("-1 in 16-bit two's complement", "-1 to hex(16)"),
            ("base 36", "255 to base(36)"),
            ("the sum of the digits of 2 to the 100th", "(2 ** 100).digits.sum"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("conversions", &[
            ("to hex, to bin, to oct or to dec", "255 to bin"),
            ("to hex(bits): negatives in that many bits", "-1 to hex(16)"),
            ("to base(b): any base from 2 to 36", "255 to base(36)"),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("bases", &["hex", "bin", "oct", "dec", "base"]),
        ("digits", &["digits", "from_digits"]),
    ],
    call: fns::call,
    targets: &[("hex", "hex"), ("bin", "bin"), ("oct", "oct"), ("dec", "dec"), ("base", "base")],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{from_digits as join_digits, to_radix};
    use crate::ast::Radix;
    use crate::modules::Fail;
    use crate::signature::{As, Float, Int, List, Or, Or4};
    use crate::value::Value;

    fn number(v: Or<Int, Float>) -> Value {
        match v {
            Or::A(n) => n.0,
            Or::B(x) => Value::Float(x.0),
        }
    }

    /// `v` in `base`, `bits` wide (0 for no width).
    fn radix(v: &Value, base: u32, bits: Option<i64>) -> Result<As<Int>, Fail> {
        let width = match bits {
            None => 0,
            Some(w) if (1..=64).contains(&w) => w as u32,
            Some(w) => return Err(Fail::Arg(1, format!("width {w} is out of range\nnote: widths go from 1 to 64 bits"))),
        };
        Ok(to_radix(v, Radix { base, width }).map_err(|m| Fail::Arg(0, m))?.into())
    }

    /// an integer in hex, the same as `v to hex`
    ///
    /// With bits, a negative number in two's complement. A string or bytes becomes the hex of each byte
    #[builtin(examples = ["hex(255)", "hex(-1, 16)", r#"hex("hi")"#], see = ["bin", "base", "int"])]
    fn hex(v: Or4<Int, Float, &str, &[u8]>, bits: Option<i64>) -> Result<As<Or<Int, String>>, Fail> {
        let v = match v {
            Or4::A(n) => n.0,
            Or4::B(x) => Value::Float(x.0),
            Or4::C(s) => Value::str(s),
            Or4::D(b) => Value::bytes(b),
        };
        Ok(As::from(radix(&v, 16, bits)?.0))
    }

    /// an integer in binary, the same as `v to bin`
    ///
    /// With bits, padded to that many bits and negatives in two's complement
    #[builtin(examples = ["bin(10)", "bin(5, 8)"], see = ["hex", "oct"])]
    fn bin(v: Or<Int, Float>, bits: Option<i64>) -> Result<As<Int>, Fail> {
        radix(&number(v), 2, bits)
    }

    /// an integer in octal, the same as `v to oct`
    #[builtin(examples = ["oct(8)"], see = ["hex", "bin"])]
    fn oct(v: Or<Int, Float>, bits: Option<i64>) -> Result<As<Int>, Fail> {
        radix(&number(v), 8, bits)
    }

    /// an integer in decimal, the same as `v to dec`
    ///
    /// With bits, a negative number becomes the positive one with the same bits, so -1 in 8 bits is 255
    #[builtin(examples = ["dec(0xff)", "dec(-1, 8)"], see = ["hex", "int"])]
    fn dec(v: Or<Int, Float>, bits: Option<i64>) -> Result<As<Int>, Fail> {
        radix(&number(v), 10, bits)
    }

    /// an integer in any base from 2 to 36, the same as `v to base(b)`
    ///
    /// Digits past 9 are letters
    #[builtin(examples = ["base(35, 36)", "base(10, 3)"], see = ["hex", "digits"])]
    fn base(v: Or<Int, Float>, b: i64) -> Result<As<Int>, Fail> {
        if !(2..=36).contains(&b) {
            return Err(Fail::Arg(1, format!("base {b} is out of range\nnote: bases go from 2 to 36")));
        }
        radix(&number(v), b as u32, None)
    }

    /// the list of a number's digits, in the base it was written in
    #[builtin(examples = ["1234.digits", "0b1011.digits"], see = ["from_digits"])]
    fn digits(n: Int) -> Vec<Value> {
        match &*n {
            Value::Int(n, r) => {
                let (mut m, b) = (n.unsigned_abs(), r.base as u64);
                let mut d = vec![Value::int((m % b) as i64)];
                while m >= b {
                    m /= b;
                    d.push(Value::int((m % b) as i64));
                }
                d.reverse();
                d
            }
            Value::Big(n, r) => n.magnitude().to_radix_be(r.base).into_iter().map(|d| Value::int(d as i64)).collect(),
            _ => unreachable!("an Int holds an int"),
        }
    }

    /// put digits back together into a number, the reverse of digits
    ///
    /// The result shows in the base the digits are in
    #[builtin(examples = ["[1, 2, 3].from_digits", "[1, 0, 1, 1].from_digits(2)"], see = ["digits"])]
    fn from_digits(xs: &List, #[default(10)] base: i64) -> Result<As<Int>, Fail> {
        if !(2..=36).contains(&base) {
            return Err(Fail::BadArgs);
        }
        Ok(join_digits(&xs.borrow(), base as u32)?.into())
    }
}

/// An integer displayed in another base or bit width; strings to hex become their bytes.
fn to_radix(v: &Value, r: Radix) -> Result<Value, String> {
    Ok(match v {
        Value::Int(n, _) if r.width == 0 || fits(*n, r.width) => Value::Int(*n, r),
        Value::Int(n, _) => return Err(format!("{n} does not fit in {} bits", r.width)),
        Value::Big(n, _) if r.width == 0 => Value::Big(n.clone(), r),
        Value::Big(n, _) => return Err(format!("{n} does not fit in {} bits", r.width)),
        Value::Float(x) if x.fract() == 0.0 && x.abs() < 9.2e18 => to_radix(&Value::int(*x as i64), r)?,
        Value::Str(s) if r == (Radix { base: 16, width: 0 }) => Value::str(hex(s.as_bytes())),
        Value::Bytes(b) if r == (Radix { base: 16, width: 0 }) => Value::str(hex(b)),
        Value::Float(x) => return Err(format!("`{x}` is not a whole number")),
        v => return Err(format!("expected an integer, got {}", v.type_name())),
    })
}

fn from_digits(l: &[Value], base: u32) -> Result<Value, String> {
    let mut acc = BigInt::from(0);
    for v in l {
        let d = match v {
            Value::Int(d, _) if (0..base as i64).contains(d) => *d,
            _ => return Err(format!("`{v:?}` is not a base-{base} digit\nnote: base-{base} digits go from 0 to {}", base - 1)),
        };
        acc = acc * base + d;
    }
    Ok(exact(BigRational::from_integer(acc), Radix { base, width: 0 }, false))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn digits() {
        assert_eq!(show("1234.digits"), "[1, 2, 3, 4]");
        assert_eq!(show("0b110.digits"), "[1, 1, 0]");
        assert_eq!(show("0.digits"), "[0]");
        assert_eq!(show("[1, 1, 0].from_digits(2)"), "0b110");
        assert_eq!(show("0xff.digits.from_digits(16)"), "0xff");
        assert!(try_eval("[2].from_digits(2)").is_err());
    }

    #[test]
    fn bases() {
        assert_eq!(show("hex(255)"), "0xff");
        assert_eq!(show("255.bin"), "0b11111111");
        assert_eq!(show("bin(5, 8)"), "0b00000101");
        assert_eq!(show("oct(8)"), "0o10");
        assert_eq!(show("dec(0xff)"), "255");
        assert_eq!(show("base(35, 36)"), "36#z");
        assert_eq!(show(r#"hex("hi")"#), "6869");
        assert_eq!(show("hex = 3\nhex + 1"), "4");
        assert!(try_eval("hex(256, 8)").is_err());
        assert!(try_eval("base(1, 99)").is_err());
        assert_eq!(show("255 to hex"), "0xff");
        assert_eq!(show("x = 255 to hex\nx + 1"), "0x100");
        assert_eq!(show("0xff to dec"), "255");
        assert_eq!(show("0b1010 + 1"), "0b1011");
        assert_eq!(show("10 to bin"), "0b1010");
        assert_eq!(show("35 to base(36)"), "36#z");
        assert_eq!(show("x = 36#z\nx + 1"), "36#10");
        assert_eq!(show("x = 0b1010\nx + 1"), "0b1011");
        assert_eq!(show("2#1010"), "0b1010");
        assert_eq!(show("-3#12 to dec"), "-5");
        assert!(try_eval("37#1").is_err());
        assert!(try_eval("2#12").is_err());
        assert_eq!(show("-1 to hex(32)"), "0xffffffff");
        assert_eq!(show("5 to bin(8)"), "0b00000101");
        assert_eq!(show("-1 to dec(8)"), "255");
        assert_eq!(show("~0x0f to hex(8)"), "0xf0");
        assert_eq!(show("-5 to hex"), "-0x5");
        assert!(try_eval("256 to hex(8)").is_err());
        assert!(try_eval("5 to base(99)").is_err());
        assert!(try_eval("5 to hex(65)").is_err());
        assert_eq!(show(r#""hi" to hex"#), "6869");
    }
}
