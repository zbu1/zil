//! Number formatting: format specs, printf, fixed/sci/percent/commas, words, ordinals, Roman numerals, human_bytes.

use crate::modules::{Call, Fail, Module};
use crate::value::{Value, fmt_float, num, ratio};
use num_traits::Signed;

pub const MODULE: Module = Module {
    name: "formatting",
    about: "numbers as text: fixed decimals, commas, percentages, words, ordinals and Roman numerals",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Numbers as text, laid out the way you want."),
        ("writing", "\
Inside a string, `{x}` puts in the value of x. Add a colon and a format spec to control how it looks, as in Python: \
`\"{pi:.2f}\"` shows pi with 2 decimals. `format` takes the older printf style that C and many other languages use, \
where `%5.2f` means a number 5 characters wide with 2 decimals."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("formatting", &[
            ("separators and 2 decimals", r#""{1234567.891:,.2f}""#),
            ("hex with a prefix", r#""{255:#06x}""#),
            ("32-bit binary", r#""{0xdeadbeef:032b}""#),
            ("this year", "2026 to roman"),
            ("the Super Bowl after LX", r#"roman("LX") + 1 to roman"#),
        ]),
    ],
    #[rustfmt::skip]
    guide: &[
        ("format specs", &[
            (r#""{x:.2f}": exactly 2 decimals"#, r#""{pi:.2f}""#),
            (r#""{x:>8}": 8 wide, aligned right, < is left, ^ centered"#, r#""[{42:>8}]""#),
            (r#""{n:08x}": hex, padded with zeros to 8, X for capitals, b binary, o octal, d decimal"#, r#""{255:08x}""#),
            (r#""{n:#06x}": # adds the 0x, 0b or 0o prefix"#, r#""{255:#06x}""#),
            (r#""{x:,}": thousands separators, + always shows a sign, e scientific, % a percentage"#, r#""{1234567:,}""#),
            (r#"format("%5.2f", x): C's printf style"#, r#"format("%5.2f|%-4d|", pi, 7)"#),
        ]),
        ("conversions", &[("to words: in English words", "1999 to words"), ("to roman: as a Roman numeral", "1999 to roman")]),
    ],
    fns: fns::FNS,
    call: fns::call,
    targets: &[("words", "spell"), ("roman", "roman")],
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{Spec, human_bytes as bytes_text, render, roman as numeral, spell as words};
    use crate::modules::Fail;
    use crate::modules::units::{terms, to_unit};
    use crate::signature::{As, Num, Or, Qty};
    use crate::value::{Value, num};

    fn show(x: Or<Num, Qty>, digits: Option<i64>, kind: Option<char>, commas: bool) -> Result<String, Fail> {
        let digits = match digits {
            None => None,
            Some(d) if (0..=100).contains(&d) => Some(d as usize),
            Some(_) => return Err(Fail::BadArgs),
        };
        let kind = kind.or(digits.map(|_| 'f'));
        Ok(render(x.value(), &Spec { kind, prec: digits, commas, ..Spec::PLAIN })?)
    }

    /// x as text with exactly that many decimals, padding with zeros if needed
    ///
    /// Keeps units
    #[builtin(examples = ["pi.fixed(2)", "(5 km to mi).fixed(1)"], see = ["sci", "commas", "round"])]
    fn fixed(x: Or<Num, Qty>, digits: i64) -> Result<String, Fail> {
        show(x, Some(digits), None, false)
    }

    /// x as text in scientific notation, like 1.23e5 for 123000, with digits decimals
    #[builtin(examples = ["123456.sci", "123456.sci(2)"], see = ["fixed"])]
    fn sci(x: Or<Num, Qty>, digits: Option<i64>) -> Result<String, Fail> {
        show(x, digits, Some('e'), false)
    }

    /// a fraction as percentage text, so 0.256 is "25.6%", with digits decimals
    #[builtin(examples = ["0.256.percent", "(1/3).percent(1)"], see = ["fixed"])]
    fn percent(x: Or<Num, Qty>, digits: Option<i64>) -> Result<String, Fail> {
        show(x, digits, Some('%'), false)
    }

    /// x as text with commas between thousands, like "1,234,567", with digits decimals
    #[builtin(examples = ["1234567.commas", "1234.5.commas(2)"], see = ["fixed"])]
    fn commas(x: Or<Num, Qty>, digits: Option<i64>) -> Result<String, Fail> {
        show(x, digits, None, true)
    }

    /// an integer in English words, like "forty-two"
    ///
    /// Same as `n to words`
    #[builtin(examples = ["spell(42)", "1999 to words", "-1000001 to words"], see = ["ordinal", "roman"])]
    fn spell(n: i64) -> String {
        words(n)
    }

    /// the number with its English ordinal ending: 1st, 2nd, 3rd, 4th, 11th, 21st and so on
    #[builtin(examples = ["ordinal(3)", "(1..=4).map(ordinal)", "ordinal(112)"], see = ["spell"])]
    fn ordinal(n: i64) -> String {
        let suffix = match (n.unsigned_abs() % 10, n.unsigned_abs() % 100) {
            (_, 11..=13) => "th",
            (1, _) => "st",
            (2, _) => "nd",
            (3, _) => "rd",
            _ => "th",
        };
        format!("{n}{suffix}")
    }

    /// a number of bytes as text in the largest binary unit that fits, like "117.7 MiB", with one decimal
    ///
    /// Binary units step by 1024, so 1 KiB is 1024 bytes
    #[builtin(examples = ["123456789.human_bytes", "1023.human_bytes", "3 MB.human_bytes"], see = ["fixed"])]
    fn human_bytes(n: Or<f64, Qty>) -> Result<String, String> {
        Ok(bytes_text(match n {
            Or::A(n) => n,
            Or::B(q) => {
                let (n, u) = q.get();
                match to_unit(n, u, terms("B").expect("bytes are a unit"))? {
                    Value::Qty(b, _) => b,
                    v => num(&v).unwrap_or(f64::NAN),
                }
            }
        }))
    }

    /// an integer from 1 to 3999 as a Roman numeral, or a numeral back to an integer
    ///
    /// Same as `n to roman`
    #[builtin(examples = ["roman(2026)", r#""MCMXCIX".roman"#])]
    fn roman(n: i64) -> Result<As<String>, Fail> {
        numeral(&[Value::int(n)]).map(As::from)
    }

    #[builtin(name = "roman")]
    fn roman_value(s: &str) -> Result<As<i64>, Fail> {
        numeral(&[Value::str(s)]).map(As::from)
    }
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
const SCALES: [&str; 7] = ["", "thousand", "million", "billion", "trillion", "quadrillion", "quintillion"];

/// `-1234` → "minus one thousand two hundred thirty-four" (American style: no "and").
fn spell(n: i64) -> String {
    let below_1000 = |n: u64| {
        let (h, r) = (n / 100, n % 100);
        let mut w = vec![];
        if h > 0 {
            w.push(format!("{} hundred", ONES[h as usize]));
        }
        match r {
            0 => {}
            1..20 => w.push(ONES[r as usize].into()),
            _ if r % 10 == 0 => w.push(TENS[r as usize / 10].into()),
            _ => w.push(format!("{}-{}", TENS[r as usize / 10], ONES[r as usize % 10])),
        }
        w.join(" ")
    };
    if n == 0 {
        return "zero".into();
    }
    let mut m = n.unsigned_abs();
    let mut groups = vec![];
    for scale in SCALES {
        let g = m % 1000;
        if g > 0 {
            groups.push(if scale.is_empty() { below_1000(g) } else { format!("{} {scale}", below_1000(g)) });
        }
        m /= 1000;
    }
    groups.reverse();
    format!("{}{}", if n < 0 { "minus " } else { "" }, groups.join(" "))
}

fn human_bytes(mut n: f64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut i = 0;
    while n.abs() >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{n:.1} {}", UNITS[i]) }
}

/// A format spec, as in Python: `[[fill]align][+][0][width][,][.precision][type]`, type one of
/// `f e % x X b o d s`. Serves `"{x:spec}"`, `format("%...", ...)` and fixed/sci/percent/commas.
pub struct Spec {
    fill: char,
    align: Option<char>,
    plus: bool,
    /// `#`: 0x / 0b / 0o prefix on x X b o.
    alt: bool,
    zero: bool,
    width: usize,
    commas: bool,
    prec: Option<usize>,
    kind: Option<char>,
}

impl Spec {
    const PLAIN: Spec = Spec { fill: ' ', align: None, plus: false, alt: false, zero: false, width: 0, commas: false, prec: None, kind: None };

    pub fn parse(s: &str) -> Result<Spec, String> {
        let c: Vec<char> = s.chars().collect();
        let at = |i: usize| c.get(i).copied().unwrap_or('\0');
        let (mut sp, mut i) = (Spec::PLAIN, 0);
        if matches!(at(1), '<' | '>' | '^') {
            (sp.fill, sp.align, i) = (c[0], Some(c[1]), 2);
        } else if matches!(at(0), '<' | '>' | '^') {
            (sp.align, i) = (Some(c[0]), 1);
        }
        let flag = |ch: char, i: &mut usize| {
            let hit = at(*i) == ch;
            *i += hit as usize;
            hit
        };
        let number = |i: &mut usize| {
            let start = *i;
            while at(*i).is_ascii_digit() {
                *i += 1;
            }
            c[start..*i].iter().collect::<String>().parse::<usize>().ok().filter(|n| *n <= 1000)
        };
        sp.plus = flag('+', &mut i);
        sp.alt = flag('#', &mut i);
        sp.zero = flag('0', &mut i);
        sp.width = number(&mut i).unwrap_or(0);
        sp.commas = flag(',', &mut i);
        if flag('.', &mut i) {
            sp.prec = Some(number(&mut i).ok_or_else(|| format!("bad precision in format spec {s:?}"))?);
        }
        if at(i) != '\0' && "fe%xXbods".contains(at(i)) {
            sp.kind = Some(at(i));
            i += 1;
        }
        if i != c.len() {
            return Err(format!("bad format spec {s:?}"));
        }
        Ok(sp)
    }
}

pub fn render(v: &Value, sp: &Spec) -> Result<String, String> {
    let (n, unit) = match v {
        Value::Qty(x, u) => (Value::Float(*x), Some(u)),
        _ => (v.clone(), None),
    };
    let numeric = num(&n).is_some();
    let x = || num(&n).ok_or_else(|| format!("cannot format {} as a number", v.type_name()));
    let mut s = match sp.kind {
        Some('f') => format!("{:.*}", sp.prec.unwrap_or(6), x()?),
        Some('e') => match sp.prec {
            Some(p) => format!("{:.*e}", p, x()?),
            None => format!("{:e}", x()?),
        },
        Some('%') => match sp.prec {
            Some(p) => format!("{:.*}%", p, x()? * 100.0),
            None => format!("{}%", fmt_float(x()? * 100.0)),
        },
        Some(k @ ('x' | 'X' | 'b' | 'o' | 'd')) => {
            let i = ratio(&n).filter(|r| r.is_integer()).ok_or_else(|| format!("cannot format {} as an integer", v.type_name()))?.to_integer();
            let base = match k {
                'x' | 'X' => 16,
                'b' => 2,
                'o' => 8,
                _ => 10,
            };
            let d = i.abs().to_str_radix(base);
            let d = if k == 'X' { d.to_uppercase() } else { d };
            let prefix = match k {
                _ if !sp.alt => "",
                'x' => "0x",
                'X' => "0X",
                'b' => "0b",
                'o' => "0o",
                _ => "",
            };
            format!("{}{prefix}{d}", if i.is_negative() { "-" } else { "" })
        }
        _ if numeric && sp.prec.is_some() => format!("{:.*}", sp.prec.unwrap(), x()?),
        _ => {
            let s = n.to_string();
            match sp.prec {
                Some(p) => s.chars().take(p).collect(),
                None => s,
            }
        }
    };
    if sp.commas && numeric && !matches!(sp.kind, Some('x' | 'X' | 'b' | 'o')) {
        s = commas(&s);
    }
    if sp.plus && numeric && !s.starts_with('-') {
        s.insert(0, '+');
    }
    if let Some(u) = unit {
        s = format!("{s} {u}");
    }
    crate::state::check_len(sp.width, "characters")?;
    let pad = sp.width.saturating_sub(s.chars().count());
    if sp.zero && sp.align.is_none() && numeric {
        // Zeros go after the sign and any `#` prefix: `{255:#06x}` is `0x00ff`.
        let prefix = 2 * (sp.alt && matches!(sp.kind, Some('x' | 'X' | 'b' | 'o'))) as usize;
        s.insert_str(s.starts_with(['-', '+']) as usize + prefix, &"0".repeat(pad));
        return Ok(s);
    }
    let fill = |k: usize| sp.fill.to_string().repeat(k);
    Ok(match sp.align.unwrap_or(if numeric { '>' } else { '<' }) {
        '<' => s + &fill(pad),
        '^' => fill(pad / 2) + &s + &fill(pad - pad / 2),
        _ => fill(pad) + &s,
    })
}

/// Thousands separators in the first run of digits: `-1234.5` -> `-1,234.5`.
pub fn commas(s: &str) -> String {
    let start = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let end = s[start..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |e| start + e);
    let d = &s[start..end];
    let mut out = String::new();
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}{out}{}", &s[..start], &s[end..])
}

/// `format("%5.2f and %d", x, n)`: printf-style, translated to a `Spec`. Flags `- + # 0 ,`; `%%` is a literal `%`.
pub fn printf(f: &str, args: &[Value]) -> Result<String, String> {
    let (mut out, mut args, mut chars) = (String::new(), args.iter(), f.chars());
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut body = String::new();
        loop {
            let ch = chars.next().ok_or("unfinished % at the end of the format")?;
            body.push(ch);
            if ch.is_ascii_alphabetic() || ch == '%' {
                break;
            }
        }
        if body == "%" {
            out.push('%');
            continue;
        }
        let bad = format!("bad format %{body}");
        let kind = match body.pop().unwrap() {
            'i' | 'u' => 'd',
            k => k,
        };
        let flags: String = body.chars().take_while(|c| "-+#0,".contains(*c)).collect();
        let rest = &body[flags.len()..];
        let (width, prec) = rest.split_once('.').map_or((rest, None), |(w, p)| (w, Some(p)));
        let spec = format!(
            "{}{}{}{}{width}{}{}{kind}",
            if flags.contains('-') { "<" } else { "" },
            if flags.contains('+') { "+" } else { "" },
            if flags.contains('#') { "#" } else { "" },
            if flags.contains('0') { "0" } else { "" },
            if flags.contains(',') { "," } else { "" },
            prec.map(|p| format!(".{p}")).unwrap_or_default(),
        );
        let v = args.next().ok_or("more % specs than arguments")?;
        out += &render(v, &Spec::parse(&spec).map_err(|_| bad)?)?;
    }
    if args.next().is_some() {
        return Err("more arguments than % specs".into());
    }
    Ok(out)
}

const DIGITS: [(i64, &str); 13] =
    [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];

/// `roman(n)` or `roman("XIV")`.
fn roman(args: &[Value]) -> Call {
    match args {
        [Value::Int(n, _)] if (1..4000).contains(n) => Ok(Value::str(to_roman(*n))),
        [Value::Int(n, _)] => Err(format!("`{n}` has no Roman numeral\nnote: Roman numerals go from 1 to 3999").into()),
        [Value::Str(s)] => {
            let s = s.trim().to_uppercase();
            // Read greedily, then check by writing it back: rejects IIII, IC, VX and friends.
            let (mut n, mut rest) = (0, s.as_str());
            for (v, d) in DIGITS {
                while let Some(r) = rest.strip_prefix(d) {
                    n += v;
                    rest = r;
                }
            }
            if !rest.is_empty() || n == 0 || to_roman(n) != s {
                return Err(format!("{s:?} is not a Roman numeral").into());
            }
            Ok(Value::int(n))
        }
        _ => Err(Fail::BadArgs),
    }
}

fn to_roman(mut n: i64) -> String {
    let mut s = String::new();
    for (v, d) in DIGITS {
        while n >= v {
            s += d;
            n -= v;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn human_bytes() {
        assert_eq!(show("123456789.human_bytes"), "117.7 MiB");
        assert_eq!(show("[0, 1023, 1024].map(human_bytes)"), r#"["0 B", "1023 B", "1.0 KiB"]"#);
    }

    #[test]
    fn formatting() {
        assert_eq!(show("[0.spell, 13.spell, 40.spell, 999 to words]"), r#"["zero", "thirteen", "forty", "nine hundred ninety-nine"]"#);
        assert_eq!(show("-1000001 to words"), "minus one million one");
        assert!(show("9223372036854775807.spell").starts_with("nine quintillion two hundred twenty-three quadrillion"));
        assert_eq!(
            show("[1, 2, 3, 4, 11, 12, 13, 21, 22, 111, 123, -1].map(ordinal).join(\" \")"),
            "1st 2nd 3rd 4th 11th 12th 13th 21st 22nd 111th 123rd -1st"
        );
        assert_eq!(show(r#"["{255:#x}", "{255:#06x}", "{255:#X}", "{5:#b}", format("%#o", 8)]"#), r#"["0xff", "0x00ff", "0XFF", "0b101", "0o10"]"#);
        assert_eq!(show("[pi.fixed(2), (5 km to mi).fixed(1), 2.fixed(0)]"), r#"["3.14", "3.1 mi", "2"]"#);
        assert_eq!(show("[123456.sci, 123456.sci(2), 0.256.percent, (1/3).percent(1)]"), r#"["1.23456e5", "1.23e5", "25.6%", "33.3%"]"#);
        assert_eq!(
            show("[1234567.commas, (-1234.5).commas(2), (2 ** 70).commas, 999.commas]"),
            r#"["1,234,567", "-1,234.50", "1,180,591,620,717,411,303,424", "999"]"#
        );
        assert_eq!(
            show(
                r#"x = 3.14159
n = 255
"{x:.2f}|{n:>6}|{n:<6}|{n:^7}|{n:08x}|{n:X}|{n:b}|{-n:o}|{x:+.1f}|{1234567:,}|{-42:06}|{"ab":*>4}|{0.5:%}|{1/8:.1%}""#
            ),
            "3.14|   255|255   |  255  |000000ff|FF|11111111|-377|+3.1|1,234,567|-00042|**ab|50%|12.5%"
        );
        assert_eq!(
            show(
                r#"d = 5 km
"{d:.1f} / {d:>8}""#
            ),
            "5.0 km /     5 km"
        );
        assert_eq!(
            show(r#"format("%5.2f|%-4d|%05d|%x|%s|%,d|%%|%+.1e", 3.14159, 7, 42, 255, "hi", 1234567, 1234.5)"#),
            " 3.14|7   |00042|ff|hi|1,234,567|%|+1.2e3"
        );
        assert_eq!(
            show(
                r#"x = {a: 1}
"{ {b: x.a}.b :>3}""#
            ),
            "  1"
        );
        assert!(try_eval(r#""{1:q}""#).is_err());
        assert!(try_eval(r#""{1.5:x}""#).is_err());
        assert!(try_eval(r#"format("%d %d", 1)"#).is_err());
        assert!(try_eval(r#"format("%d", 1, 2)"#).is_err());
    }

    #[test]
    fn roman() {
        assert_eq!(show("2026 to roman"), "MMXXVI");
        assert_eq!(show("3999.roman"), "MMMCMXCIX");
        assert_eq!(show(r#""mcmxcix".roman"#), "1999");
        assert!((1..4000).all(|n| show(&format!("{n}.roman.roman")) == n.to_string()));
        assert!(try_eval(r#""IIII".roman"#).is_err());
        assert!(try_eval(r#""IC".roman"#).is_err());
        assert!(try_eval("0.roman").is_err());
    }
}
