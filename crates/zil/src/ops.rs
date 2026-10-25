//! Arithmetic and comparison on values: exact ints and fractions, floats, and the hooks modules add for their own types.

use crate::ast::{BinOp, Radix};
use crate::modules::{self, units};
use crate::value::{Value, compare, exact, num, ratio};
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

pub fn verb(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "subtract",
        BinOp::PlusMinus => "attach an uncertainty to",
        BinOp::Mul => "multiply",
        BinOp::Div | BinOp::IntDiv => "divide",
        BinOp::Rem => "take the remainder of",
        BinOp::Mod => "reduce",
        BinOp::Pow => "raise",
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => "compare",
        BinOp::Range | BinOp::RangeIncl => "make a range of",
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => "bit-combine",
        _ => "combine",
    }
}

/// `int`, or `length (km)` for a quantity.
fn describe(v: &Value) -> String {
    match v {
        Value::Qty(_, u) => units::describe(u, None),
        _ => v.type_name().to_string(),
    }
}

pub fn mismatch(op: BinOp, a: &Value, b: &Value) -> String {
    format!("cannot {} {} and {}", verb(op), describe(a), describe(b))
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> Result<Value, String> {
    use Value::*;
    let mismatch = || mismatch(op, a, b);
    match op {
        BinOp::Eq => return Ok(Bool(a == b)),
        BinOp::Ne => return Ok(Bool(a != b)),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = compare(a, b).ok_or_else(mismatch)?;
            return Ok(Bool(match op {
                BinOp::Lt => ord.is_lt(),
                BinOp::Le => ord.is_le(),
                BinOp::Gt => ord.is_gt(),
                _ => ord.is_ge(),
            }));
        }
        _ => {}
    }
    if let Some(r) = modules::binary(op, a, b) {
        return r;
    }
    if let (Int(x, bx), Int(y, by)) = (a, b)
        && let Some(r) = int_op(op, *x, *y)?
    {
        return Ok(Int(r, if *bx != Radix::DEC { *bx } else { *by }));
    }
    if let (Some(x), Some(y)) = (ratio(a), ratio(b)) {
        return exact_op(op, a, b, x, y);
    }
    Ok(match (num(a), num(b)) {
        (Some(x), Some(y)) => Float(match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => x / y,
            BinOp::IntDiv => (x / y).floor(),
            BinOp::Rem => x.rem_euclid(y),
            BinOp::Pow => x.powf(y),
            _ => return Err(mismatch()),
        }),
        _ => return Err(mismatch()),
    })
}

/// The i64 fast path; `None` hands over to `exact_op` (overflow, uneven division, negative powers).
fn int_op(op: BinOp, x: i64, y: i64) -> Result<Option<i64>, String> {
    Ok(match op {
        BinOp::Add => x.checked_add(y),
        BinOp::Sub => x.checked_sub(y),
        BinOp::Mul => x.checked_mul(y),
        BinOp::Div | BinOp::IntDiv | BinOp::Rem if y == 0 => return Err("division by zero".into()),
        BinOp::Div if x.checked_rem(y) != Some(0) => None,
        BinOp::Div => x.checked_div(y),
        BinOp::IntDiv => x.checked_div(y).map(|q| if x % y != 0 && (x < 0) != (y < 0) { q - 1 } else { q }),
        BinOp::Rem => x.checked_rem_euclid(y),
        BinOp::Pow => u32::try_from(y).ok().and_then(|y| x.checked_pow(y)),
        BinOp::BitAnd => Some(x & y),
        BinOp::BitOr => Some(x | y),
        BinOp::BitXor => Some(x ^ y),
        // ponytail: `<<` drops high bits like C instead of erroring on overflow.
        BinOp::Shl | BinOp::Shr if !(0..64).contains(&y) => return Err(format!("shift by {y} is out of range\nnote: shifts go from 0 to 63")),
        BinOp::Shl => Some(x << y),
        BinOp::Shr => Some(x >> y),
        _ => None,
    })
}

/// Arithmetic on ints, big ints and fractions with no rounding; the result shrinks back to the smallest type.
fn exact_op(op: BinOp, a: &Value, b: &Value, x: BigRational, y: BigRational) -> Result<Value, String> {
    let radix = |v: &Value| match v {
        Value::Int(_, r) | Value::Big(_, r) if *r != Radix::DEC => Some(*r),
        _ => None,
    };
    let base = radix(a).or(radix(b)).unwrap_or(Radix::DEC);
    let as_frac = matches!(a, Value::Frac(_, true)) || matches!(b, Value::Frac(_, true));
    if matches!(op, BinOp::Div | BinOp::IntDiv | BinOp::Rem) && y.is_zero() {
        return Err("division by zero".into());
    }
    let r = match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div => x / y,
        BinOp::IntDiv => (x / y).floor(),
        BinOp::Rem => {
            let m = y.abs();
            let q = (&x / &m).floor();
            x - m * q
        }
        BinOp::Pow if !y.is_integer() => {
            return Ok(Value::Float(num(a).unwrap_or(f64::NAN).powf(num(b).unwrap_or(f64::NAN))));
        }
        BinOp::Pow => pow(x, y.to_integer())?,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr if x.is_integer() && y.is_integer() => {
            let (p, q) = (x.to_integer(), y.to_integer());
            BigRational::from_integer(match op {
                BinOp::BitAnd => p & q,
                BinOp::BitOr => p | q,
                BinOp::BitXor => p ^ q,
                _ => {
                    let s = q.to_u8().filter(|s| *s < 64).ok_or(format!("shift by {q} is out of range\nnote: shifts go from 0 to 63"))?;
                    if op == BinOp::Shl { p << s } else { p >> s }
                }
            })
        }
        _ => return Err(mismatch(op, a, b)),
    };
    crate::state::check_bits(r.numer().bits().max(r.denom().bits()))?;
    Ok(exact(r, base, as_frac))
}

/// Exact power, refusing results over the session's bit cap.
fn pow(x: BigRational, e: BigInt) -> Result<BigRational, String> {
    if x.is_zero() && e.is_negative() {
        return Err("division by zero".into());
    }
    // 0, 1 and -1 only care whether e is zero, odd or even, however large it is.
    if x.is_zero() || x.abs().is_one() {
        return Ok(x.pow(if e.is_zero() {
            0
        } else if e.is_odd() {
            1
        } else {
            2
        }));
    }
    let bits = x.numer().bits().max(x.denom().bits());
    let hint = |e: String| e + "; a float base like `2.0 ** n` gives an estimate";
    let Some(n) = e.to_i32() else { return Err(hint("result too large\nnote: exact powers take exponents up to 2^31".into())) };
    crate::state::check_bits(bits.saturating_mul(n.unsigned_abs() as u64)).map_err(hint)?;
    Ok(x.pow(n))
}
