//! Tokens and the logos lexer: numbers, strings, operators, comments.

use crate::Error;
use crate::ast::BinOp;
use logos::Logos;
use num_bigint::BigInt;
use num_rational::BigRational;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")]
#[logos(skip(r"#[^\n]*", allow_greedy = true))]
pub enum Tok {
    #[token("fn")]
    Fn,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("to")]
    To,
    /// `20% of 50`: multiplication.
    #[token("of")]
    Of,
    /// `5 mod 7`: a modular value.
    #[token("mod")]
    Mod,
    #[token("return")]
    Return,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("nil")]
    Nil,

    /// Scientific notation stays a float: `6.022e23`, `1e-9`.
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?[eE][+-]?[0-9]+", |l| l.slice().replace('_', "").parse().ok())]
    Float(f64),
    /// A plain decimal, kept exact so `0.1 + 0.2 == 0.3`.
    // ponytail: repeated decimal multiplies in a loop grow the denominator; fall back to f64 past a size cap if that gets slow.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", |l| decimal(l.slice()))]
    Dec(BigRational),
    #[regex(r"[0-9][0-9_]*", |l| l.slice().replace('_', "").parse().ok())]
    Int(i64),
    /// Integer literal that keeps its base: `0xff`, `0b101`, `0o17`, `36#z`.
    #[regex(r"0x[0-9a-fA-F_]+", |l| radix(&l.slice()[2..], 16))]
    #[regex(r"0b[01_]+", |l| radix(&l.slice()[2..], 2))]
    #[regex(r"0o[0-7_]+", |l| radix(&l.slice()[2..], 8))]
    #[regex(r"[0-9]+#[0-9a-zA-Z_]+", |l| based(l.slice()))]
    Based((i64, u32)),
    /// Compact duration: `1h30m`, `2d4h`, `1m30s` (`m` is minutes here); single `90s` stays a plain quantity.
    #[regex(r"[0-9]+[dhms]([0-9]+[dhms])+", |l| duration(l.slice()))]
    Dur(Vec<(i64, String)>),
    /// Time of day: `9:30`, `17:45:10`, `5pm`, `5:30am`. Touching `pm` is a clock time, so picometres need a space.
    #[regex(r"[0-9]{1,2}:[0-9]{2}(:[0-9]{2})?([ap]m)?", |l| l.slice().to_string())]
    #[regex(r"[0-9]{1,2}[ap]m", |l| l.slice().to_string())]
    Clock(String),
    /// Integer literal too big for i64, and its base; made by `lex_at`, not logos.
    Big((BigInt, u32)),
    /// A character no token starts with, made by `lex_loose`: an error only if the parser reaches it outside command words.
    Bad,
    /// Raw contents between the quotes; escapes and `{}` interpolation are handled by the parser.
    #[token("\"", lex_string)]
    Str(String),
    #[regex(r#"r"[^"]*""#, |l| { let s = l.slice(); s[2..s.len() - 1].to_string() })]
    Regex(String),
    /// Raw contents of `b"..."`; the parser decodes escapes, and there's no interpolation.
    #[regex(r#"b"([^"\\]|\\.)*""#, |l| { let s = l.slice(); s[2..s.len() - 1].to_string() })]
    Bytes(String),
    #[regex(r"[A-Za-z_ΩµμΩÅ‰‱][A-Za-z0-9_ΩµμΩÅ‰‱]*", |l| l.slice().to_string())]
    Ident(String),
    /// `$25` is `25 USD`: the currency code for `$`, `€` or `£`.
    #[regex("[$€£]", |l| match l.slice() { "$" => "USD", "€" => "EUR", _ => "GBP" }.to_string())]
    Currency(String),

    #[token("\n")]
    #[token(";")]
    Newline,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,
    #[token("..")]
    DotDot,
    #[token("..=")]
    DotDotEq,
    #[token("|>")]
    Pipe,
    #[token("=")]
    Assign,
    /// `+=`, `//=`, `<<=`, ...
    #[regex(r"(\+|-|\*\*|\*|//|/|%|&|\||\^|<<|>>)=", |l| op_assign(l.slice()))]
    OpAssign(BinOp),
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    /// `5 ± 0.1`: a value with an uncertainty.
    #[token("±")]
    #[token("+-")]
    PlusMinus,
    #[token("*")]
    Star,
    #[token("**")]
    StarStar,
    #[token("/")]
    Slash,
    #[token("//")]
    SlashSlash,
    #[token("%")]
    Percent,
    #[token("^")]
    Caret,
    #[token("!")]
    Bang,
    #[token("==")]
    Eq,
    #[token("!=")]
    Ne,
    #[token("<")]
    Lt,
    #[token("<=")]
    Le,
    #[token(">")]
    Gt,
    #[token(">=")]
    Ge,
    #[token("&&")]
    And,
    #[token("||")]
    Or,
    #[token("&")]
    Amp,
    #[token("|")]
    Bar,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token("~")]
    Tilde,

    Eof,
}

fn lex_string(l: &mut logos::Lexer<Tok>) -> Option<String> {
    let rest = l.remainder();
    let end = string_end(rest)?;
    l.bump(end + 1);
    Some(rest[..end].to_string())
}

/// Index of the closing quote, skipping escapes and quotes nested inside `{...}` interpolations.
pub(crate) fn string_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let (mut i, mut depth) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'"' if depth == 0 => return Some(i),
            b'"' => i += 1 + string_end(&s[i + 1..])?,
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// `36#z`
fn based(s: &str) -> Option<(i64, u32)> {
    let (b, d) = s.split_once('#')?;
    radix(d, b.parse().ok()?)
}

fn radix(digits: &str, base: u32) -> Option<(i64, u32)> {
    if !(2..=36).contains(&base) {
        return None;
    }
    Some((i64::from_str_radix(&digits.replace('_', ""), base).ok()?, base))
}

fn duration(s: &str) -> Option<Vec<(i64, String)>> {
    let mut parts = Vec::new();
    for (n, u) in s.split_inclusive(['d', 'h', 'm', 's']).map(|p| p.split_at(p.len() - 1)) {
        let u = match u {
            "d" => "d",
            "h" => "h",
            "m" => "min",
            _ => "s",
        };
        parts.push((n.parse().ok()?, u.to_string()));
    }
    Some(parts)
}

fn op_assign(s: &str) -> BinOp {
    match &s[..s.len() - 1] {
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "**" => BinOp::Pow,
        "/" => BinOp::Div,
        "//" => BinOp::IntDiv,
        "%" => BinOp::Rem,
        "&" => BinOp::BitAnd,
        "|" => BinOp::BitOr,
        "^" => BinOp::BitXor,
        "<<" => BinOp::Shl,
        _ => BinOp::Shr,
    }
}

/// `12.50` as exactly 1250/100.
fn decimal(s: &str) -> Option<BigRational> {
    let s = s.replace('_', "");
    let (int, frac) = s.split_once('.')?;
    let n = format!("{int}{frac}").parse::<BigInt>().ok()?;
    Some(BigRational::new(n, BigInt::from(10).pow(frac.len() as u32)))
}

/// An integer literal that overflowed i64: `99999999999999999999`, `0xffffffffffffffffff`, `36#zzzzzzzzzzzzzz`.
fn big(s: &str) -> Option<(BigInt, u32)> {
    let s = s.replace('_', "");
    let (digits, base) = match s.get(..2) {
        Some("0x") => (&s[2..], 16),
        Some("0b") => (&s[2..], 2),
        Some("0o") => (&s[2..], 8),
        _ => match s.split_once('#') {
            Some((b, d)) => (d, b.parse().ok()?),
            None => (&s[..], 10),
        },
    };
    if !(2..=36).contains(&base) || !digits.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((BigInt::parse_bytes(digits.as_bytes(), base)?, base))
}

pub type Span = std::ops::Range<usize>;

/// Lex `src`, shifting spans by `offset` (used for `{...}` inside strings).
pub fn lex_at(src: &str, offset: usize) -> Result<Vec<(Tok, Span)>, Error> {
    lex_with(src, offset, false)
}

/// Like `lex`, but an unknown character becomes `Tok::Bad`, since command words may hold one: `cd C:\src`.
pub fn lex_loose(src: &str) -> Result<Vec<(Tok, Span)>, Error> {
    lex_with(src, 0, true)
}

fn lex_with(src: &str, offset: usize, loose: bool) -> Result<Vec<(Tok, Span)>, Error> {
    let mut toks = Vec::new();
    for (tok, span) in Tok::lexer(src).spanned() {
        let span = span.start + offset..span.end + offset;
        match tok {
            Ok(t) => toks.push((t, span)),
            Err(()) => {
                let text = &src[span.start - offset..span.end - offset];
                match big(text) {
                    Some(b) => toks.push((Tok::Big(b), span)),
                    None if text == "\"" => return Err(unclosed_string(src, span.start - offset, offset)),
                    None if loose => toks.push((Tok::Bad, span)),
                    None => return Err(Error::new(format!("unexpected character `{text}`"), span)),
                }
            }
        }
    }
    let end = src.len() + offset;
    toks.push((Tok::Eof, end..end));
    Ok(toks)
}

/// The string opened at byte `at` never closes: either a `{` inside it doesn't, or the closing quote is missing.
fn unclosed_string(src: &str, at: usize, offset: usize) -> Error {
    let quote = at + offset..at + offset + 1;
    let (mut opens, mut esc) = (Vec::new(), false);
    for (i, c) in src[at + 1..].char_indices() {
        let i = at + 1 + i + offset;
        match (esc, c) {
            (true, _) => esc = false,
            (_, '\\') => esc = true,
            (_, '{') => opens.push(i),
            (_, '}') => drop(opens.pop()),
            (_, '"') if !opens.is_empty() => {
                let brace = opens[0]..opens[0] + 1;
                return Error::new("unclosed `{` in a string", brace.clone())
                    .label(brace, "opened here")
                    .label(i..i + 1, "string ends here")
                    .help("close it with `}`, or write `\\{` for a literal brace");
            }
            _ => {}
        }
    }
    let end = src.len() + offset;
    Error::new("unclosed string", quote.clone()).label(quote, "opened here").label(end..end, "expected `\"` here")
}

pub fn lex(src: &str) -> Result<Vec<(Tok, Span)>, Error> {
    lex_at(src, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(
            toks("0xff 0b101 36#z 1_000 2.5e3 1..3 1h30m 9:30 5pm # c"),
            vec![
                Tok::Based((255, 16)),
                Tok::Based((5, 2)),
                Tok::Based((35, 36)),
                Tok::Int(1000),
                Tok::Float(2500.0),
                Tok::Int(1),
                Tok::DotDot,
                Tok::Int(3),
                Tok::Dur(vec![(1, "h".into()), (30, "min".into())]),
                Tok::Clock("9:30".into()),
                Tok::Clock("5pm".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn units_and_strings() {
        assert_eq!(
            toks(r#"5km to mi "a{f("}")}" r"\d+" # c"#),
            vec![Tok::Int(5), Tok::Ident("km".into()), Tok::To, Tok::Ident("mi".into()), Tok::Str(r#"a{f("}")}"#.into()), Tok::Regex(r"\d+".into()), Tok::Eof]
        );
    }
}
