//! Number bases: integers keep the base they were written or converted in.

use crate::ast::Radix;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::text::encoding::hex;
use crate::modules::{Call, Doc, Fail, Module, doc};
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
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("bases", &["hex", "bin", "oct", "dec", "base"]),
        ("digits", &["digits", "from_digits"]),
    ],
    call,
    targets: &[("hex", "hex"), ("bin", "bin"), ("oct", "oct"), ("dec", "dec"), ("base", "base")],
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("hex", "hex(v: int|float|str|bytes, bits?: int) -> int|str", "an integer in hex, the same as `v to hex`", &["hex(255)", "hex(-1, 16)", r#"hex("hi")"#], &["bin", "base", "int"])
        .detail("With bits, a negative number in two's complement. A string or bytes becomes the hex of each byte"),
    doc("bin", "bin(v: int|float, bits?: int) -> int", "an integer in binary, the same as `v to bin`", &["bin(10)", "bin(5, 8)"], &["hex", "oct"])
        .detail("With bits, padded to that many bits and negatives in two's complement"),
    doc("oct", "oct(v: int|float, bits?: int) -> int", "an integer in octal, the same as `v to oct`", &["oct(8)"], &["hex", "bin"]),
    doc("dec", "dec(v: int|float, bits?: int) -> int", "an integer in decimal, the same as `v to dec`", &["dec(0xff)", "dec(-1, 8)"], &["hex", "int"])
        .detail("With bits, a negative number becomes the positive one with the same bits, so -1 in 8 bits is 255"),
    doc("base", "base(v: int|float, b: int) -> int", "an integer in any base from 2 to 36, the same as `v to base(b)`", &["base(35, 36)", "base(10, 3)"], &["hex", "digits"])
        .detail("Digits past 9 are letters"),
    doc("digits", "digits(n: int) -> list", "the list of a number's digits, in the base it was written in", &["1234.digits", "0b1011.digits"], &["from_digits"]),
    doc("from_digits", "from_digits(xs: list, base?: int) -> int", "put digits back together into a number, the reverse of digits", &["[1, 2, 3].from_digits", "[1, 0, 1, 1].from_digits(2)"], &["digits"])
        .detail("Base 10 unless a base is given, and the result shows in that base"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("hex" | "bin" | "oct" | "dec" | "base", [v, rest @ ..]) if rest.len() <= 1 => {
            let base = match name {
                "hex" => 16,
                "bin" => 2,
                "oct" => 8,
                "dec" => 10,
                _ => match rest {
                    [Int(b, _)] if (2..=36).contains(b) => *b as u32,
                    [Int(b, _)] => return Err(Fail::Arg(1, format!("base {b} is out of range\nnote: bases go from 2 to 36"))),
                    _ => return Err(Fail::BadArgs),
                },
            };
            let width = match (name, rest) {
                ("base", _) | (_, []) => 0,
                (_, [Int(w, _)]) if (1..=64).contains(w) => *w as u32,
                (_, [Int(w, _)]) => return Err(Fail::Arg(1, format!("width {w} is out of range\nnote: widths go from 1 to 64 bits"))),
                _ => return Err(Fail::BadArgs),
            };
            to_radix(v, Radix { base, width }).map_err(|m| Fail::Arg(0, m))?
        }
        ("digits", [Int(n, r)]) => {
            let (mut m, b) = (n.unsigned_abs(), r.base as u64);
            let mut d = vec![Value::int((m % b) as i64)];
            while m >= b {
                m /= b;
                d.push(Value::int((m % b) as i64));
            }
            d.reverse();
            Value::list(d)
        }
        ("digits", [Big(n, r)]) => Value::list(n.magnitude().to_radix_be(r.base).into_iter().map(|d| Value::int(d as i64)).collect()),
        ("from_digits", [List(l)]) => from_digits(&l.borrow(), 10)?,
        ("from_digits", [List(l), Int(b, _)]) if (2..=36).contains(b) => from_digits(&l.borrow(), *b as u32)?,
        _ => return Err(Fail::BadArgs),
    })
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
