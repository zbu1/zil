//! Number theory on exact integers: gcd, primes, factoring, combinatorics.

use super::{big, int};
use crate::ast::BinOp;
use crate::modules::{Call, Claim, Fail, Module};
use crate::ops::mismatch;
use crate::value::Value;
use num_bigint::BigInt;
use num_integer::Integer;
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
    fns: fns::FNS,
    call: fns::call,
    binary: Some(binary),
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{arrangements, crt as solve, factor, gcd_lcm, inverse, is_prime as prime, pow};
    use crate::ast::Radix;
    use crate::modules::Fail;
    use crate::modules::math::big;
    use crate::signature::{As, List, Num, Or3, Rest};
    use crate::value::{Value, exact, num, ratio};
    use num_bigint::BigInt;
    use num_integer::Integer;
    use num_rational::BigRational;
    use num_traits::{One, Signed, Zero};

    fn ints(a: BigInt, b: BigInt, rest: Rest<BigInt>) -> Vec<Value> {
        [a, b].into_iter().chain(rest.0).map(big).collect()
    }

    /// greatest common divisor: the largest integer that divides all of them evenly
    #[builtin(examples = ["gcd(12, 18)", "[12, 18, 27].gcd"], see = ["lcm"])]
    fn gcd(a: BigInt, b: BigInt, rest: Rest<BigInt>) -> Result<As<BigInt>, Fail> {
        gcd_lcm("gcd", &ints(a, b, rest)).map(As::from)
    }

    #[builtin(name = "gcd")]
    fn gcd_of(xs: &List) -> Result<As<BigInt>, Fail> {
        gcd_lcm("gcd", &xs.borrow()).map(As::from)
    }

    /// least common multiple: the smallest positive integer they all divide evenly
    #[builtin(examples = ["lcm(4, 6)", "(1..=20).lcm"], see = ["gcd"])]
    fn lcm(a: BigInt, b: BigInt, rest: Rest<BigInt>) -> Result<As<BigInt>, Fail> {
        gcd_lcm("lcm", &ints(a, b, rest)).map(As::from)
    }

    #[builtin(name = "lcm")]
    fn lcm_of(xs: &List) -> Result<As<BigInt>, Fail> {
        gcd_lcm("lcm", &xs.borrow()).map(As::from)
    }

    /// whether n is prime
    ///
    /// Certain for n below about 3 × 10²⁴. Above that, a wrong answer is possible but astronomically unlikely (Miller-Rabin test)
    #[builtin(examples = ["is_prime(97)", "is_prime(2 ** 61 - 1)"], see = ["factors"])]
    fn is_prime(n: BigInt) -> bool {
        prime(&n)
    }

    /// The prime factors of a positive `n`, smallest first.
    fn primes(mut n: BigInt) -> Result<Vec<BigInt>, Fail> {
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
        Ok(out)
    }

    /// the primes that multiply to give n, smallest first, with repeats
    #[builtin(examples = ["360.factors", "factors(2 ** 32 + 1)"], see = ["is_prime", "gcd"])]
    fn factors(n: BigInt) -> Result<Vec<Value>, Fail> {
        Ok(primes(n)?.into_iter().map(big).collect())
    }

    /// n from 0 to 20000, with the bits its kind of factorial needs checked.
    fn factorial_arg(n: i64, steep: bool) -> Result<i64, Fail> {
        if !(0..=20_000).contains(&n) {
            return Err(Fail::Arg(0, format!("n must be from 0 to 20000, got {n}")));
        }
        let bits = (n as f64 * (n as f64).max(1.0).log2()) as u64;
        crate::state::check_bits(if steep { bits * n as u64 / 2 } else { bits })?;
        Ok(n)
    }

    /// n! = 1 × 2 × ... × n, exact
    ///
    /// Also written `n!`
    #[builtin(examples = ["factorial(5)", "30!"], see = ["choose", "double_factorial"])]
    fn factorial(n: i64) -> Result<BigInt, Fail> {
        Ok((1..=factorial_arg(n, false)?).map(BigInt::from).product())
    }

    /// n!! = n × (n - 2) × (n - 4) × ... down to 1 or 2, exact
    ///
    /// Also written `n!!`
    #[builtin(examples = ["double_factorial(7)", "8!!"], see = ["factorial"])]
    fn double_factorial(n: i64) -> Result<BigInt, Fail> {
        Ok((1..=factorial_arg(n, false)?).rev().step_by(2).map(BigInt::from).product())
    }

    /// 1¹ × 2² × ... × nⁿ, exact
    #[builtin(examples = ["hyperfactorial(4)"], see = ["superfactorial", "factorial"])]
    fn hyperfactorial(n: i64) -> Result<BigInt, Fail> {
        Ok((1..=factorial_arg(n, true)?).map(|i| BigInt::from(i).pow(i as u32)).product())
    }

    /// 1! × 2! × ... × n!, exact
    #[builtin(examples = ["superfactorial(4)"], see = ["hyperfactorial", "factorial"])]
    fn superfactorial(n: i64) -> Result<BigInt, Fail> {
        Ok((1..=factorial_arg(n, true)?)
            .scan(BigInt::one(), |f, i| {
                *f *= i;
                Some(f.clone())
            })
            .product())
    }

    /// how many ways n items can be shuffled so that none ends up in its original place, exact
    #[builtin(examples = ["derangements(4)"], see = ["permutations", "factorial"])]
    fn derangements(n: i64) -> Result<BigInt, Fail> {
        // !n = n·!(n-1) + (-1)^n
        Ok((1..=factorial_arg(n, false)?).fold(BigInt::one(), |d, i| d * i + if i % 2 == 0 { 1 } else { -1 }))
    }

    fn binomial(n: Num, k: i64) -> Result<As<Or3<BigInt, BigRational, f64>>, Fail> {
        if k < 0 {
            return Err(Fail::BadArgs);
        }
        if let Value::Int(n, _) = *n
            && n >= 0
        {
            if k > n {
                return Ok(Value::int(0).into());
            }
            let k = k.min(n - k);
            if k > 100_000 {
                return Err("result too large".into());
            }
            let mut r = BigInt::one();
            for i in 0..k {
                r = r * (n - i) / (i + 1);
            }
            return Ok(big(r).into());
        }
        // Generalized: n(n-1)...(n-k+1) / k!, exact for fractions and negative ints.
        if k > 10_000 {
            return Err("result too large".into());
        }
        Ok(match ratio(&n) {
            Some(r) => {
                let mut acc = BigRational::one();
                for i in 0..k {
                    acc = acc * (&r - BigRational::from_integer(i.into())) / BigRational::from_integer((i + 1).into());
                }
                exact(acc, Radix::DEC, true)
            }
            None => {
                let x = num(&n).ok_or(Fail::BadArgs)?;
                Value::Float((0..k).fold(1.0, |acc, i| acc * (x - i as f64) / (i + 1) as f64))
            }
        }
        .into())
    }

    /// how many ways there are to pick k things from n when order doesn't matter (the binomial coefficient), exact
    ///
    /// n can also be a fraction or negative, for the general formula used in series expansions
    #[builtin(examples = ["choose(5, 2)", "choose(52, 5)", "choose(1/2, 2)", "choose(-1, 3)"], see = ["permutations", "combinations", "factorial"])]
    fn choose(n: Num, k: i64) -> Result<As<Or3<BigInt, BigRational, f64>>, Fail> {
        binomial(n, k)
    }

    /// the same as choose, under the name calculators use
    #[builtin(examples = ["nCr(5, 2)"], see = ["choose"])]
    #[allow(non_snake_case)]
    fn nCr(n: Num, k: i64) -> Result<As<Or3<BigInt, BigRational, f64>>, Fail> {
        binomial(n, k)
    }

    fn count(n: i64, k: i64) -> Result<BigInt, Fail> {
        if n < 0 || k < 0 {
            return Err(Fail::BadArgs);
        }
        if k > n {
            return Ok(BigInt::zero());
        }
        crate::state::check_bits((k as f64 * (n as f64).log2()) as u64)?;
        Ok((n - k..n).map(|i| BigInt::from(i + 1)).product())
    }

    fn orderings(xs: &List, k: Option<i64>) -> Result<Vec<Value>, Fail> {
        let xs = xs.borrow().clone();
        let k = match k {
            None => xs.len(),
            Some(k) if k >= 0 => k as usize,
            Some(_) => return Err(Fail::Arg(1, "k must be an int, 0 or more".into())),
        };
        arrangements(&xs, k, true)
    }

    /// how many ways there are to pick k things from n when order matters, exact
    ///
    /// Given a list instead, every ordering of k of its items (all of them by default)
    #[builtin(examples = ["permutations(5, 2)", "[1, 2, 3].permutations", r#""abc".chars.permutations(2).len"#], see = ["choose", "combinations"])]
    fn permutations(n: i64, k: i64) -> Result<BigInt, Fail> {
        count(n, k)
    }

    #[builtin(name = "permutations")]
    fn permutations_of(xs: &List, k: Option<i64>) -> Result<Vec<Value>, Fail> {
        orderings(xs, k)
    }

    /// the same as permutations
    #[builtin(examples = ["perm(5, 2)"], see = ["permutations"])]
    fn perm(n: i64, k: i64) -> Result<BigInt, Fail> {
        count(n, k)
    }

    #[builtin(name = "perm")]
    fn perm_of(xs: &List, k: Option<i64>) -> Result<Vec<Value>, Fail> {
        orderings(xs, k)
    }

    /// the same as permutations, under the name calculators use
    #[builtin(examples = ["nPr(5, 2)"], see = ["permutations"])]
    #[allow(non_snake_case)]
    fn nPr(n: i64, k: i64) -> Result<BigInt, Fail> {
        count(n, k)
    }

    #[builtin(name = "nPr")]
    #[allow(non_snake_case)]
    fn nPr_of(xs: &List, k: Option<i64>) -> Result<Vec<Value>, Fail> {
        orderings(xs, k)
    }

    /// every way to pick k of the list's items when order doesn't matter, each as a list in the original order
    #[builtin(examples = ["[1, 2, 3, 4].combinations(2)"], see = ["choose", "permutations"])]
    fn combinations(xs: &List, k: i64) -> Result<Vec<Value>, Fail> {
        if k < 0 {
            return Err(Fail::BadArgs);
        }
        arrangements(&xs.borrow(), k as usize, false)
    }

    /// the remainder of b to the power e, divided by m, worked out without ever computing the huge power
    ///
    /// A negative e uses b's inverse, as in mod_inv
    #[builtin(examples = ["mod_pow(2, 100, 7)", "mod_pow(3, 10 ** 18, 1000000007)", "mod_pow(3, -1, 7)"], see = ["mod_inv", "gcd"])]
    fn mod_pow(b: BigInt, e: BigInt, m: BigInt) -> Result<BigInt, Fail> {
        if !m.is_positive() {
            return Err(Fail::Arg(2, format!("the modulus must be positive, got {m}")));
        }
        Ok(pow(&b, &e, &m)?)
    }

    /// the number b from 0 to m - 1 for which a × b leaves remainder 1 when divided by m
    ///
    /// An error if a and m share a factor, since then there isn't one
    #[builtin(examples = ["mod_inv(3, 7)", "mod_inv(5 mod 12)"], see = ["mod_pow", "crt"])]
    fn mod_inv(a: BigInt, m: BigInt) -> Result<BigInt, Fail> {
        if !m.is_positive() {
            return Err(Fail::Arg(1, format!("the modulus must be positive, got {m}")));
        }
        Ok(inverse(&a, &m)?.mod_floor(&m))
    }

    #[builtin(name = "mod_inv")]
    fn mod_inv_of(x: &(BigInt, BigInt)) -> Result<(BigInt, BigInt), String> {
        Ok((inverse(&x.0, &x.1)?, x.1.clone()))
    }

    /// the number that leaves remainder rs[0] when divided by ms[0], rs[1] when divided by ms[1], and so on (the Chinese remainder theorem)
    ///
    /// The answer is a mod, since adding the moduli's least common multiple gives another. An error if there's none
    #[builtin(examples = ["crt([2, 3, 2], [3, 5, 7])"], see = ["mod_inv"])]
    fn crt(rs: &List, ms: &List) -> Result<As<(BigInt, BigInt)>, Fail> {
        let (rs, ms) = (rs.borrow(), ms.borrow());
        if rs.len() != ms.len() {
            return Err(format!("{} remainders for {} moduli", rs.len(), ms.len()).into());
        }
        let int = crate::modules::math::int;
        let mut pairs = vec![];
        for (r, m) in rs.iter().zip(ms.iter()) {
            let m = int(m).map_err(|_| Fail::Arg(1, "moduli must be ints".into()))?;
            if !m.is_positive() {
                return Err(Fail::Arg(1, format!("moduli must be positive, got {m}")));
            }
            pairs.push((int(r).map_err(|_| Fail::Arg(0, "remainders must be ints".into()))?, m));
        }
        solve(pairs).map(As::from)
    }

    #[builtin(name = "crt", examples = ["crt([2 mod 3, 3 mod 5])"])]
    fn crt_of(xs: &List) -> Result<As<(BigInt, BigInt)>, Fail> {
        let pairs = xs
            .borrow()
            .iter()
            .map(|x| if let Value::Mod(x) = x { Ok((**x).clone()) } else { Err(Fail::Arg(0, format!("expected a list of mods, got a {}", x.type_name()))) })
            .collect::<Result<Vec<_>, _>>()?;
        solve(pairs).map(As::from)
    }

    /// the plain integer a mod value is at, from 0 to its modulus - 1
    #[builtin(examples = ["(17 mod 5).residue"], see = ["modulus"])]
    fn residue(x: &(BigInt, BigInt)) -> BigInt {
        x.0.clone()
    }

    /// the number a mod value wraps around at
    #[builtin(examples = ["(17 mod 5).modulus"], see = ["residue"])]
    fn modulus(x: &(BigInt, BigInt)) -> BigInt {
        x.1.clone()
    }

    /// the remainder of a divided by b, with the same sign as a, as C's and JavaScript's `%` give it
    ///
    /// zil's own `%` is never negative
    #[builtin(examples = ["rem(-7, 3)", "-7 % 3"], see = ["mod_pow"])]
    fn rem(a: BigInt, b: BigInt) -> Result<BigInt, String> {
        if b.is_zero() {
            return Err("division by zero".into());
        }
        Ok(a % b)
    }

    #[builtin(name = "rem")]
    fn rem_num(a: f64, b: f64) -> f64 {
        a % b
    }

    /// the nth Fibonacci number, exact
    ///
    /// 0, 1, 1, 2, 3, 5, 8, ..., each the sum of the two before, starting from fib(0) = 0
    #[builtin(examples = ["fib(10)", "fib(100)"], see = ["factorial"])]
    fn fib(n: i64) -> Result<BigInt, Fail> {
        if !(0..=100_000).contains(&n) {
            return Err(Fail::Arg(0, format!("n must be from 0 to 100000, got {n}")));
        }
        let (mut a, mut b) = (BigInt::zero(), BigInt::one());
        for _ in 0..n {
            (a, b) = (b.clone(), a + b);
        }
        Ok(a)
    }

    /// the smallest prime greater than n
    #[builtin(examples = ["next_prime(100)", "next_prime(2 ** 64)"], see = ["is_prime"])]
    fn next_prime(n: BigInt) -> BigInt {
        let mut n = n.max(BigInt::one()) + 1u32;
        while !prime(&n) {
            n += 1u32;
        }
        n
    }

    /// Euler's totient φ(n)
    ///
    /// How many of the numbers from 1 to n share no factor with n
    #[builtin(examples = ["totient(36)", "totient(97)"], see = ["factors", "gcd"])]
    fn totient(n: BigInt) -> Result<BigInt, Fail> {
        let mut ps = primes(n.clone())?;
        ps.dedup();
        Ok(ps.iter().fold(n, |acc, p| acc / p * (p - 1u32)))
    }

    /// the integer part of the square root of n, exact at any size
    #[builtin(examples = ["isqrt(99)", "isqrt(10 ** 40)"], see = ["sqrt"])]
    fn isqrt(n: BigInt) -> Result<BigInt, Fail> {
        if n.is_negative() {
            return Err(Fail::Arg(0, format!("expected 0 or more, got {n}")));
        }
        Ok(n.sqrt())
    }

    /// x as a continued fraction, the list [a0, a1, a2, ...] meaning a0 + 1/(a1 + 1/(a2 + ...))
    ///
    /// Cutting it short gives the best fractions close to x. Exact for fractions, while floats give up to terms numbers
    #[builtin(examples = ["(415/93).cfrac", "pi.cfrac(5)", "sqrt(2).cfrac(6)"], see = ["frac"])]
    fn cfrac(x: Num, #[default(20)] terms: i64) -> Result<Vec<Value>, Fail> {
        if !(1..=1000).contains(&terms) {
            return Err(Fail::Arg(1, "terms must be an int from 1 to 1000".into()));
        }
        let terms = terms as usize;
        let mut out = vec![];
        if let Some(r) = ratio(&x) {
            let (mut p, mut q) = (r.numer().clone(), r.denom().clone());
            while !q.is_zero() && out.len() < terms {
                let (a, rem) = p.div_mod_floor(&q);
                out.push(big(a));
                (p, q) = (q, rem);
            }
        } else {
            let mut x = num(&x).ok_or(Fail::BadArgs)?;
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
        Ok(out)
    }
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
    let s = n1.trailing_zeros().unwrap_or_default();
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
