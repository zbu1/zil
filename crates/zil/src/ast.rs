//! Syntax tree produced by the parser and walked by the interpreter.

use crate::lexer::Span;
use num_bigint::BigInt;
use regex::Regex;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Nil,
    Bool(bool),
    /// Value and the base it was written in.
    Int(i64, u32),
    Big(Rc<BigInt>, u32),
    Float(f64),
    /// Exact decimal literal: `0.1`.
    Dec(Rc<num_rational::BigRational>),
    Str(Rc<str>),
    Regex(Rc<Regex>),
    Ident(String),
    List(Vec<Expr>),
    Map(Vec<(String, Expr)>),
    /// Number literal with a unit attached: `5 km`, `60 km/h`.
    Qty(Box<Expr>, UnitSpec),
    /// `20%`, which is 0.2; `x + 20%` is sugar for `x * (1 + 20%)`.
    Percent(Box<Expr>),
    /// `3i`: the number times the imaginary unit.
    Imag(Box<Expr>),
    /// `"{x:.2f}"`: an interpolated value and its format spec.
    Format(Box<Expr>, Rc<str>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Chained comparison `a < b <= c`: each operand evaluated once, stops at the first false.
    Chain(Box<Expr>, Vec<(BinOp, Expr)>),
    To(Box<Expr>, Target),
    Assign(Box<Expr>, Box<Expr>),
    /// `[a, [b, _]] = xs`, or `a, b = 3, 4`.
    Unpack(Pat, Box<Expr>),
    /// `*xs` in a call's arguments or a list: the items of xs, spread in place.
    Spread(Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>, bool),
    Fn(Rc<FnDef>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    While(Box<Expr>, Box<Expr>),
    For(Pat, Box<Expr>, Box<Expr>),
    Block(Vec<Expr>),
    Return(Option<Box<Expr>>),
    /// `unit pizza` or `unit slice = pizza / 8`.
    UnitDef(String, Option<Box<Expr>>),
    Break,
    Continue,
}

/// What a value binds to: a name, or `[a, [b, c]]` to unpack a list, where `_` skips an item and `*rest` takes what's left.
#[derive(Debug, Clone)]
pub enum Pat {
    Name(String),
    Skip,
    List(Vec<Pat>, Span),
    /// `*rest` inside a list pattern.
    Rest(Box<Pat>),
}

/// Unit names with powers, e.g. `km/h^2` = [("km", 1), ("h", -2)].
pub type UnitSpec = Vec<(String, i8)>;

#[derive(Debug, Clone)]
pub enum Target {
    Unit(UnitSpec),
    /// Several units, largest first: `to d h min`, `to ft in`.
    Units(Vec<UnitSpec>),
    /// A keyword some module registered, with an optional int argument: `hex`, `hex(32)`, `UTC`.
    Named(String, Option<i64>),
    /// A string such as a time zone name: `to "Asia/Tokyo"`.
    Str(String),
    /// `to "{zone}"` or `to (zone)`: evaluated to a string, then used like `Str`.
    Expr(Box<Expr>),
}

/// How an integer displays: `base`, and `width` bits of two's complement (0 = plain signed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Radix {
    pub base: u32,
    pub width: u32,
}

impl Radix {
    pub const DEC: Radix = Radix { base: 10, width: 0 };
}

#[derive(Debug)]
pub struct FnDef {
    pub params: Vec<Pat>,
    pub body: Expr,
    /// The fn as written, `|x| x * 2`: parsed again to restore a saved session.
    pub src: Rc<str>,
}

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
    /// Postfix `n!`.
    Factorial,
    /// Postfix `n!!`.
    DoubleFactorial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    /// `x ± e`: x with uncertainty e.
    PlusMinus,
    Mul,
    Div,
    IntDiv,
    Rem,
    /// `a mod m`: a modular value.
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Range,
    RangeIncl,
    In,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
