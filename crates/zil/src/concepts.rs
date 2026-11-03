//! The values `help` explains as a topic of their own: one page per `type()` type, plus matrix, vector, duration and money.

use crate::modules::Rows;

pub struct Concept {
    pub name: &'static str,
    /// One lowercase line, no period.
    pub about: &'static str,
    /// Ways to make it, beside the `make` rows modules label with its name: (what it makes, example).
    pub make: Rows,
    /// Operators and forms that work on it: (what it does, example).
    pub syntax: Rows,
    /// The functions that take it, by name. Empty means every function whose first parameter takes a type of this name.
    pub takes: &'static [&'static str],
}

impl Concept {
    const EMPTY: Concept = Concept { name: "", about: "", make: &[], syntax: &[], takes: &[] };
}

pub fn find(name: &str) -> Option<&'static Concept> {
    CONCEPTS.iter().find(|c| c.name == name)
}

#[rustfmt::skip]
pub const CONCEPTS: &[Concept] = &[
    Concept { name: "nil", about: "no value, what a missing key or an `if` that didn't run gives",
        syntax: &[("compare it like any value", "nil == nil")], ..Concept::EMPTY },
    Concept { name: "bool", about: "true or false, what comparisons give",
        make: &[("a comparison", "3 > 2")], syntax: &[("and, or and not combine them", "true and not false")], ..Concept::EMPTY },
    Concept { name: "int", about: "a whole number of any size",
        make: &[("in another base", "0b1010 + 0xff"), ("digits grouped with underscores", "1_000_000")],
        syntax: &[("// is whole division, % the remainder", "17 // 5"), ("** is the power, with no overflow", "2 ** 100"), ("n! is the factorial", "5!")], ..Concept::EMPTY },
    Concept { name: "frac", about: "an exact fraction, so thirds stay thirds",
        make: &[("a division of whole numbers", "1 / 3"), ("a decimal made exact", "0.75 to frac")], syntax: &[("arithmetic stays exact", "1/3 + 1/6")], ..Concept::EMPTY },
    Concept { name: "float", about: "an approximate decimal number",
        make: &[("with an exponent", "1.5e3"), ("from a fraction", "1/3 to float")], syntax: &[("to float converts", r#""2.5" to float"#)], ..Concept::EMPTY },
    Concept { name: "quantity", about: "a number with a unit",
        make: &[("a number then a unit", "5 km"), ("a unit power", "5 m2")],
        syntax: &[("to converts", "5 km to mi"), ("* and / combine the units", "100 km / 2 h")], ..Concept::EMPTY },
    Concept { name: "uncertain", about: "a measurement with an error that carries through math",
        make: &[("a value ± an error", "10 ± 0.5")], syntax: &[("errors combine through arithmetic", "(10 ± 0.5) * 2")], ..Concept::EMPTY },
    Concept { name: "complex", about: "a number with a real and an imaginary part",
        make: &[("a number then i", "3 + 4i")], syntax: &[("arithmetic works as usual", "(1 + 2i) * (3 - 1i)")], ..Concept::EMPTY },
    Concept { name: "mod", about: "a number that wraps around, like a clock",
        make: &[("a number mod a modulus", "5 mod 7")], syntax: &[("arithmetic wraps around", "(5 mod 7) * 3")], ..Concept::EMPTY },
    Concept { name: "str", about: "text",
        make: &[("in double quotes", r#""hello""#), ("with values inside braces", r#""1 + 1 = {1 + 1}""#)],
        syntax: &[("+ joins, * repeats", r#""ab" * 3"#), ("s[i] and s[a..b] pick characters", r#""hello"[1..3]"#)], ..Concept::EMPTY },
    Concept { name: "bytes", about: "raw bytes, as read from a file or built by hand",
        make: &[("b then quotes", r#"b"\x7fELF""#), ("from text", r#""hi".bytes"#)], syntax: &[("b[i] is a byte's value", r#"b"hi"[0]"#)], ..Concept::EMPTY },
    Concept { name: "regex", about: "a pattern for matching text",
        make: &[("r then quotes", r#"r"\d+""#)], syntax: &[("a regex works where a function takes a pattern", r#""a1b22".find_all(r"\d+")"#)], ..Concept::EMPTY },
    Concept { name: "date", about: "a moment in time",
        make: &[("from text", r#"date("2026-12-25 18:30")"#), ("the current moment", "now")],
        syntax: &[("+ or - a duration moves it", "now + 2 days"), ("to a time zone", r#"now to "Tokyo""#)], ..Concept::EMPTY },
    Concept { name: "list", about: "an ordered collection of values",
        make: &[("in brackets", "[1, 2, 3]"), ("a range", "1..5")],
        syntax: &[("xs[i] and xs[a..b] pick items", "[10, 20, 30][1]"), ("xs.f(...) calls f with the list first", "[3, 1, 2].sort")], ..Concept::EMPTY },
    Concept { name: "map", about: "values stored under names",
        make: &[("in braces", "{a: 1, b: 2}")], syntax: &[("m.key and m[\"key\"] read a value", "{a: 1}.a")], ..Concept::EMPTY },
    Concept { name: "set", about: "a collection with no repeats and no order",
        make: &[("from values", "set(1, 2, 2)")], syntax: &[("| & - combine sets", "set(1, 2) | set(2, 3)")], ..Concept::EMPTY },
    Concept { name: "table", about: "rows of values under shared column names",
        make: &[("from a list of maps", "[{a: 1, b: 2}] to table")], syntax: &[("t.col is a column", "[{a: 1}, {a: 2}].table.a")], ..Concept::EMPTY },
    Concept { name: "fn", about: "a function, which can be passed around like any value",
        make: &[("a lambda", "|x| x * 2"), ("a typed function", "fn(x: int) -> int { x * 2 }")], syntax: &[("f(x) calls it", "(|x| x * 2)(4)")], ..Concept::EMPTY },
    Concept { name: "matrix", about: "a list of equal-length lists, rows of numbers",
        make: &[("in nested brackets", "[[1, 2], [3, 4]]"), ("filled with one value", "matrix(2, 3, 7)"), ("the identity", "identity(3)")],
        takes: &["matmul", "transpose", "det", "inv", "solve", "rref", "rank", "cofactor", "adjugate", "kron", "permanent", "identity", "matrix", "reshape", "row", "col", "hcat", "vcat"],
        ..Concept::EMPTY },
    Concept { name: "vector", about: "a list of numbers or quantities, a point or a direction",
        make: &[("in brackets", "[1, 2, 3]"), ("with units", "[3 m, 4 m]")], takes: &["dot", "cross", "norm", "triple"], ..Concept::EMPTY },
    Concept { name: "duration", about: "an amount of time",
        make: &[("with time units", "1h30m"), ("between two dates", r#"date("2027-01-01") - date("2026-12-25")"#)],
        syntax: &[("date + or - a duration moves it", "now + 90 min")], takes: &["diff", "age", "relative", "parts", "hms"], ..Concept::EMPTY },
    Concept { name: "money", about: "an amount in a currency, converted at live rates",
        make: &[("a currency symbol or code", "$5.20"), ("with a code", "100 EUR")], syntax: &[("add and compare amounts", "$5.20 + $1")],
        takes: &["share", "settle", "payment", "payoff", "amortize"], ..Concept::EMPTY },
];
