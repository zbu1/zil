//! Number theory on exact integers: gcd, primes, factoring, combinatorics.

use super::{big, int};
use crate::ast::{BinOp, Radix};
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Claim, Doc, Fail, Module, doc};
use crate::ops::mismatch;
use crate::value::{Value, exact, num, ratio};
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

pub const MODULE: Module = Module {
    name: "numtheory",
    about: "whole-number math: divisors, primes, factorials, counting arrangements, and clock (modular) arithmetic",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Math on whole numbers: divisors, primes, factorials, counting, and modular arithmetic. Everything works on integers of \
any size and gives exact answers, so `factorial(100)` has all 158 digits."),
        ("writing", "\
`10 mod 12` makes a value that wraps around at 12, like hours on a clock, where 5 hours after 10 o'clock is 3 o'clock. \
It keeps wrapping through `+`, `-`, `*`, `/` and `**`, and shows as `10 (mod 12)`."),
        ("good to know", "\
Two mod values are equal when they land on the same spot, which is called being congruent and written ≡. Dividing \
works whenever the divisor shares no factor with the modulus. The `%` operator is different, and just gives a plain \
remainder."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("numtheory", &[
            ("prime factors", "factors(2 ** 32 + 1)"),
            ("is 2 to the 61st minus 1 prime?", "is_prime(2 ** 61 - 1)"),
            ("how many 5-card poker hands there are", "choose(52, 5)"),
            ("podium finishes from 8 runners", "permutations(8, 3)"),
            ("every way to pick 2 of them", "[1, 2, 3].combinations(2)"),
            ("the remainder of 3 to the 1000th, divided by 7", "mod_pow(3, 1000, 7)"),
            ("5 hours after 10 o'clock", "10 mod 12 + 5"),
            ("what times 3 gives 1, wrapping at 7?", "1 / (3 mod 7)"),
            ("x with x % 3 == 2 and x % 5 == 3", "crt([2, 3], [3, 5])"),
            ("the 1000th Fibonacci number has 209 digits", "fib(1000).str.len"),
            ("the start of pi's continued fraction", "pi.cfrac(4)"),
            ("the next prime after a googol", "(10 ** 100).next_prime - 10 ** 100"),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("types", &[("mod", "5 mod 7")]),
        ("operators", &[
            ("+ - * / with a mod or an int wrap around", "(5 mod 7) * 3"),
            ("** with any integer power, and -1 gives the inverse", "(3 mod 7) ** -1"),
            ("== asks whether they land on the same spot", "(2 mod 7) ** 3 == 1"),
        ]),
        ("syntax", &[
            ("n! is the factorial, touching the number", "5!"),
            ("n!! is the double factorial", "7!!"),
        ]),
    ],
    fns: FNS,
    call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("gcd", "gcd(a: int, b: int, ...) -> int / gcd(xs: list) -> int", "greatest common divisor: the largest integer that divides all of them evenly", &["gcd(12, 18)", "[12, 18, 27].gcd"], &["lcm"]),
    doc("lcm", "lcm(a: int, b: int, ...) -> int / lcm(xs: list) -> int", "least common multiple: the smallest positive integer they all divide evenly", &["lcm(4, 6)", "(1..=20).lcm"], &["gcd"]),
    doc("is_prime", "is_prime(n: int) -> bool", "whether n is prime", &["is_prime(97)", "is_prime(2 ** 61 - 1)"], &["factors"])
        .detail("Certain for n below about 3 × 10²⁴. Above that, a wrong answer is possible but astronomically unlikely (Miller-Rabin test)"),
    doc("factors", "factors(n: int) -> list", "the primes that multiply to give n, smallest first, with repeats", &["360.factors", "factors(2 ** 32 + 1)"], &["is_prime", "gcd"]),
    doc("factorial", "factorial(n: int) -> int", "n! = 1 × 2 × ... × n, exact", &["factorial(5)", "30!"], &["choose", "double_factorial"])
        .detail("Also written `n!`"),
    doc("double_factorial", "double_factorial(n: int) -> int", "n!! = n × (n - 2) × (n - 4) × ... down to 1 or 2, exact", &["double_factorial(7)", "8!!"], &["factorial"])
        .detail("Also written `n!!`"),
    doc("hyperfactorial", "hyperfactorial(n: int) -> int", "1¹ × 2² × ... × nⁿ, exact", &["hyperfactorial(4)"], &["superfactorial", "factorial"]),
    doc("superfactorial", "superfactorial(n: int) -> int", "1! × 2! × ... × n!, exact", &["superfactorial(4)"], &["hyperfactorial", "factorial"]),
    doc("derangements", "derangements(n: int) -> int", "how many ways n items can be shuffled so that none ends up in its original place, exact", &["derangements(4)"], &["permutations", "factorial"]),
    doc("choose", "choose(n: num, k: int) -> int|frac", "how many ways there are to pick k things from n when order doesn't matter (the binomial coefficient), exact", &["choose(5, 2)", "choose(52, 5)", "choose(1/2, 2)", "choose(-1, 3)"], &["permutations", "combinations", "factorial"])
        .detail("n can also be a fraction or negative, for the general formula used in series expansions"),
    doc("nCr", "nCr(n: num, k: int) -> int|frac", "the same as choose, under the name calculators use", &["nCr(5, 2)"], &["choose"]),
    doc("permutations", "permutations(n: int, k: int) -> int / permutations(xs: list, k?: int) -> list", "how many ways there are to pick k things from n when order matters, exact",
        &["permutations(5, 2)", "[1, 2, 3].permutations", r#""abc".chars.permutations(2).len"#], &["choose", "combinations"])
        .detail("Given a list instead, every ordering of k of its items (all of them by default)"),
    doc("perm", "perm(n: int, k: int) -> int / perm(xs: list, k?: int) -> list", "the same as permutations", &["perm(5, 2)"], &["permutations"]),
    doc("nPr", "nPr(n: int, k: int) -> int / nPr(xs: list, k?: int) -> list", "the same as permutations, under the name calculators use", &["nPr(5, 2)"], &["permutations"]),
    doc("combinations", "combinations(xs: list, k: int) -> list", "every way to pick k of the list's items when order doesn't matter, each as a list in the original order", &["[1, 2, 3, 4].combinations(2)"], &["choose", "permutations"]),
    doc("mod_pow", "mod_pow(b: int, e: int, m: int) -> int", "the remainder of b to the power e, divided by m, worked out without ever computing the huge power", &["mod_pow(2, 100, 7)", "mod_pow(3, 10 ** 18, 1000000007)", "mod_pow(3, -1, 7)"], &["mod_inv", "gcd"])
        .detail("A negative e uses b's inverse, as in mod_inv"),
    doc("mod_inv", "mod_inv(a: int, m: int) -> int / mod_inv(x: mod) -> mod", "the number b from 0 to m - 1 for which a × b leaves remainder 1 when divided by m", &["mod_inv(3, 7)", "mod_inv(5 mod 12)"], &["mod_pow", "crt"])
        .detail("An error if a and m share a factor, since then there isn't one"),
    doc("crt", "crt(rs: list, ms: list) -> mod / crt(xs: list) -> mod", "the number that leaves remainder rs[0] when divided by ms[0], rs[1] when divided by ms[1], and so on (the Chinese remainder theorem)",
        &["crt([2, 3, 2], [3, 5, 7])", "crt([2 mod 3, 3 mod 5])"], &["mod_inv"])
        .detail("The answer is a mod, since adding the moduli's least common multiple gives another. An error if there's none"),
    doc("residue", "residue(x: mod) -> int", "the plain integer a mod value is at, from 0 to its modulus - 1", &["(17 mod 5).residue"], &["modulus"]),
    doc("modulus", "modulus(x: mod) -> int", "the number a mod value wraps around at", &["(17 mod 5).modulus"], &["residue"]),
    doc("rem", "rem(a: num, b: num) -> int|float", "the remainder of a divided by b, with the same sign as a, as C's and JavaScript's `%` give it", &["rem(-7, 3)", "-7 % 3"], &["mod_pow"])
        .detail("zil's own `%` is never negative"),
    doc("fib", "fib(n: int) -> int", "the nth Fibonacci number, exact", &["fib(10)", "fib(100)"], &["factorial"])
        .detail("0, 1, 1, 2, 3, 5, 8, ..., each the sum of the two before, starting from fib(0) = 0"),
    doc("next_prime", "next_prime(n: int) -> int", "the smallest prime greater than n", &["next_prime(100)", "next_prime(2 ** 64)"], &["is_prime"]),
    doc("totient", "totient(n: int) -> int", "Euler's totient φ(n)", &["totient(36)", "totient(97)"], &["factors", "gcd"])
        .detail("How many of the numbers from 1 to n share no factor with n"),
    doc("isqrt", "isqrt(n: int) -> int", "the integer part of the square root of n, exact at any size", &["isqrt(99)", "isqrt(10 ** 40)"], &["sqrt"]),
    doc("cfrac", "cfrac(x: num, terms?: int) -> list", "x as a continued fraction, the list [a0, a1, a2, ...] meaning a0 + 1/(a1 + 1/(a2 + ...))", &["(415/93).cfrac", "pi.cfrac(5)", "sqrt(2).cfrac(6)"], &["frac"])
        .detail("Cutting it short gives the best fractions close to x. Exact for fractions, while floats give up to terms numbers (20 by default)"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("gcd" | "lcm", [List(l)]) => gcd_lcm(name, &l.borrow())?,
        ("gcd" | "lcm", vs) if vs.len() >= 2 => gcd_lcm(name, vs)?,
        ("is_prime", [v]) => Bool(is_prime(&int(v)?)),
        ("factors", [v]) => {
            let mut n = int(v)?;
            if !n.is_positive() {
                return Err(Fail::Arg(0, format!("expected a positive integer, got {n}")));
            }
            let mut out = Vec::new();
            for p in [2u32, 3, 5] {
                while (&n % p).is_zero() {
                    n /= p;
                    out.push(BigInt::from(p));
                }
            }
            factor(n, &mut out);
            out.sort();
            Value::list(out.into_iter().map(big).collect())
        }
        ("factorial" | "double_factorial" | "hyperfactorial" | "superfactorial" | "derangements", [Int(n, _)]) => {
            if !(0..=20_000).contains(n) {
                return Err(Fail::Arg(0, format!("n must be from 0 to 20000, got {n}")));
            }
            let n = *n;
            let bits = (n as f64 * (n as f64).max(1.0).log2()) as u64;
            crate::state::check_bits(if matches!(name, "hyperfactorial" | "superfactorial") { bits * n as u64 / 2 } else { bits })?;
            big(match name {
                "factorial" => (1..=n).map(BigInt::from).product(),
                "double_factorial" => (1..=n).rev().step_by(2).map(BigInt::from).product(),
                "hyperfactorial" => (1..=n).map(|i| BigInt::from(i).pow(i as u32)).product(),
                "superfactorial" => (1..=n)
                    .scan(BigInt::one(), |f, i| {
                        *f *= i;
                        Some(f.clone())
                    })
                    .product(),
                // !n = n·!(n-1) + (-1)^n
                _ => (1..=n).fold(BigInt::one(), |d, i| d * i + if i % 2 == 0 { 1 } else { -1 }),
            })
        }
        ("permutations" | "perm" | "nPr", [Int(n, _), Int(k, _)]) if *n >= 0 && *k >= 0 => {
            if k > n {
                return Ok(Value::int(0));
            }
            crate::state::check_bits((*k as f64 * (*n as f64).log2()) as u64)?;
            big((n - k..*n).map(|i| BigInt::from(i + 1)).product())
        }
        ("permutations" | "perm" | "nPr", [List(l), rest @ ..]) if rest.len() <= 1 => {
            let xs = l.borrow().clone();
            let k = match rest {
                [] => xs.len(),
                [Int(k, _)] if *k >= 0 => *k as usize,
                _ => return Err(Fail::Arg(1, "k must be an int, 0 or more".into())),
            };
            Value::list(arrangements(&xs, k, true)?)
        }
        ("combinations", [List(l), Int(k, _)]) if *k >= 0 => Value::list(arrangements(&l.borrow(), *k as usize, false)?),
        // Generalized: n(n-1)...(n-k+1) / k!, exact for fractions and negative ints.
        ("choose" | "nCr", [n, Int(k, _)]) if *k >= 0 && !matches!(n, Int(m, _) if *m >= 0) => {
            if *k > 10_000 {
                return Err("result too large".into());
            }
            if let Some(r) = ratio(n) {
                let mut acc = BigRational::one();
                for i in 0..*k {
                    acc = acc * (&r - BigRational::from_integer(i.into())) / BigRational::from_integer((i + 1).into());
                }
                exact(acc, Radix::DEC, true)
            } else {
                let x = num(n).ok_or(Fail::BadArgs)?;
                Value::Float((0..*k).fold(1.0, |acc, i| acc * (x - i as f64) / (i + 1) as f64))
            }
        }
        ("choose" | "nCr", [Int(n, _), Int(k, _)]) if *n >= 0 && *k >= 0 => {
            if k > n {
                return Ok(Value::int(0));
            }
            let k = *k.min(&(n - k));
            if k > 100_000 {
                return Err("result too large".into());
            }
            let mut r = BigInt::one();
            for i in 0..k {
                r = r * (n - i) / (i + 1);
            }
            big(r)
        }
        ("mod_pow", [b, e, m]) => {
            let (b, e, m) = (int(b)?, int(e)?, int(m)?);
            if !m.is_positive() {
                return Err(Fail::Arg(2, format!("the modulus must be positive, got {m}")));
            }
            big(pow(&b, &e, &m)?)
        }
        ("mod_inv", [Mod(x)]) => Value::modular(inverse(&x.0, &x.1)?, x.1.clone()),
        ("mod_inv", [a, m]) => {
            let (a, m) = (int(a)?, int(m)?);
            if !m.is_positive() {
                return Err(Fail::Arg(1, format!("the modulus must be positive, got {m}")));
            }
            big(inverse(&a, &m)?.mod_floor(&m))
        }
        ("crt", [List(xs)]) => {
            let pairs = xs
                .borrow()
                .iter()
                .map(|x| if let Mod(x) = x { Ok((**x).clone()) } else { Err(Fail::Arg(0, format!("expected a list of mods, got a {}", x.type_name()))) })
                .collect::<Result<Vec<_>, _>>()?;
            crt(pairs)?
        }
        ("crt", [List(rs), List(ms)]) => {
            let (rs, ms) = (rs.borrow(), ms.borrow());
            if rs.len() != ms.len() {
                return Err(format!("{} remainders for {} moduli", rs.len(), ms.len()).into());
            }
            let mut pairs = vec![];
            for (r, m) in rs.iter().zip(ms.iter()) {
                let m = int(m).map_err(|_| Fail::Arg(1, "moduli must be ints".into()))?;
                if !m.is_positive() {
                    return Err(Fail::Arg(1, format!("moduli must be positive, got {m}")));
                }
                pairs.push((int(r).map_err(|_| Fail::Arg(0, "remainders must be ints".into()))?, m));
            }
            crt(pairs)?
        }
        ("residue", [Mod(x)]) => big(x.0.clone()),
        ("modulus", [Mod(x)]) => big(x.1.clone()),
        ("rem", [a @ (Int(..) | Big(..)), b @ (Int(..) | Big(..))]) => {
            let (a, b) = (int(a)?, int(b)?);
            if b.is_zero() {
                return Err("division by zero".into());
            }
            big(a % b)
        }
        ("rem", [a, b]) if num(a).is_some() && num(b).is_some() => Float(num(a).unwrap() % num(b).unwrap()),
        ("fib", [Int(n, _)]) if (0..=100_000).contains(n) => {
            let (mut a, mut b) = (BigInt::zero(), BigInt::one());
            for _ in 0..*n {
                (a, b) = (b.clone(), a + b);
            }
            big(a)
        }
        ("fib", [Int(n, _)]) => return Err(Fail::Arg(0, format!("n must be from 0 to 100000, got {n}"))),
        ("next_prime", [v]) => {
            let mut n = int(v)?.max(BigInt::one()) + 1u32;
            while !is_prime(&n) {
                n += 1u32;
            }
            big(n)
        }
        ("totient", [v]) => {
            let n = int(v)?;
            if !n.is_positive() {
                return Err(Fail::Arg(0, format!("expected a positive integer, got {n}")));
            }
            let mut ps = vec![];
            let mut m = n.clone();
            for p in [2u32, 3, 5] {
                while (&m % p).is_zero() {
                    m /= p;
                    ps.push(BigInt::from(p));
                }
            }
            factor(m, &mut ps);
            ps.sort();
            ps.dedup();
            big(ps.iter().fold(n, |acc, p| acc / p * (p - 1u32)))
        }
        ("isqrt", [v]) => {
            let n = int(v)?;
            if n.is_negative() {
                return Err(Fail::Arg(0, format!("expected 0 or more, got {n}")));
            }
            big(n.sqrt())
        }
        ("cfrac", [v, rest @ ..]) if rest.len() <= 1 => {
            let terms = match rest {
                [] => 20,
                [Int(t, _)] if (1..=1000).contains(t) => *t as usize,
                _ => return Err(Fail::Arg(1, "terms must be an int from 1 to 1000".into())),
            };
            let mut out = vec![];
            if let Some(r) = ratio(v) {
                let (mut p, mut q) = (r.numer().clone(), r.denom().clone());
                while !q.is_zero() && out.len() < terms {
                    let (a, rem) = p.div_mod_floor(&q);
                    out.push(big(a));
                    (p, q) = (q, rem);
                }
            } else {
                let mut x = num(v).ok_or(Fail::BadArgs)?;
                for _ in 0..terms {
                    let a = x.floor();
                    out.push(Value::int(a as i64));
                    // ponytail: stop once the float's error swamps the remainder; exact input avoids this.
                    if (x - a).abs() < 1e-9 {
                        break;
                    }
                    x = 1.0 / (x - a);
                }
            }
            Value::list(out)
        }
        _ => return Err(Fail::BadArgs),
    })
}

/// Every selection of `k` of `xs`: in each order if `ordered`, else once each, in list order.
fn arrangements(xs: &[Value], k: usize, ordered: bool) -> Result<Vec<Value>, Fail> {
    fn go(xs: &[Value], k: usize, ordered: bool, used: &mut Vec<usize>, out: &mut Vec<Value>) -> Result<(), Fail> {
        if used.len() == k {
            crate::state::check_len(out.len() + 1, "items")?;
            out.push(Value::list(used.iter().map(|&i| xs[i].clone()).collect()));
            return Ok(());
        }
        let from = if ordered { 0 } else { used.last().map_or(0, |&i| i + 1) };
        for i in from..xs.len() {
            if !used.contains(&i) {
                used.push(i);
                go(xs, k, ordered, used, out)?;
                used.pop();
            }
        }
        Ok(())
    }
    let mut out = vec![];
    if k <= xs.len() {
        go(xs, k, ordered, &mut vec![], &mut out)?;
    }
    Ok(out)
}

/// `5 mod 7`, and arithmetic once either side is a mod: a plain int joins in at that modulus.
fn binary(op: BinOp, a: &Value, b: &Value) -> Claim {
    use Value::Mod;
    let whole = |v: &Value| int(v).ok();
    if op == BinOp::Mod {
        return Some(match (whole(a), whole(b)) {
            (Some(_), Some(m)) if !m.is_positive() => Err(format!("the modulus must be positive, got {m}")),
            (Some(r), Some(m)) => Ok(Value::modular(r, m)),
            _ => Err(mismatch(op, a, b) + "\nhelp: `a mod m` takes two ints"),
        });
    }
    let m = match (a, b) {
        (Mod(x), Mod(y)) if x.1 != y.1 => return Some(Err(format!("cannot {} mods with different moduli, {} and {}", crate::ops::verb(op), x.1, y.1))),
        (Mod(x), _) | (_, Mod(x)) => x.1.clone(),
        _ => return None,
    };
    let residue = |v: &Value| if let Mod(x) = v { Some(x.0.clone()) } else { whole(v) };
    let (Some(x), Some(y)) = (residue(a), residue(b)) else { return Some(Err(mismatch(op, a, b))) };
    let r = match op {
        BinOp::Add => Ok(x + y),
        BinOp::Sub => Ok(x - y),
        BinOp::Mul => Ok(x * y),
        BinOp::Div => inverse(&y, &m).map(|inv| x * inv),
        BinOp::Pow if !matches!(b, Mod(_)) => pow(&x, &y, &m),
        BinOp::Pow => Err("the exponent must be an int, not a mod".into()),
        _ => return Some(Err(mismatch(op, a, b))),
    };
    Some(r.map(|r| Value::modular(r, m)))
}

/// The b with `a * b ≡ 1 (mod m)`, not yet reduced.
fn inverse(a: &BigInt, m: &BigInt) -> Result<BigInt, String> {
    let g = a.extended_gcd(m);
    if !g.gcd.is_one() {
        return Err(format!("{a} has no inverse mod {m}: both are divisible by {}", g.gcd));
    }
    Ok(g.x)
}

/// `b ** e mod m`; a negative `e` raises the inverse.
fn pow(b: &BigInt, e: &BigInt, m: &BigInt) -> Result<BigInt, String> {
    if e.is_negative() { Ok(inverse(b, m)?.modpow(&-e, m)) } else { Ok(b.modpow(e, m)) }
}

/// One `x mod lcm` meeting every `x ≡ r (mod m)`, merging two at a time.
fn crt(pairs: Vec<(BigInt, BigInt)>) -> Call {
    let (mut x, mut l) = (BigInt::zero(), BigInt::one());
    for (r, m) in pairs {
        let g = l.gcd(&m);
        let diff = &r - &x;
        if !(&diff % &g).is_zero() {
            return Err(format!("no solution: x ≡ {x} (mod {l}) and x ≡ {} (mod {m}) disagree", r.mod_floor(&m)).into());
        }
        let n = &m / &g;
        let k = (diff / &g * inverse(&(&l / &g), &n)?).mod_floor(&n);
        x += &l * k;
        l = l.lcm(&m);
        x = x.mod_floor(&l);
    }
    Ok(Value::modular(x, l))
}

fn gcd_lcm(name: &str, vs: &[Value]) -> Call {
    let mut acc = int(vs.first().ok_or("empty list")?)?;
    for v in &vs[1..] {
        acc = if name == "gcd" { acc.gcd(&int(v)?) } else { acc.lcm(&int(v)?) };
    }
    Ok(big(acc))
}

const WITNESSES: [u32; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

/// Miller-Rabin with the first 12 primes as witnesses, which is exact below 3.3e24.
// ponytail: probabilistic above 3.3e24 (fixed witnesses); add random witnesses if huge primes ever matter.
fn is_prime(n: &BigInt) -> bool {
    if *n < BigInt::from(2) {
        return false;
    }
    for p in WITNESSES {
        if (n % p).is_zero() {
            return *n == BigInt::from(p);
        }
    }
    let n1: BigInt = n - 1u32;
    let s = n1.trailing_zeros().unwrap();
    let d = &n1 >> s;
    'next: for a in WITNESSES {
        let mut x = BigInt::from(a).modpow(&d, n);
        if x.is_one() || x == n1 {
            continue;
        }
        for _ in 1..s {
            x = &x * &x % n;
            if x == n1 {
                continue 'next;
            }
        }
        return false;
    }
    true
}

/// Prime factors of an odd `n`, by Pollard's rho.
// ponytail: rho takes ~sqrt(smallest factor) steps, so a product of two 20+ digit primes hangs; ECM if that matters.
fn factor(n: BigInt, out: &mut Vec<BigInt>) {
    if n.is_one() {
        return;
    }
    if is_prime(&n) {
        out.push(n);
        return;
    }
    for c in 1u32.. {
        let f = |x: &BigInt| (x * x + c) % &n;
        let (mut x, mut y, mut d) = (BigInt::from(2), BigInt::from(2), BigInt::one());
        while d.is_one() {
            x = f(&x);
            y = f(&f(&y));
            d = (&x - &y).abs().gcd(&n);
        }
        if d != n {
            let rest = &n / &d;
            factor(d, out);
            factor(rest, out);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn modular() {
        assert_eq!(
            show("[5 mod 7 + 4, 10 mod 12 + 5, -1 mod 5, -(2 mod 5), (3 mod 7) * 5, 2 - (3 mod 7)]"),
            "[2 (mod 7), 3 (mod 12), 4 (mod 5), 3 (mod 5), 1 (mod 7), 6 (mod 7)]"
        );
        assert_eq!(
            show("[1 / (3 mod 7), (3 mod 7) ** -1, (2 mod 7) ** 100, (2 mod 7) ** 3 == 1, (2 mod 7) == 9, 2 mod 7 == 2 mod 5]"),
            "[5 (mod 7), 5 (mod 7), 2 (mod 7), true, true, false]"
        );
        assert_eq!(
            show("[mod_inv(3, 7), mod_inv(5 mod 12), mod_pow(3, -1, 7), (17 mod 5).residue, (17 mod 5).modulus, type(1 mod 2)]"),
            r#"[5, 5 (mod 12), 5, 2, 5, "mod"]"#
        );
        assert_eq!(
            show("[crt([2, 3, 2], [3, 5, 7]), crt([2 mod 3, 3 mod 5]), crt([1, 3], [4, 6]), crt([], [])]"),
            "[23 (mod 105), 8 (mod 15), 9 (mod 12), 0 (mod 1)]"
        );
        assert_eq!(show("[rem(-7, 3), -7 % 3, rem(7, -3), rem(-7.5, 2)]"), "[-1, 2, 1, -1.5]");
        assert_eq!(show("(2 ** 127 - 1) mod 2 ** 64"), "18446744073709551615 (mod 18446744073709551616)");
        for bad in [
            "1 / (2 mod 4)",
            "(1 mod 5) + (1 mod 7)",
            "5 mod 0",
            "5.5 mod 2",
            "(2 mod 5) ** (2 mod 5)",
            "(1 mod 5) < 2",
            "mod_inv(2, 4)",
            "crt([1, 2], [4, 6])",
            "rem(1, 0)",
        ] {
            assert!(try_eval(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn number_theory() {
        assert_eq!(show("[gcd(12, 18), [12, 18, 27].gcd, lcm(4, 6), (1..=20).lcm, gcd(0, 5)]"), "[6, 3, 12, 232792560, 5]");
        assert_eq!(show("(1..30).filter(is_prime)"), "[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]");
        assert_eq!(
            show("[is_prime(2 ** 61 - 1), is_prime(2 ** 61 + 1), is_prime(3215031751), is_prime(1), is_prime(-7)]"),
            "[true, false, false, false, false]"
        );
        assert_eq!(show("[360.factors, 1.factors, 97.factors, factors(2 ** 32 + 1)]"), "[[2, 2, 2, 3, 3, 5], [], [97], [641, 6700417]]");
        assert_eq!(show("factors(600851475143)"), "[71, 839, 1471, 6857]");
        assert_eq!(show("[factorial(0), factorial(20), factorial(25)]"), "[1, 2432902008176640000, 15511210043330985984000000]");
        assert_eq!(show("[choose(5, 2), choose(52, 5), choose(3, 5), choose(100, 50)]"), "[10, 2598960, 0, 100891344545564193334812497256]");
        assert_eq!(show("[mod_pow(2, 100, 7), mod_pow(-2, 3, 5), mod_pow(3, 10 ** 18, 1000000007)]"), "[2, 2, 246336683]");
        assert_eq!(show("[fib(0), fib(1), fib(10), fib(100)]"), "[0, 1, 55, 354224848179261915075]");
        assert_eq!(
            show("[next_prime(0), next_prime(13), next_prime(2 ** 61 - 2), totient(1), totient(36), totient(97)]"),
            "[2, 17, 2305843009213693951, 1, 12, 96]"
        );
        assert_eq!(show("[isqrt(0), isqrt(99), isqrt(100), isqrt(10 ** 40)]"), "[0, 9, 10, 100000000000000000000]");
        assert_eq!(show("[(415/93).cfrac, 3.cfrac, (-7/2).cfrac, pi.cfrac(4), 0.75.cfrac]"), "[[4, 2, 6, 7], [3], [-4, 2], [3, 7, 15, 1], [0, 1, 3]]");
        assert!(try_eval("isqrt(-1)").is_err());
        assert!(try_eval("totient(0)").is_err());
        assert!(try_eval("factors(0)").is_err());
        assert!(try_eval("factorial(-1)").is_err());
        assert!(try_eval("mod_pow(2, 3, 0)").is_err());
        assert!(try_eval("mod_pow(2, -1, 4)").is_err());
    }

    #[test]
    fn combinatorics() {
        assert_eq!(show("[5!, 0!, 7!!, 8!!, 0!!, -3!, 2 ** 3!, 3! + 1, (2 + 1)!]"), "[120, 1, 105, 384, 1, -6, 64, 7, 6]");
        assert_eq!(show("[5 != 3, !false]"), "[true, true]");
        assert_eq!(
            show("[hyperfactorial(4), superfactorial(4), derangements(0), derangements(1), derangements(4), derangements(10)]"),
            "[27648, 288, 1, 0, 9, 1334961]"
        );
        assert_eq!(show("[permutations(5, 2), perm(5, 5), nPr(3, 4), nCr(5, 2)]"), "[20, 120, 0, 10]");
        assert_eq!(show("[choose(1/2, 2), choose(-1, 3), choose(-2, 2), choose(2.5, 2)]"), "[-1/8, -1, 3, 15/8]");
        assert_eq!(show("[1, 2, 3].permutations"), "[[1, 2, 3], [1, 3, 2], [2, 1, 3], [2, 3, 1], [3, 1, 2], [3, 2, 1]]");
        assert_eq!(show("[[1, 2, 3].permutations(2).len, [1, 2].permutations(3), [1, 2].permutations(0)]"), "[6, [], [[]]]");
        assert_eq!(show("[1, 2, 3, 4].combinations(2)"), "[[1, 2], [1, 3], [1, 4], [2, 3], [2, 4], [3, 4]]");
        assert!(try_eval("(-1)!").is_err());
    }
}
