//! Recursive-descent parser: tokens into statements of `Expr`, including string `{}` interpolation.

use crate::Error;
use crate::ast::{BinOp, Expr, ExprKind, FnDef, Param, Pat, Target, UnOp, UnitSpec};
use crate::lexer::{Span, Tok, lex_at, lex_loose};
use num_rational::BigRational;
use std::rc::Rc;

mod command;

pub(crate) use command::{Mark, marks, quote};

type PResult<T> = Result<T, Error>;
/// Command lines' spans, each with its canonical call or why its words don't parse.
type Commands = Vec<(Span, PResult<String>)>;

pub fn parse(src: &str) -> PResult<Vec<Expr>> {
    Ok(program(src)?.0)
}

/// `src` with each command line rewritten as the call it means: `ls -la src` is `ls("src", long: true, all: true)`.
pub fn desugar(src: &str) -> PResult<String> {
    let mut out = String::new();
    let mut at = 0;
    for (span, call) in program(src)?.1 {
        out += &src[at..span.start];
        out += &call?;
        at = span.end;
    }
    Ok(out + &src[at..])
}

/// `src` with its grouping made visible: each operand of an operator that is itself an operation sits in parentheses,
/// so `5 m/2 s` is `(5 m) / (2 s)`. Other forms show as written.
pub fn grouped(src: &str) -> PResult<String> {
    Ok(program(src)?.0.iter().map(|e| group(src, e, false)).collect::<Vec<_>>().join(
        "
",
    ))
}

fn group(src: &str, e: &Expr, nested: bool) -> String {
    use ExprKind::*;
    let wrap = |s: String| if nested { format!("({s})") } else { s };
    let between = |a: &Expr, b: &Expr| src[a.span.end..b.span.start].trim_matches(|c: char| c.is_whitespace() || c == '(' || c == ')').to_string();
    match &e.kind {
        Binary(_, a, b) | Each(_, a, b) => wrap(format!("{} {} {}", group(src, a, true), between(a, b), group(src, b, true))),
        Chain(a, rest) => {
            let mut s = group(src, a, true);
            let mut prev = &**a;
            for (_, x) in rest {
                s += &format!(" {} {}", between(prev, x), group(src, x, true));
                prev = x;
            }
            wrap(s)
        }
        Unary(UnOp::Neg | UnOp::Not | UnOp::BitNot, a) => wrap(format!("{}{}", src[e.span.start..a.span.start].trim_end_matches('('), group(src, a, true))),
        Qty(..) | Juxt(..) | Percent(_) | Imag(_) | To(..) => wrap(src[e.span.clone()].to_string()),
        _ => src[e.span.clone()].to_string(),
    }
}

/// The statements of `src`, and each command line's span with its canonical call.
fn program(src: &str) -> PResult<(Vec<Expr>, Commands)> {
    let mut p = Parser {
        src,
        declared: vec![],
        toks: lex_loose(src)?,
        pos: 0,
        prev_end: 0,
        holes: 0,
        ph: 0,
        ph_at: None,
        in_pipe: false,
        bare: 0,
        lone: 0,
        args_base: 0,
        in_args: 0,
        no_ph: false,
        depth: 0,
        stop: usize::MAX,
        stmt_start: usize::MAX,
        commands: vec![],
    };
    let mut stmts = Vec::new();
    p.skip_nl();
    while *p.peek() != Tok::Eof {
        stmts.push(p.stmt()?);
        if *p.peek() != Tok::Eof {
            p.expect(Tok::Newline, "a newline or `;` between statements")?;
        }
        p.skip_nl();
    }
    Ok((stmts, p.commands))
}

struct Parser<'a> {
    /// The whole source; spans index into it, interpolations included.
    src: &'a str,
    /// Types this source has declared so far, which later params may name.
    declared: Vec<String>,
    toks: Vec<(Tok, Span)>,
    pos: usize,
    prev_end: usize,
    /// `_` placeholders seen so far, so `x |> f(_, 2)` knows to bind `_` instead of inserting `x`.
    holes: usize,
    /// `_` placeholders in the nearest enclosing parentheses or call argument, each renamed `_1`, `_2`…, and the first one's span.
    ph: usize,
    ph_at: Option<Span>,
    /// Parsing a statement again with `_` as a plain name.
    no_ph: bool,
    /// Inside a pipe's right side, where `_` keeps its pipe meaning.
    in_pipe: bool,
    /// Set by `list` for the call it just parsed: how many arguments were a bare `_`.
    bare: usize,
    /// Lone `_` arguments handed to an enclosing argument's placeholder scope.
    lone: usize,
    /// The `in_args` depth where the innermost fn body began.
    args_base: usize,
    /// How many call arguments the parser is inside.
    in_args: usize,
    /// A token position where expressions end, like a closer: the `|` of a set-builder.
    stop: usize,
    /// How deep the expression being built is nested, capped at `MAX_NEST`.
    depth: usize,
    /// Token position of the current statement's first token, where a command name starts command mode.
    stmt_start: usize,
    /// For `desugar`.
    commands: Commands,
}

/// Deeper trees would overflow the stack evaluating them: the browser's ~1 MB fits about 300 levels.
const MAX_NEST: usize = 256;

/// (left, right) binding power of infix operators; higher binds tighter.
fn infix_bp(t: &Tok) -> Option<(u8, u8)> {
    Some(match t {
        Tok::Assign | Tok::OpAssign(_) => (ASSIGN_BP, 1),
        Tok::Pipe => (4, 5),
        Tok::Or => (6, 7),
        Tok::And => (8, 9),
        Tok::Eq | Tok::Ne => (10, 11),
        Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::In => (12, 13),
        Tok::DotDot | Tok::DotDotEq => (14, 15),
        Tok::Bar => (16, 17),
        Tok::Caret => (18, 19),
        Tok::Amp => (20, 21),
        Tok::Shl | Tok::Shr => (22, 23),
        Tok::Plus | Tok::Minus | Tok::PlusMinus | Tok::Each(BinOp::Add | BinOp::Sub) => (24, 25),
        Tok::Star | Tok::Slash | Tok::SlashSlash | Tok::Percent | Tok::Of | Tok::Mod | Tok::Cdot | Tok::Times | Tok::Each(BinOp::Mul | BinOp::Div) => (26, 27),
        Tok::StarStar | Tok::Each(_) => (30, 29),
        _ => return None,
    })
}
const ASSIGN_BP: u8 = 2;
const TO_BP: u8 = 3;
const PREFIX_BP: u8 = 28;
const POSTFIX_BP: u8 = 32;

pub(crate) fn binop(t: &Tok) -> BinOp {
    match t {
        Tok::Or => BinOp::Or,
        Tok::And => BinOp::And,
        Tok::Eq => BinOp::Eq,
        Tok::Ne => BinOp::Ne,
        Tok::Lt => BinOp::Lt,
        Tok::Le => BinOp::Le,
        Tok::Gt => BinOp::Gt,
        Tok::Ge => BinOp::Ge,
        Tok::DotDot => BinOp::Range,
        Tok::DotDotEq => BinOp::RangeIncl,
        Tok::In => BinOp::In,
        Tok::Plus => BinOp::Add,
        Tok::Minus => BinOp::Sub,
        Tok::PlusMinus => BinOp::PlusMinus,
        Tok::Star | Tok::Of => BinOp::Mul,
        Tok::Slash => BinOp::Div,
        Tok::SlashSlash => BinOp::IntDiv,
        Tok::Percent => BinOp::Rem,
        Tok::Mod => BinOp::Mod,
        Tok::StarStar => BinOp::Pow,
        Tok::Caret => BinOp::BitXor,
        Tok::Amp => BinOp::BitAnd,
        Tok::Bar => BinOp::BitOr,
        Tok::Shl => BinOp::Shl,
        Tok::Shr => BinOp::Shr,
        Tok::Cdot => BinOp::Dot,
        Tok::Times => BinOp::Cross,
        _ => unreachable!(),
    }
}

impl Parser<'_> {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].0
    }

    fn span(&self) -> Span {
        self.toks[self.pos].1.clone()
    }

    fn bump(&mut self) -> Tok {
        let (t, s) = self.toks[self.pos].clone();
        if t != Tok::Eof {
            self.pos += 1;
        }
        self.prev_end = s.end;
        t
    }

    /// No whitespace between the previous token and the current one.
    /// `xor` after an operand is `^`, unless it's called: `xor(b, key)`.
    fn word_xor(&self) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == "xor") && !(*self.peek_at(1) == Tok::LParen && self.toks[self.pos].1.end == self.toks[self.pos + 1].1.start)
    }

    /// Whether token `i` can end an operand.
    fn ends_operand(&self, i: usize) -> bool {
        use Tok::*;
        match self.toks[i].0 {
            Ident(_) | Dur(_) | Angle(_) | Clock(_) | Sup(_) | Str(_) | Bytes(_) | Regex(_) | RParen | RBracket | RBrace | True | False | Nil => true,
            // `5!` and `20%`
            Bang | Percent => i > 0 && self.toks[i - 1].1.end == self.toks[i].1.start,
            ref t => is_number(t),
        }
    }

    /// Whether token `i` is the word `x` as the cross product: an operand on each side and a space between. `a x - 1` is `a x`, minus 1.
    fn cross_at(&self, i: usize) -> bool {
        let spaced = |i: usize| self.toks[i].1.end != self.toks[i + 1].1.start;
        matches!(&self.toks[i].0, Tok::Ident(s) if s == "x")
            && i > 0
            && self.ends_operand(i - 1)
            && spaced(i - 1)
            && spaced(i)
            && match self.toks[i + 1].0 {
                Tok::Ident(_) | Tok::LParen | Tok::LBracket => true,
                Tok::Minus => !spaced(i + 1),
                ref t => is_number(t),
            }
    }

    /// At a name after `match`: whether a `{` ends the line's expression, so `if match > 0 {` keeps `match` a plain name.
    fn is_match(&self) -> bool {
        if !starts_operand(self.peek()) || matches!(self.peek(), Tok::Minus | Tok::Bar | Tok::Or) || (*self.peek() == Tok::LParen && self.touching()) {
            return false;
        }
        // `if match { ... }` is the name; `match {x: 1} { ... }` opens with a map.
        if *self.peek() == Tok::LBrace {
            return *self.peek_at(2) == Tok::Colon;
        }
        let mut depth = 0;
        for (t, _) in &self.toks[self.pos..] {
            match t {
                Tok::LBrace if depth == 0 => return true,
                Tok::LParen | Tok::LBracket | Tok::LBrace => depth += 1,
                Tok::RParen | Tok::RBracket | Tok::RBrace => depth -= 1,
                Tok::Newline if depth == 0 => return false,
                Tok::Eof => return false,
                _ => {}
            }
            if depth < 0 {
                return false;
            }
        }
        false
    }

    /// How far ahead the token after the `)` matching the `(` at offset `i` is.
    fn past_parens(&self, mut i: usize) -> Option<usize> {
        let mut depth = 0;
        loop {
            match self.peek_at(i) {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                Tok::Eof => return None,
                _ => {}
            }
            i += 1;
        }
    }

    /// Whether the next tokens, after `op`, are an operator touching `(params) =`.
    fn is_op_def(&mut self) -> bool {
        if op_symbol(self.peek()).is_none() {
            return false;
        }
        self.pos += 1;
        let def = self.is_def();
        self.pos -= 1;
        def
    }

    /// Whether the next tokens, after a name, are `(params) =` or `(params) -> types =`.
    fn is_def(&self) -> bool {
        if *self.peek() != Tok::LParen || !self.touching() {
            return false;
        }
        let Some(mut i) = self.past_parens(0) else { return false };
        if *self.peek_at(i) == Tok::Arrow {
            while matches!(self.peek_at(i + 1), Tok::Ident(_) | Tok::Fn | Tok::Nil | Tok::Mod | Tok::Bar) {
                i += 1;
            }
            i += 1;
        }
        *self.peek_at(i) == Tok::Assign
    }

    fn touching(&self) -> bool {
        self.pos > 0 && self.toks[self.pos - 1].1.end == self.toks[self.pos].1.start
    }

    fn expect(&mut self, t: Tok, what: &str) -> PResult<()> {
        if *self.peek() == t {
            self.bump();
            Ok(())
        } else {
            Err(self.unexpected(self.pos, what))
        }
    }

    /// `expected {what}` at token `at`: "after `+`" when the input or line ran out there, else "found `x`".
    fn unexpected(&self, at: usize, what: &str) -> Error {
        let (tok, span) = &self.toks[at];
        if *tok == Tok::Bad {
            return Error::new(format!("unexpected character `{}`", &self.src[span.clone()]), span.clone());
        }
        if matches!(tok, Tok::Eof | Tok::Newline) && at > 0 {
            let prev = self.toks[at - 1].1.clone();
            return Error::new(format!("expected {what} after `{}`", &self.src[prev.clone()]), prev);
        }
        let found = match tok {
            Tok::Eof => "nothing".to_string(),
            Tok::Newline if &self.src[span.clone()] == "\n" => "a newline".to_string(),
            _ => format!("`{}`", &self.src[span.clone()]),
        };
        Error::new(format!("expected {what}, found {found}"), span.clone())
    }

    /// The `close` matching the opener at `open`; "unclosed" when the input ends first.
    fn close(&mut self, close: Tok, open: &Span) -> PResult<()> {
        if *self.peek() == close {
            self.bump();
            return Ok(());
        }
        let c = match close {
            Tok::RParen => ")",
            Tok::RBracket => "]",
            _ => "}",
        };
        if *self.peek() == Tok::Eof {
            let msg = format!("unclosed `{}`", &self.src[open.clone()]);
            return Err(Error::new(msg, open.clone()).label(open.clone(), "opened here").label(self.span(), format!("expected `{c}` here")));
        }
        Err(self.unexpected(self.pos, &format!("`{c}`")).label(open.clone(), "opened here"))
    }

    fn ident(&mut self) -> PResult<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected(self.pos, "a name")),
        }
    }

    fn skip_nl(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }

    fn past_nl(&self) -> usize {
        let mut i = self.pos;
        while self.toks[i].0 == Tok::Newline {
            i += 1;
        }
        i
    }

    /// Token after any run of newlines.
    fn peek_past_nl(&self) -> &Tok {
        &self.toks[self.past_nl()].0
    }

    /// Whether the next line continues this expression, by starting with `|>` or a `.` touching a name.
    fn continues(&self) -> bool {
        let i = self.past_nl();
        match self.toks[i].0 {
            Tok::Pipe => true,
            Tok::Dot => self.toks[i].1.end == self.toks[i + 1].1.start,
            _ => false,
        }
    }

    /// At a `.` after an operand: whether it's the dot product rather than a field. A space on one side only is an error.
    fn dot_product(&self) -> PResult<bool> {
        let left = self.touching() || self.toks[self.pos - 1].0 == Tok::Newline;
        let right = self.toks[self.pos].1.end == self.toks[self.pos + 1].1.start;
        if left != right {
            return Err(Error::new("a `.` needs a space on both sides or neither", self.span()).help("`a.b` reads a field and `a . b` is the dot product"));
        }
        Ok(!left || matches!(self.peek_at(1), Tok::LBracket | Tok::LParen))
    }

    fn mk(&self, kind: ExprKind, start: usize) -> Expr {
        Expr { kind, span: start..self.prev_end }
    }

    /// One more level of nesting, or an error past `MAX_NEST`.
    fn nest(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_NEST {
            return Err(Error::new("nested too deeply", self.span())
                .help(format!("expressions can nest at most {MAX_NEST} levels, long `a + b + ...` chains included")));
        }
        Ok(())
    }

    /// An expression, or `a, b = 3, 4` / `first, *rest = xs`: unpacking without brackets.
    fn stmt(&mut self) -> PResult<Expr> {
        let (outer, pos, commands) = (self.ph_enter(), self.pos, self.commands.len());
        let e = self.stmt_inner()?;
        let (n, _) = self.ph_leave(outer);
        if n == 0 || matches!(e.kind, ExprKind::Unpack(..)) {
            return Ok(e);
        }
        // `_` outside parentheses is a plain variable (the REPL's last result), so parse the statement again without placeholders.
        (self.pos, self.holes, self.in_pipe) = (pos, 0, false);
        self.commands.truncate(commands);
        self.no_ph = true;
        let e = self.stmt_inner();
        self.no_ph = false;
        e
    }

    fn stmt_inner(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        self.stmt_start = self.pos;
        let first = if *self.peek() == Tok::Star { self.item(ASSIGN_BP + 1)? } else { self.expr(0)? };
        if *self.peek() != Tok::Comma && !matches!(first.kind, ExprKind::Spread(_)) {
            return Ok(first);
        }
        if matches!(first.kind, ExprKind::Assign(..) | ExprKind::Unpack(..)) {
            return Err(Error::new("a list needs brackets here", first.span).help("write `x = [1, 2]`; bare commas only unpack: `a, b = 1, 2`"));
        }
        let mut names = vec![first];
        while *self.peek() == Tok::Comma {
            self.bump();
            names.push(self.item(ASSIGN_BP + 1)?);
        }
        let pat = to_pat(&self.mk(ExprKind::List(names), start))?;
        self.expect(Tok::Assign, "`=`")?;
        self.skip_nl();
        let rhs_start = self.span().start;
        let mut values = vec![self.item(ASSIGN_BP + 1)?];
        while *self.peek() == Tok::Comma {
            self.bump();
            values.push(self.item(ASSIGN_BP + 1)?);
        }
        let rhs = match values.pop() {
            Some(v) if values.is_empty() && !matches!(v.kind, ExprKind::Spread(_)) => v,
            last => {
                values.extend(last);
                self.mk(ExprKind::List(values), rhs_start)
            }
        };
        Ok(self.mk(ExprKind::Unpack(pat, Box::new(rhs)), start))
    }

    /// A call argument or list item, where `*xs` spreads xs.
    fn item(&mut self, min_bp: u8) -> PResult<Expr> {
        let start = self.span().start;
        if *self.peek() != Tok::Star {
            return self.expr(min_bp);
        }
        self.bump();
        let e = self.expr(min_bp)?;
        Ok(self.mk(ExprKind::Spread(Box::new(e)), start))
    }

    fn expr(&mut self, min_bp: u8) -> PResult<Expr> {
        let depth = self.depth;
        let e = self.expr_chain(min_bp);
        self.depth = depth;
        e
    }

    /// Each operator applied to `lhs` nests it a level deeper.
    fn expr_chain(&mut self, min_bp: u8) -> PResult<Expr> {
        self.nest()?;
        let start = self.span().start;
        let mut lhs = self.prefix()?;
        while self.pos != self.stop {
            if *self.peek() == Tok::Newline && self.continues() {
                self.skip_nl();
            }
            let t = match self.peek() {
                _ if self.word_xor() => Tok::Caret,
                _ if self.cross_at(self.pos) => Tok::Times,
                Tok::Dot if self.dot_product()? => Tok::Cdot,
                Tok::Ellipsis => {
                    let (from, to) = (&self.src[lhs.span.clone()], &self.src[self.toks[self.pos + 1].1.clone()]);
                    return Err(Error::new("`...` is not an operator", self.span()).help(format!("did you mean `{from}..{to}`?")));
                }
                Tok::Dec(_) | Tok::Float(_) if self.touching() && self.src[self.span()].starts_with('.') => {
                    return Err(
                        Error::new("a number can't touch the value before it", self.span()).help("a field name starts with a letter, and `*` multiplies")
                    );
                }
                t => t.clone(),
            };
            if matches!(t, Tok::LParen | Tok::LBracket | Tok::Dot) {
                if POSTFIX_BP < min_bp {
                    break;
                }
                let open = self.span();
                self.bump();
                let kind = match t {
                    Tok::LParen => {
                        let mut args = self.list(Tok::RParen, &open, true)?;
                        let mut bare = std::mem::take(&mut self.bare);
                        // a lone `_` argument is the REPL's last result, not a partial application
                        if bare == 1 && self.in_args == 0 {
                            bare = 0;
                            for a in args.iter_mut().filter(|a| matches!(&a.kind, ExprKind::Ident(n) if n == "_1") && &self.src[a.span.clone()] == "_") {
                                a.kind = ExprKind::Ident("_".into());
                            }
                        }
                        let call = self.mk(ExprKind::Call(Box::new(lhs), args), start);
                        if bare > 0 {
                            lhs = self.wrap_ph(call, bare);
                            self.nest()?;
                            continue;
                        }
                        call.kind
                    }
                    Tok::LBracket => self.index(lhs, &open)?,
                    _ => ExprKind::Field(Box::new(lhs), self.ident()?),
                };
                lhs = self.mk(kind, start);
                self.nest()?;
                continue;
            }
            // `(a + b)x`, `(1 + 2)sin(x)`: multiplication.
            if matches!(t, Tok::Ident(_)) && self.touching() && self.toks[self.pos - 1].0 == Tok::RParen {
                if POSTFIX_BP < min_bp {
                    break;
                }
                let rhs = self.expr(POSTFIX_BP)?;
                lhs = self.mk(ExprKind::Binary(BinOp::Mul, Box::new(lhs), Box::new(rhs)), start);
                self.nest()?;
                continue;
            }
            // `5!` and `5!!`, touching the operand.
            if t == Tok::Bang && self.touching() {
                if POSTFIX_BP < min_bp {
                    break;
                }
                self.bump();
                let op = if *self.peek() == Tok::Bang && self.touching() {
                    self.bump();
                    UnOp::DoubleFactorial
                } else {
                    UnOp::Factorial
                };
                lhs = self.mk(ExprKind::Unary(op, Box::new(lhs)), start);
                self.nest()?;
                continue;
            }
            if t == Tok::To {
                if TO_BP < min_bp {
                    break;
                }
                self.bump();
                let target = self.target()?;
                lhs = self.mk(ExprKind::To(Box::new(lhs), target), start);
                self.nest()?;
                continue;
            }
            let Some((lbp, rbp)) = infix_bp(&t) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            self.bump();
            self.skip_nl();
            let outer_holes = (t == Tok::Pipe).then(|| std::mem::take(&mut self.holes));
            let was_pipe = self.in_pipe;
            self.in_pipe |= t == Tok::Pipe;
            let rhs = self.expr(rbp)?;
            self.in_pipe = was_pipe;
            let hole = outer_holes.is_some_and(|outer| std::mem::replace(&mut self.holes, outer) > 0);
            let kind = match t {
                Tok::Assign if let ExprKind::List(_) = lhs.kind => ExprKind::Unpack(to_pat(&lhs)?, Box::new(rhs)),
                Tok::Assign | Tok::OpAssign(_) if !matches!(lhs.kind, ExprKind::Ident(_) | ExprKind::Field(..) | ExprKind::Index(..)) => {
                    return Err(Error::new("cannot assign to this", lhs.span)
                        .help("assign to a name, field, index or list of names: `x = 1`, `m.k = 1`, `xs[0] = 1`, `[a, b] = xs`"));
                }
                Tok::Assign => ExprKind::Assign(Box::new(lhs), Box::new(rhs)),
                // ponytail: `xs[f()] += 1` evaluates `xs` and `f()` twice; desugar to a temp if side effects there ever matter.
                Tok::OpAssign(op) => {
                    let read = if matches!(lhs.kind, ExprKind::Index(..)) {
                        let span = lhs.span.clone();
                        Expr { kind: ExprKind::Lenient(Box::new(lhs.clone())), span }
                    } else {
                        lhs.clone()
                    };
                    let value = self.mk(arith(op, read, rhs), start);
                    ExprKind::Assign(Box::new(lhs), Box::new(value))
                }
                // `x |> f(_, 2)` => `(|_| f(_, 2))(x)`
                Tok::Pipe if hole => {
                    let span = rhs.span.clone();
                    // Called on the spot and never stored, so it's never saved and its `src` never parsed again.
                    let src = format!("|_| {}", &self.src[span.clone()]).into();
                    let f = Expr { kind: ExprKind::Fn(Rc::new(FnDef { params: vec![Pat::Name("_".into()).into()], ret: vec![], body: rhs, src })), span };
                    ExprKind::Call(Box::new(f), vec![lhs])
                }
                // `x |> f(a)` => `f(x, a)`; `x |> f` => `f(x)`
                Tok::Pipe => match rhs.kind {
                    ExprKind::Call(f, mut args) => {
                        args.insert(0, lhs);
                        ExprKind::Call(f, args)
                    }
                    _ => ExprKind::Call(Box::new(rhs), vec![lhs]),
                },
                // `a < b < c` => Chain(a, [(<, b), (<, c)])
                Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge => match lhs.kind {
                    ExprKind::Binary(op @ (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge), a, b) => ExprKind::Chain(a, vec![(op, *b), (binop(&t), rhs)]),
                    ExprKind::Chain(a, mut rest) => {
                        rest.push((binop(&t), rhs));
                        ExprKind::Chain(a, rest)
                    }
                    _ => ExprKind::Binary(binop(&t), Box::new(lhs), Box::new(rhs)),
                },
                Tok::Each(op) => ExprKind::Each(op, Box::new(lhs), Box::new(rhs)),
                _ => arith(binop(&t), lhs, rhs),
            };
            lhs = self.mk(kind, start);
            self.nest()?;
        }
        Ok(lhs)
    }

    fn prefix(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        let tok_span = self.span();
        let at = self.pos;
        if matches!(self.peek(), Tok::Ident(s) if s == "_") {
            self.holes += 1;
        }
        // `!prog` at a statement's start or after `=` runs a program; `!(x)` is always not.
        if *self.peek() == Tok::Bang && !matches!(self.peek_at(1), Tok::LParen | Tok::Bang) {
            let bare = self.pos == self.stmt_start;
            if bare || self.pos.checked_sub(1).is_some_and(|p| self.toks[p].0 == Tok::Assign) {
                return self.external(bare);
            }
        }
        if self.pos == self.stmt_start
            && let Some(e) = self.command()?
        {
            return Ok(e);
        }
        if !self.in_pipe && !self.no_ph && matches!(self.peek(), Tok::Ident(s) if s == "_") {
            self.bump();
            self.ph += 1;
            self.ph_at.get_or_insert(tok_span);
            return Ok(self.mk(ExprKind::Ident(format!("_{}", self.ph)), start));
        }
        let kind = match self.bump() {
            Tok::Nil => ExprKind::Nil,
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            // `1h30m` is `30 min + 1 h`, smallest unit first so the sum stays whole: `90 min`.
            Tok::Dur(parts) => {
                let qty = |(n, u): (i64, String)| self.mk(ExprKind::Qty(Box::new(self.mk(ExprKind::Int(n, 10), start)), vec![(u, 1)]), start);
                let mut it = parts.into_iter().rev().map(qty);
                let first = it.next().expect("lexer gives two or more parts");
                return Ok(it.fold(first, |a, b| self.mk(ExprKind::Binary(BinOp::Add, Box::new(a), Box::new(b)), start)));
            }
            // `52°20′24″` is `24 arcsec + 20 arcmin + 52 deg`; `20°C` is 20 degrees Celsius.
            Tok::Angle(parts) => {
                let mut parts: Vec<_> = parts
                    .into_iter()
                    .map(|(n, c)| {
                        (
                            n,
                            match c {
                                '°' => "deg",
                                '′' => "arcmin",
                                _ => "arcsec",
                            },
                        )
                    })
                    .collect();
                if let [(_, "deg")] = parts[..]
                    && matches!(self.peek(), Tok::Ident(u) if u == "C" || u == "F")
                    && self.touching()
                {
                    let Tok::Ident(u) = self.bump() else { unreachable!() };
                    parts[0].1 = if u == "C" { "C" } else { "F" };
                }
                let qty = |(n, u): (BigRational, &str)| {
                    let n = if n.is_integer() { ExprKind::Int(n.to_integer().to_string().parse().unwrap_or(i64::MAX), 10) } else { ExprKind::Dec(Rc::new(n)) };
                    self.mk(ExprKind::Qty(Box::new(self.mk(n, start)), vec![(u.to_string(), 1)]), start)
                };
                let mut it = parts.into_iter().rev().map(qty);
                let first = it.next().expect("lexer gives one or more parts");
                return Ok(it.fold(first, |a, b| self.mk(ExprKind::Binary(BinOp::Add, Box::new(a), Box::new(b)), start)));
            }
            // `9:30` is `"9:30" to date`: today at that time.
            Tok::Clock(s) => ExprKind::To(Box::new(self.mk(ExprKind::Str(s.into()), start)), Target::Named("date".into(), None)),
            t @ (Tok::Int(_) | Tok::Based(_) | Tok::Big(_) | Tok::Float(_) | Tok::Dec(_)) => {
                let kind = match t {
                    Tok::Int(n) => ExprKind::Int(n, 10),
                    Tok::Based((n, b)) => ExprKind::Int(n, b),
                    Tok::Big((n, b)) => ExprKind::Big(Rc::new(n), b),
                    Tok::Float(n) => ExprKind::Float(n),
                    // `2.0` asks for a float; `2.5` is exact.
                    Tok::Dec(r) if r.is_integer() => ExprKind::Float(r.to_integer().to_string().parse().unwrap_or(f64::NAN)),
                    Tok::Dec(r) => ExprKind::Dec(Rc::new(r)),
                    _ => unreachable!(),
                };
                // `3i`: imaginary, only when the `i` touches the number.
                if matches!(self.peek(), Tok::Ident(s) if s == "i") && self.touching() {
                    self.bump();
                    ExprKind::Imag(Box::new(self.mk(kind, start)))
                // `20%` unless an operand follows: `20 % 3` and `7%3` stay remainders.
                } else if *self.peek() == Tok::Percent && self.touching() && (self.pos + 1 == self.stop || !starts_operand(self.peek_at(1))) {
                    self.bump();
                    ExprKind::Percent(Box::new(self.mk(kind, start)))
                // `2sin(50)`: the number times the call.
                } else if matches!(self.peek(), Tok::Ident(_))
                    && self.touching()
                    && *self.peek_at(1) == Tok::LParen
                    && self.toks[self.pos].1.end == self.toks[self.pos + 1].1.start
                {
                    let num = self.mk(kind, start);
                    ExprKind::Binary(BinOp::Mul, Box::new(num), Box::new(self.expr(POSTFIX_BP)?))
                } else if self.unit_next() {
                    let num = self.mk(kind, start);
                    let touching = matches!(self.peek(), Tok::Ident(_)) && self.touching();
                    let spec = self.unit_spec(true)?;
                    let qty = if touching { ExprKind::Juxt(Box::new(num), spec) } else { ExprKind::Qty(Box::new(num), spec) };
                    // `5 ft 11 in`, `1 h 30 min`: adjacent quantities add, as one literal.
                    if self.pos != self.stop
                        && matches!(self.peek(), Tok::Int(_) | Tok::Float(_) | Tok::Dec(_))
                        && matches!(self.peek_at(1), Tok::Ident(_) | Tok::In)
                    {
                        let lhs = self.mk(qty, start);
                        ExprKind::Binary(BinOp::Add, Box::new(lhs), Box::new(self.prefix()?))
                    } else {
                        qty
                    }
                } else {
                    kind
                }
            }
            Tok::Str(raw) => self.string(&raw, tok_span.start + 1)?,
            Tok::Bytes(raw) => ExprKind::Bytes(byte_string(&raw, tok_span.start + 2)?.into()),
            Tok::Regex(src) => match crate::state::regex(&src) {
                Ok(re) => ExprKind::Regex(Rc::new(re)),
                Err(e) => {
                    let why = e.to_string().lines().last().unwrap_or_default().trim_start_matches("error: ").to_string();
                    return Err(Error::new("invalid regex", tok_span).note(why));
                }
            },
            // `unit pizza`, `unit slice = pizza / 8`; `unit` stays usable as a variable otherwise.
            Tok::Ident(s) if s == "unit" && matches!((self.peek(), self.peek_at(1)), (Tok::Ident(_), Tok::Assign | Tok::Newline | Tok::Eof | Tok::RBrace)) => {
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                let value = if *self.peek() == Tok::Assign {
                    self.bump();
                    Some(Box::new(self.expr(0)?))
                } else {
                    None
                };
                ExprKind::UnitDef(name, value)
            }
            // `type vec2 = {x: num, y: num = 0}`; `type` stays usable as a variable otherwise.
            Tok::Ident(s) if s == "type" && matches!((self.peek(), self.peek_at(1), self.peek_at(2)), (Tok::Ident(_), Tok::Assign, Tok::LBrace)) => {
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                self.bump();
                self.bump();
                self.skip_nl();
                let fields = self.params(Tok::RBrace)?;
                self.skip_nl();
                self.expect(Tok::RBrace, "`}`")?;
                if fields.iter().any(|p| !matches!(p.pat, Pat::Name(_))) {
                    return Err(Error::new("a type's fields are plain names", start..self.prev_end));
                }
                self.declared.push(name.clone());
                ExprKind::TypeDef(name, Rc::new(fields))
            }
            // `del unit sprint`; `del unit` stays a variable.
            Tok::Ident(s) if s == "del" && matches!((self.peek(), self.peek_at(1)), (Tok::Ident(u), Tok::Ident(_)) if u == "unit") => {
                self.bump();
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                ExprKind::DelUnit(name)
            }
            // `del area(c: circle)`: the parens end the statement, which no call of `del` does.
            Tok::Ident(s)
                if s == "del"
                    && matches!((self.peek(), self.peek_at(1)), (Tok::Ident(_), Tok::LParen))
                    && self.past_parens(1).is_some_and(|i| matches!(self.peek_at(i), Tok::Newline | Tok::Eof | Tok::RBrace)) =>
            {
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                self.bump();
                let params = self.params(Tok::RParen)?;
                self.expect(Tok::RParen, "`)`")?;
                ExprKind::DelFn(name, Rc::new(params))
            }
            // `del x, y`; `del(m, k)` and `m.del(k)` stay the map and set functions.
            Tok::Ident(s) if s == "del" && matches!((self.peek(), self.peek_at(1)), (Tok::Ident(_), Tok::Comma | Tok::Newline | Tok::Eof | Tok::RBrace)) => {
                let mut names = vec![];
                while let Tok::Ident(n) = self.peek().clone() {
                    self.bump();
                    names.push(n);
                    if *self.peek() != Tok::Comma {
                        break;
                    }
                    self.bump();
                }
                ExprKind::Del(names)
            }
            // `match v { ... }`; `match(s, re)` and `s.match(re)` stay the text function.
            Tok::Ident(s) if s == "match" && self.is_match() => {
                let value = self.expr(0)?;
                ExprKind::Match(Box::new(value), self.match_arms()?)
            }
            // `op alias comb = choose`; `op` stays usable as a name otherwise.
            Tok::Ident(s)
                if s == "op"
                    && matches!((self.peek(), self.peek_at(1), self.peek_at(2), self.peek_at(3)), (Tok::Ident(a), Tok::Ident(_), Tok::Assign, Tok::Ident(_)) if a == "alias") =>
            {
                self.bump();
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                self.bump();
                let Tok::Ident(target) = self.bump() else { unreachable!() };
                ExprKind::OpAlias(name, target)
            }
            // `op +(a: vec2, b: vec2) = ...`: an overload of the operator.
            Tok::Ident(s) if s == "op" && self.is_op_def() => self.op_def(start)?,
            // `f(x: int) = body` or `f(x) -> int = body`: a named, overloadable function.
            Tok::Ident(s) if self.is_def() => self.def(s, start)?,
            // `sin 50`, `sqrt 2pi`: a paren-less call.
            Tok::Ident(s)
                if !self.touching()
                    && self.pos != self.stop
                    && matches!(self.peek(), Tok::Int(_) | Tok::Based(_) | Tok::Big(_) | Tok::Float(_) | Tok::Dec(_) | Tok::Str(_)) =>
            {
                let f = self.mk(ExprKind::Ident(s), start);
                ExprKind::Apply(Box::new(f), Box::new(self.expr(POSTFIX_BP)?))
            }
            // `a m`: a variable holding a number, given a unit; `sin x` when `a` is a function.
            Tok::Ident(s) if self.unit_next() => {
                let var = self.mk(ExprKind::Ident(s), start);
                ExprKind::Qty(Box::new(var), self.unit_spec(true)?)
            }
            Tok::Ident(s) => ExprKind::Ident(s),
            // `$25`, `$25/h`: a number in that currency, optionally per some unit.
            Tok::Currency(code) => {
                let n = self.prefix()?;
                if !matches!(n.kind, ExprKind::Int(..) | ExprKind::Big(..) | ExprKind::Float(_) | ExprKind::Dec(_)) {
                    return Err(Error::new(format!("expected a number after the {code} symbol"), n.span)
                        .help(format!("for a computed amount, write the code: `(x) {code}`")));
                }
                let mut spec = vec![(code.to_string(), 1)];
                self.unit_rest(&mut spec, true)?;
                ExprKind::Qty(Box::new(n), spec)
            }
            Tok::LParen => {
                self.skip_nl();
                let (outer, no_ph) = (self.ph_enter(), std::mem::take(&mut self.no_ph));
                let e = self.expr(0)?;
                self.skip_nl();
                self.close(Tok::RParen, &tok_span)?;
                self.no_ph = no_ph;
                let (n, _) = self.ph_leave(outer);
                return Ok(if n > 0 { self.wrap_ph(e, n) } else { e });
            }
            Tok::LBracket => self.bracket(&tok_span)?,
            Tok::LBrace => {
                self.skip_nl();
                if matches!(self.peek(), Tok::RBrace) || matches!(self.peek_at(1), Tok::Colon) {
                    ExprKind::Map(self.map_body(&tok_span)?)
                } else {
                    self.set_builder(&tok_span)?
                }
            }
            Tok::Minus => ExprKind::Unary(UnOp::Neg, Box::new(self.expr(PREFIX_BP)?)),
            Tok::Bang | Tok::Not => ExprKind::Unary(UnOp::Not, Box::new(self.expr(PREFIX_BP)?)),
            Tok::Tilde => ExprKind::Unary(UnOp::BitNot, Box::new(self.expr(PREFIX_BP)?)),
            Tok::Sqrt => {
                let f = self.mk(ExprKind::Ident("sqrt".into()), start);
                ExprKind::Apply(Box::new(f), Box::new(self.expr(PREFIX_BP)?))
            }
            // `|x, y| body`, or `|| body` with no params.
            t @ (Tok::Bar | Tok::Or) => {
                let params = if t == Tok::Or { Vec::new() } else { self.params(Tok::Bar)? };
                if t == Tok::Bar {
                    self.expect(Tok::Bar, "`|`")?;
                }
                // placeholders inside the body never reach out past the fn
                let base = std::mem::replace(&mut self.args_base, self.in_args);
                let body = self.expr(0);
                self.args_base = base;
                let body = body?;
                ExprKind::Fn(Rc::new(FnDef { params, ret: vec![], body, src: self.src[start..self.prev_end].into() }))
            }
            Tok::Fn => {
                self.expect(Tok::LParen, "`(`")?;
                let params = self.params(Tok::RParen)?;
                self.expect(Tok::RParen, "`)`")?;
                let ret = if *self.peek() == Tok::Arrow {
                    self.bump();
                    self.types()?
                } else {
                    vec![]
                };
                let body = self.block()?;
                ExprKind::Fn(Rc::new(FnDef { params, ret, body, src: self.src[start..self.prev_end].into() }))
            }
            Tok::If => {
                let cond = self.expr(0)?;
                let then = self.block()?;
                let els = if *self.peek_past_nl() == Tok::Else {
                    self.skip_nl();
                    self.bump();
                    Some(Box::new(if *self.peek() == Tok::If { self.expr(0)? } else { self.block()? }))
                } else {
                    None
                };
                ExprKind::If(Box::new(cond), Box::new(then), els)
            }
            Tok::While => {
                let cond = self.expr(0)?;
                ExprKind::While(Box::new(cond), Box::new(self.block()?))
            }
            // `for x in`, `for [i, x] in`, `for i, x in`.
            Tok::For => {
                let at = self.span().start;
                let mut pats = vec![self.pat_item()?];
                while *self.peek() == Tok::Comma {
                    self.bump();
                    pats.push(self.pat_item()?);
                }
                let pat = match pats.pop() {
                    Some(p) if pats.is_empty() && !matches!(p, Pat::Rest(_)) => p,
                    last => {
                        pats.extend(last);
                        list_pat(pats, at..self.prev_end)?
                    }
                };
                self.expect(Tok::In, "`in`")?;
                let iter = self.expr(0)?;
                ExprKind::For(pat, Box::new(iter), Box::new(self.block()?))
            }
            Tok::Break => ExprKind::Break,
            Tok::Continue => ExprKind::Continue,
            Tok::Return => {
                let val = match self.peek() {
                    Tok::Newline | Tok::RBrace | Tok::RParen | Tok::Eof => None,
                    _ => Some(Box::new(self.expr(0)?)),
                };
                ExprKind::Return(val)
            }
            Tok::RParen | Tok::RBracket | Tok::RBrace => return Err(Error::new(format!("unmatched `{}`", &self.src[tok_span.clone()]), tok_span)),
            Tok::Dot => {
                let name = if let Tok::Ident(s) = self.peek() { s.as_str() } else { "name" };
                return Err(Error::new(format!("placeholders are `_`: write `_.{name}`"), tok_span));
            }
            _ => return Err(self.unexpected(at, "an expression")),
        };
        Ok(self.mk(kind, start))
    }

    /// After a number or variable: a unit follows. `in` is inches only when no expression
    /// follows it, so `5 in to cm` is inches but `5 in xs` is membership.
    fn unit_next(&self) -> bool {
        if self.pos == self.stop {
            return false;
        }
        match self.peek() {
            Tok::Ident(_) => !self.word_xor() && !self.cross_at(self.pos),
            Tok::In => !matches!(
                self.peek_at(1),
                Tok::Ident(_)
                    | Tok::Int(_)
                    | Tok::Based(_)
                    | Tok::Big(_)
                    | Tok::Float(_)
                    | Tok::Dec(_)
                    | Tok::Dur(_)
                    | Tok::Angle(_)
                    | Tok::Clock(_)
                    | Tok::Str(_)
                    | Tok::Bytes(_)
                    | Tok::Regex(_)
                    | Tok::LParen
                    | Tok::LBracket
                    | Tok::LBrace
            ),
            _ => false,
        }
    }

    /// The rest of a definition, from its `(`.
    fn def(&mut self, name: String, start: usize) -> PResult<ExprKind> {
        self.bump();
        let params = self.params(Tok::RParen)?;
        self.expect(Tok::RParen, "`)`")?;
        let ret = if *self.peek() == Tok::Arrow {
            self.bump();
            self.types()?
        } else {
            vec![]
        };
        self.expect(Tok::Assign, "`=`")?;
        self.skip_nl();
        let base = std::mem::replace(&mut self.args_base, self.in_args);
        let body = if *self.peek() == Tok::LBrace { self.block() } else { self.expr(ASSIGN_BP) };
        self.args_base = base;
        Ok(ExprKind::Def(name, Rc::new(FnDef { params, ret, body: body?, src: self.src[start..self.prev_end].into() })))
    }

    /// The rest of `op +(a, b) = ...`, from the operator. In `op to(v: vec2, t: polar)` the second type names a target, not a type.
    fn op_def(&mut self, start: usize) -> PResult<ExprKind> {
        let sym = op_symbol(self.peek()).expect("is_op_def checked it");
        self.bump();
        let declared = self.declared.len();
        if sym == "to" {
            let mut colons = (0..).take_while(|i| !matches!(self.peek_at(*i), Tok::RParen | Tok::Eof)).filter(|i| *self.peek_at(*i) == Tok::Colon);
            if let Some(Tok::Ident(target)) = colons.nth(1).map(|i| self.peek_at(i + 1).clone()) {
                self.declared.push(target);
            }
        }
        let def = self.def(format!("op {sym}"), start);
        self.declared.truncate(declared);
        let def = def?;
        let ExprKind::Def(_, f) = &def else { unreachable!("def makes a Def") };
        let n = f.params.len();
        let why = match sym {
            "to" if n < 2 || f.params[1].ty.is_empty() => Some("a value and a target, like `op to(v: vec2, t: polar)`"),
            "to" => None,
            "-" if n == 1 || n == 2 => None,
            "-" => Some("1 or 2 params"),
            "!" | "not" | "~" if n != 1 => Some("1 param"),
            "!" | "not" | "~" => None,
            _ if n != 2 => Some("2 params"),
            _ => None,
        };
        match why {
            Some(why) => Err(Error::new(format!("`op {sym}` takes {why}"), start..self.prev_end)),
            None => Ok(def),
        }
    }

    /// `x, y?: str, n: int = 2, *xs: num` up to `close`; bar closures (`close` is `|`) take no types.
    fn params(&mut self, close: Tok) -> PResult<Vec<Param>> {
        let bar = close == Tok::Bar;
        let mut params: Vec<Param> = Vec::new();
        while *self.peek() != close {
            let start = self.span().start;
            if params.last().is_some_and(|p| matches!(p.pat, Pat::Rest(_))) {
                return Err(Error::new("a `*` param must come last", self.span()));
            }
            let mut p = Param::from(self.pat_item()?);
            if *self.peek() == Tok::Question {
                self.bump();
                p.opt = true;
            }
            if *self.peek() == Tok::Colon {
                if bar {
                    return Err(Error::new("types go in `fn(...)`, since `|x: int|str|` would be ambiguous", self.span()));
                }
                self.bump();
                p.ty = self.types()?;
            }
            if *self.peek() == Tok::Assign {
                self.bump();
                let from = self.span().start;
                // In bars, `|` ends the default: `|x = 1| x`.
                let e = self.expr(if bar { 17 } else { ASSIGN_BP + 1 })?;
                p.default = Some((e, self.src[from..self.prev_end].into()));
            }
            let text = &self.src[start..self.prev_end];
            let ty = p.ty.join("|");
            let why = match &p.pat {
                Pat::Rest(inner) if !matches!(**inner, Pat::Name(_)) => Some("only a name can follow `*`".into()),
                Pat::Rest(_) if p.opt || p.default.is_some() => Some(format!("`{text}` can't take `?` or a default: it's an empty list when left out")),
                Pat::List(..) if p.opt || !p.ty.is_empty() => Some(format!("`{text}` can't take `?` or a type: unpacking already needs a list")),
                _ if p.opt && p.default.is_some() => Some(format!("`{text}` defaults to nil already, so drop the `?` to give it another default")),
                _ if p.opt && p.ty.iter().any(|t| t == "nil") => Some(format!("`{text}` already allows nil, so drop `|nil`")),
                _ if matches!(p.default, Some((Expr { kind: ExprKind::Nil, .. }, _))) && !ty.is_empty() && !p.ty.iter().any(|t| t == "nil") => {
                    Some(format!("`{text}` doesn't allow nil, so write `{}?: {ty}`", text.split(':').next().unwrap_or(text)))
                }
                Pat::Name(_) | Pat::Skip | Pat::List(..) if !p.opt && p.default.is_none() && params.iter().any(|q| q.opt || q.default.is_some()) => {
                    Some(format!("`{text}` needs a default too, since a param before it has one"))
                }
                _ => None,
            };
            if let Some(why) = why {
                return Err(Error::new(why, start..self.prev_end));
            }
            params.push(p);
            if *self.peek() != Tok::Comma {
                break;
            }
            self.bump();
        }
        Ok(params)
    }

    /// `int|str`, after a param's `:` or a fn's `->`.
    fn types(&mut self) -> PResult<Vec<String>> {
        let mut types = vec![];
        loop {
            let span = self.span();
            if !matches!(self.peek(), Tok::Ident(_) | Tok::Fn | Tok::Nil | Tok::Mod) {
                return Err(self.unexpected(self.pos, "a type"));
            }
            let name = &self.src[span.clone()];
            if !crate::signature::TYPES.contains(&name)
                && !crate::signature::is_kind(name)
                && !self.declared.iter().any(|d| d == name)
                && !crate::state::with(|s| s.types.borrow().contains_key(name))
            {
                let hint = crate::error::did_you_mean(name, crate::signature::TYPES.iter().copied());
                return Err(Error::new(format!("unknown type `{name}`{hint}"), span));
            }
            types.push(name.to_string());
            self.bump();
            if *self.peek() != Tok::Bar {
                return Ok(types);
            }
            self.bump();
        }
    }

    /// A pattern, or `*rest` inside a list of them.
    fn pat_item(&mut self) -> PResult<Pat> {
        if *self.peek() != Tok::Star {
            return self.pattern();
        }
        self.bump();
        Ok(Pat::Rest(Box::new(self.pattern()?)))
    }

    /// A name, or `[a, [b, _]]`.
    fn pattern(&mut self) -> PResult<Pat> {
        if *self.peek() != Tok::LBracket {
            return Ok(Pat::Name(self.ident()?));
        }
        let depth = self.depth;
        let p = self.list_pattern();
        self.depth = depth;
        p
    }

    fn list_pattern(&mut self) -> PResult<Pat> {
        self.nest()?;
        let open = self.span();
        self.bump();
        let mut items = Vec::new();
        while *self.peek() != Tok::RBracket {
            items.push(self.pat_item()?);
            if *self.peek() != Tok::Comma {
                break;
            }
            self.bump();
        }
        self.close(Tok::RBracket, &open)?;
        list_pat(items, open.start..self.prev_end)
    }

    /// After `[`: `x[i]`, `x[a..b]`, `x[a..=b]`, `x[a..]`, `x[..b]`.
    fn index(&mut self, lhs: Expr, open: &Span) -> PResult<ExprKind> {
        let bound = |p: &mut Self| -> PResult<Option<Box<Expr>>> {
            Ok(match p.peek() {
                Tok::DotDot | Tok::DotDotEq | Tok::RBracket => None,
                _ => Some(Box::new(p.expr(15)?)),
            })
        };
        let from = bound(self)?;
        let kind = if matches!(self.peek(), Tok::DotDot | Tok::DotDotEq) {
            let incl = self.bump() == Tok::DotDotEq;
            let to = bound(self)?;
            if incl && to.is_none() {
                return Err(self.unexpected(self.pos, "an end bound"));
            }
            ExprKind::Slice(Box::new(lhs), from, to, incl)
        } else {
            let idx = from.ok_or_else(|| self.unexpected(self.pos, "an index"))?;
            ExprKind::Index(Box::new(lhs), idx)
        };
        self.close(Tok::RBracket, open)?;
        Ok(kind)
    }

    /// Units after a number (`touching`: `/`, `*`, `^` only continue the unit without spaces,
    /// so `60 km/h` is a unit but `60 km / h` divides) or after `to` (spaces allowed).
    fn unit_spec(&mut self, touching: bool) -> PResult<UnitSpec> {
        let mut spec = vec![self.unit_term(touching, 1)?];
        // `2.5 million USD`: a number word, then the unit it counts.
        if crate::modules::units::is_number_word(&spec[0].0) && matches!(self.peek(), Tok::Ident(_)) && self.unit_next() {
            spec.push(self.unit_term(touching, 1)?);
        }
        self.unit_rest(&mut spec, touching)?;
        Ok(spec)
    }

    /// More `*unit` or `/unit` terms after the first.
    fn unit_rest(&mut self, spec: &mut UnitSpec, touching: bool) -> PResult<()> {
        loop {
            let sign = match self.peek() {
                Tok::Star => 1,
                Tok::Slash => -1,
                _ => break,
            };
            let unit_next = matches!(self.peek_at(1), Tok::Ident(_) | Tok::In);
            if !unit_next || (touching && !(self.touching() && self.toks[self.pos].1.end == self.toks[self.pos + 1].1.start)) {
                break;
            }
            self.bump();
            spec.push(self.unit_term(touching, sign)?);
        }
        Ok(())
    }

    fn unit_term(&mut self, touching: bool, sign: i8) -> PResult<(String, i8)> {
        let name = match self.peek().clone() {
            Tok::Ident(s) => s,
            Tok::In => "in".into(),
            _ => return Err(self.unexpected(self.pos, "a unit")),
        };
        self.bump();
        let mut pow = 1;
        if let Tok::Sup(n) = *self.peek()
            && self.touching()
        {
            let span = self.span();
            if !(1..=9).contains(&n.abs()) {
                return Err(Error::new("expected a unit power from 1 to 9", span));
            }
            self.bump();
            pow = n;
        }
        if *self.peek() == Tok::Caret && (!touching || self.touching()) {
            self.bump();
            let neg = *self.peek() == Tok::Minus;
            if neg {
                self.bump();
            }
            let span = self.span();
            pow = match self.bump() {
                Tok::Int(n) if (1..=9).contains(&n) => n as i8,
                _ => return Err(Error::new("expected a unit power from 1 to 9", span)),
            };
            if neg {
                pow = -pow;
            }
        }
        Ok((name, pow * sign))
    }

    fn target(&mut self) -> PResult<Target> {
        match self.peek().clone() {
            Tok::Ident(s) if crate::modules::target(&s).is_some() => {
                self.bump();
                let mut arg = None;
                if *self.peek() == Tok::LParen {
                    let open = self.span();
                    self.bump();
                    arg = match self.peek() {
                        Tok::Int(n) => Some(*n),
                        _ => return Err(self.unexpected(self.pos, "an integer").help(format!("`to {s}(n)` takes a literal number, like `to {s}(8)`"))),
                    };
                    self.bump();
                    self.close(Tok::RParen, &open)?;
                }
                Ok(Target::Named(s, arg))
            }
            Tok::Str(s) if !s.contains(['\\', '{']) => {
                self.bump();
                Ok(Target::Str(s))
            }
            // `to "{zone}"` and `to (zone)`: a string computed at run time.
            Tok::Str(_) | Tok::LParen => Ok(Target::Expr(Box::new(self.prefix()?))),
            _ => {
                let mut specs = vec![self.unit_spec(false)?];
                while matches!(self.peek(), Tok::Ident(_) | Tok::In) {
                    specs.push(self.unit_spec(false)?);
                }
                Ok(if let [one] = &mut specs[..] { Target::Unit(std::mem::take(one)) } else { Target::Units(specs) })
            }
        }
    }

    /// `raw` is the text between the quotes, starting at byte `start` of the source.
    fn string(&self, raw: &str, start: usize) -> PResult<ExprKind> {
        let span = start - 1..start + raw.len() + 1;
        let lit_expr = |s: &mut String| Expr { kind: ExprKind::Str(std::mem::take(s).into()), span: span.clone() };
        let mut parts: Vec<Expr> = Vec::new();
        let mut lit = String::new();
        let mut chars = raw.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    let (_, e) = chars.next().expect("lexer guarantees escape target");
                    lit.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        '\\' | '"' | '{' | '}' => e,
                        _ => {
                            let at = start + i;
                            return Err(Error::new(format!("unknown escape `\\{e}`"), at..at + 1 + e.len_utf8()));
                        }
                    });
                }
                '{' => {
                    let end = close_brace(raw, i + 1).ok_or_else(|| {
                        let open = start + i..start + i + 1;
                        Error::new("unclosed `{` in a string", open.clone())
                            .label(open, "opened here")
                            .help("close it with `}`, or write `\\{` for a literal brace")
                    })?;
                    let (src_end, spec) = match spec_colon(&raw[i + 1..end]) {
                        Some(c) => (i + 1 + c, Some(&raw[i + 2 + c..end])),
                        None => (end, None),
                    };
                    if let Some(spec) = spec {
                        crate::modules::math::formatting::Spec::parse(spec).map_err(|m| Error::new(m, start + i..start + end + 1))?;
                    }
                    if raw[i + 1..src_end].trim().is_empty() {
                        return Err(Error::new("empty `{}` in a string", start + i..start + end + 1)
                            .help("put an expression inside, or write `\\{` for a literal brace"));
                    }
                    let e = self.sub_expr(&raw[i + 1..src_end], start + i + 1)?;
                    let e = match spec {
                        Some(spec) => Expr { span: e.span.clone(), kind: ExprKind::Format(Box::new(e), spec.into()) },
                        None => e,
                    };
                    if !lit.is_empty() || parts.is_empty() {
                        parts.push(lit_expr(&mut lit));
                    }
                    parts.push(e);
                    while chars.as_str().len() > raw.len() - end - 1 {
                        chars.next();
                    }
                }
                _ => lit.push(c),
            }
        }
        if parts.is_empty() {
            return Ok(ExprKind::Str(lit.into()));
        }
        if !lit.is_empty() {
            parts.push(lit_expr(&mut lit));
        }
        // Leading Str part makes `+` concatenate everything.
        let mut it = parts.into_iter();
        let mut acc = it.next().expect("parts is non-empty");
        for p in it {
            acc = Expr { kind: ExprKind::Binary(BinOp::Add, Box::new(acc), Box::new(p)), span: span.clone() };
        }
        Ok(acc.kind)
    }

    /// The one expression in `text`, which starts at byte `offset` of the source: a `{...}` in a string or command word.
    fn sub_expr(&self, text: &str, offset: usize) -> PResult<Expr> {
        let toks = lex_at(text, offset)?;
        let mut p = Parser {
            src: self.src,
            declared: self.declared.clone(),
            toks,
            pos: 0,
            prev_end: 0,
            holes: 0,
            ph: 0,
            ph_at: None,
            in_pipe: false,
            bare: 0,
            lone: 0,
            args_base: 0,
            in_args: 0,
            no_ph: false,
            depth: self.depth + 1,
            stop: usize::MAX,
            stmt_start: usize::MAX,
            commands: vec![],
        };
        p.skip_nl();
        let e = p.expr(0)?;
        p.skip_nl();
        p.expect(Tok::Eof, "`}`")?;
        Ok(e)
    }

    /// Comma-separated exprs up to `close`; newlines and a trailing comma allowed.
    fn list(&mut self, close: Tok, open: &Span, named: bool) -> PResult<Vec<Expr>> {
        let mut items = Vec::new();
        let mut bare = 0;
        self.skip_nl();
        while *self.peek() != close {
            if let (true, Tok::Ident(name), Tok::Colon) = (named, self.peek().clone(), self.peek_at(1)) {
                let start = self.span().start;
                self.bump();
                self.bump();
                let v = self.arg(named, &mut bare, |p| p.expr(0))?;
                items.push(self.mk(ExprKind::Named(name, Box::new(v)), start));
            } else if named && items.iter().any(|a| matches!(a.kind, ExprKind::Named(..))) {
                return Err(self.unexpected(self.pos, "a named argument; positional ones come first"));
            } else {
                items.push(self.arg(named, &mut bare, |p| p.item(0))?);
            }
            self.skip_nl();
            if *self.peek() != Tok::Comma {
                break;
            }
            self.bump();
            self.skip_nl();
        }
        self.close(close, open)?;
        self.bare = bare;
        Ok(items)
    }

    fn ph_enter(&mut self) -> (usize, Option<Span>) {
        (std::mem::take(&mut self.ph), self.ph_at.take())
    }

    /// Back to the outer placeholder scope, returning the count and first span of the one just closed.
    fn ph_leave(&mut self, outer: (usize, Option<Span>)) -> (usize, Option<Span>) {
        (std::mem::replace(&mut self.ph, outer.0), std::mem::replace(&mut self.ph_at, outer.1))
    }

    /// A call argument in its own placeholder scope: a bare `_` becomes the call's next parameter, any other `_` makes the argument a fn.
    ///
    /// A lone `_` argument of a call nested in another argument, like `chr(ord(_) - 5)`, belongs to the enclosing argument instead.
    fn arg(&mut self, call: bool, bare: &mut usize, parse: impl FnOnce(&mut Self) -> PResult<Expr>) -> PResult<Expr> {
        if !call {
            return parse(self);
        }
        let nested = self.in_args > self.args_base;
        let lone = matches!(self.peek(), Tok::Ident(s) if s == "_") && matches!(self.peek_at(1), Tok::Comma | Tok::RParen);
        if nested && lone && !self.no_ph && !self.in_pipe {
            self.lone += 1;
            return parse(self);
        }
        let lone_before = self.lone;
        let (outer, no_ph) = (self.ph_enter(), std::mem::take(&mut self.no_ph));
        self.in_args += 1;
        let e = parse(self);
        self.in_args -= 1;
        let mut e = e?;
        self.no_ph = no_ph;
        let (lone_only, outer_free) = (self.lone - lone_before, outer.0 == 0);
        let (n, at) = self.ph_leave(outer);
        if nested && outer_free && n > 0 && lone_only == n && !self.no_ph && !self.in_pipe {
            // every placeholder came from a nested lone `_` and the enclosing scope has none of its own, so they are its placeholders
            self.ph = n;
            self.ph_at = at;
            return Ok(e);
        }
        if n == 1
            && let ExprKind::Ident(name) = &mut e.kind
        {
            *bare += 1;
            *name = format!("_{bare}");
        } else if n > 0 {
            e = self.wrap_ph(e, n);
        }
        Ok(e)
    }

    /// `body` as `|_1, _2| body` over its `n` placeholders; its source is parenthesized so it parses again as the same fn.
    fn wrap_ph(&self, body: Expr, n: usize) -> Expr {
        let params = (1..=n).map(|i| Pat::Name(format!("_{i}")).into()).collect();
        let src = format!("({})", &self.src[body.span.clone()]).into();
        Expr { span: body.span.clone(), kind: ExprKind::Fn(Rc::new(FnDef { params, ret: vec![], body, src })) }
    }

    fn at_semi(&self) -> bool {
        *self.peek() == Tok::Newline && &self.src[self.span()] == ";"
    }

    /// Newlines, which don't end a row, unlike a `;`.
    fn skip_lines(&mut self) {
        while *self.peek() == Tok::Newline && !self.at_semi() {
            self.bump();
        }
    }

    /// Whether a new item starts at token `i` of a row without commas: a space splits operands, unless it surrounds a binary
    /// operator (`[1 -2]` is two items, `[1 - 2]` one) or holds a number and its unit (`[5 km]`).
    fn splits_at(&self, i: usize) -> bool {
        let touching = |i: usize| self.toks[i].1.end == self.toks[i + 1].1.start;
        if touching(i - 1) || !self.ends_operand(i - 1) || self.cross_at(i - 1) {
            return false;
        }
        match &self.toks[i].0 {
            Tok::Minus => touching(i),
            Tok::Ident(_) if is_number(&self.toks[i - 1].0) => self.toks[i + 1].0 == Tok::LParen && touching(i),
            Tok::Ident(_) => !self.cross_at(i),
            Tok::Bar | Tok::Or => false,
            t => starts_operand(t),
        }
    }

    /// Just after `[`: whether `;` splits it into rows, and the tokens that start an item in its rows without commas.
    /// A row holding a block, a lambda or `to` has spaces that mean something else, so only commas split it.
    fn row_marks(&self) -> (bool, Vec<usize>) {
        let (mut matrix, mut splits, mut row, mut plain, mut depth) = (false, vec![], vec![], false, 0);
        let mut i = self.pos;
        loop {
            let t = &self.toks[i].0;
            let closer = matches!(t, Tok::RParen | Tok::RBracket | Tok::RBrace);
            let last = *t == Tok::Eof || (depth == 0 && closer);
            let semi = depth == 0 && *t == Tok::Newline && &self.src[self.toks[i].1.clone()] == ";";
            if last || semi {
                if !plain {
                    splits.append(&mut row);
                }
                row.clear();
                plain = false;
                matrix |= semi;
                if last {
                    return (matrix, splits);
                }
            } else if depth == 0 {
                plain |= matches!(t, Tok::Comma | Tok::Fn | Tok::If | Tok::While | Tok::For | Tok::Return | Tok::Bar | Tok::Or | Tok::To);
                plain |= matches!(t, Tok::Ident(s) if s == "match");
                if i > self.pos && self.splits_at(i) {
                    row.push(i);
                }
            }
            if closer {
                depth -= 1;
            } else if matches!(t, Tok::LParen | Tok::LBracket | Tok::LBrace) {
                depth += 1;
            }
            i += 1;
        }
    }

    /// After `[`: a list, or a matrix when `;` splits it into rows. `[1, 2; 3, 4]`, `[1 2; 3 4]`.
    fn bracket(&mut self, open: &Span) -> PResult<ExprKind> {
        let (matrix, splits) = self.row_marks();
        if !matrix && splits.is_empty() {
            return Ok(ExprKind::List(self.list(Tok::RBracket, open, false)?));
        }
        let start = open.start;
        let typo = splits.iter().any(|&i| self.toks[i].0 == Tok::Minus && !(is_number(&self.toks[i - 1].0) && is_number(&self.toks[i + 1].0)));
        let outer = self.stop;
        let mut rows = vec![vec![]];
        loop {
            self.skip_lines();
            if matches!(self.peek(), Tok::RBracket | Tok::Eof) {
                break;
            }
            if self.at_semi() {
                self.bump();
                rows.push(vec![]);
                continue;
            }
            self.stop = splits.get(splits.partition_point(|&i| i <= self.pos)).copied().unwrap_or(outer);
            let item = self.item(0);
            self.stop = outer;
            rows.last_mut().expect("rows starts with one").push(item?);
            self.skip_lines();
            if *self.peek() == Tok::Comma {
                self.bump();
            } else if !matches!(self.peek(), Tok::RBracket | Tok::Eof) && !self.at_semi() && splits.binary_search(&self.pos).is_err() {
                return Err(self.unexpected(self.pos, "`,`, `;` or `]`").label(open.clone(), "opened here"));
            }
        }
        self.close(Tok::RBracket, open)?;
        let kind = if matrix {
            if rows.last().is_some_and(Vec::is_empty) {
                rows.pop();
            }
            let spread = rows.iter().flatten().any(|e| matches!(e.kind, ExprKind::Spread(_)));
            if rows.is_empty() || rows.iter().any(Vec::is_empty) || (!spread && rows.iter().any(|r| r.len() != rows[0].len())) {
                return Err(Error::new("a matrix's rows must all have the same number of items", start..self.prev_end));
            }
            ExprKind::List(rows.into_iter().map(|r| self.mk(ExprKind::List(r), start)).collect())
        } else {
            ExprKind::List(rows.pop().expect("rows starts with one"))
        };
        if typo {
            let msg = format!(
                "`{}` has a `-` touching only what's after it, which starts a new item: write a comma to keep it, or space it to subtract",
                &self.src[start..self.prev_end]
            );
            return Ok(ExprKind::Lint(msg, Box::new(self.mk(kind, start))));
        }
        Ok(kind)
    }

    /// `{ pat => body, ... }` after `match`'s value; arms separated by commas and/or newlines, and a body may be a block.
    fn match_arms(&mut self) -> PResult<Vec<(Pat, Expr)>> {
        let open = self.span();
        self.expect(Tok::LBrace, "`{`")?;
        let mut arms = vec![];
        self.skip_nl();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            let pat = self.match_pat()?;
            self.expect(Tok::FatArrow, "`=>`")?;
            self.skip_nl();
            let body = if *self.peek() == Tok::LBrace { self.block()? } else { self.expr(0)? };
            arms.push((pat, body));
            if *self.peek() == Tok::Comma {
                self.bump();
            } else if !matches!(self.peek(), Tok::Newline | Tok::RBrace | Tok::Eof) {
                return Err(self.unexpected(self.pos, "`,`, a newline or `}`"));
            }
            self.skip_nl();
        }
        self.close(Tok::RBrace, &open)?;
        Ok(arms)
    }

    /// A `match` pattern: `_`, a name, `[a, *rest]`, `{x, y}`, a value or a range of values.
    fn match_pat(&mut self) -> PResult<Pat> {
        let depth = self.depth;
        let p = self.match_pat_inner();
        self.depth = depth;
        p
    }

    fn match_pat_inner(&mut self) -> PResult<Pat> {
        self.nest()?;
        let open = self.span();
        match self.peek().clone() {
            Tok::Ident(n) if !matches!(self.peek_at(1), Tok::LParen | Tok::Dot) => {
                self.bump();
                Ok(if n == "_" { Pat::Skip } else { Pat::Name(n) })
            }
            Tok::LBracket => {
                self.bump();
                let mut items = vec![];
                while *self.peek() != Tok::RBracket {
                    if *self.peek() == Tok::Star {
                        self.bump();
                        items.push(Pat::Rest(Box::new(self.match_pat()?)));
                    } else {
                        items.push(self.match_pat()?);
                    }
                    if *self.peek() != Tok::Comma {
                        break;
                    }
                    self.bump();
                }
                self.close(Tok::RBracket, &open)?;
                list_pat(items, open.start..self.prev_end)
            }
            Tok::LBrace => {
                self.bump();
                let mut keys = vec![];
                while *self.peek() != Tok::RBrace {
                    keys.push(self.ident()?);
                    if *self.peek() != Tok::Comma {
                        break;
                    }
                    self.bump();
                }
                self.close(Tok::RBrace, &open)?;
                Ok(Pat::Record(keys))
            }
            _ => {
                // Above `|`, so `0 | 1 =>` is an error rather than the bitwise or of the two.
                let lo = self.expr(17)?;
                if !matches!(self.peek(), Tok::DotDot | Tok::DotDotEq) {
                    return Ok(Pat::Lit(Box::new(lo)));
                }
                let incl = self.bump() == Tok::DotDotEq;
                Ok(Pat::Range(Box::new(lo), Box::new(self.expr(17)?), incl))
            }
        }
    }

    /// `{a: 1, "b c": 2}`; entries separated by commas and/or newlines.
    fn map_body(&mut self, open: &Span) -> PResult<Vec<(String, Expr)>> {
        let mut entries = Vec::new();
        self.skip_nl();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            let key = match self.peek().clone() {
                Tok::Ident(s) => s,
                Tok::Str(s) if !s.contains(['\\', '{']) => s,
                _ => return Err(self.unexpected(self.pos, "a map key")),
            };
            self.bump();
            self.expect(Tok::Colon, "`:`")?;
            self.skip_nl();
            entries.push((key, self.expr(0)?));
            if *self.peek() == Tok::Comma {
                self.bump();
            }
            self.skip_nl();
        }
        self.close(Tok::RBrace, open)?;
        Ok(entries)
    }

    /// After `{`: `{x * 2 | x in xs}` maps and `{x in xs | x > 2}` filters, both into a set.
    fn set_builder(&mut self, open: &Span) -> PResult<ExprKind> {
        // The first `|` outside brackets splits it.
        let mut depth = 0;
        let bar = self.toks[self.pos..].iter().position(|(t, _)| {
            match t {
                Tok::LParen | Tok::LBracket | Tok::LBrace => depth += 1,
                Tok::RParen | Tok::RBracket | Tok::RBrace | Tok::Eof => depth -= 1,
                _ => {}
            }
            depth < 0 || depth == 0 && *t == Tok::Bar
        });
        let outer = std::mem::replace(&mut self.stop, bar.map_or(usize::MAX, |i| self.pos + i));
        let left = self.expr(0);
        self.stop = outer;
        let left = left?;
        self.expect(Tok::Bar, "`:` for a map, or `|` for a set like `{x * 2 | x in xs}`")?;
        let right = self.expr(0)?;
        let binding = |e: Expr| match e.kind {
            ExprKind::Binary(BinOp::In, var, src) => match var.kind {
                ExprKind::Ident(x) => Ok((x, *src)),
                _ => Err(Error::new("expected a name", var.span)),
            },
            _ => Err(Error::new("expected `x in xs`", e.span)),
        };
        let ((var, src), body, method) = match left.kind {
            ExprKind::Binary(BinOp::In, ref var, _) if matches!(var.kind, ExprKind::Ident(_)) => (binding(left)?, right, "filter"),
            _ => (binding(right)?, left, "map"),
        };
        self.skip_nl();
        self.close(Tok::RBrace, open)?;
        let start = open.start;
        let fn_src = format!("|{var}| {}", &self.src[body.span.clone()]).into();
        let f = Expr { span: body.span.clone(), kind: ExprKind::Fn(Rc::new(FnDef { params: vec![Pat::Name(var).into()], ret: vec![], body, src: fn_src })) };
        // `to list` so sets, strings and ranges all work.
        let src = Expr { span: src.span.clone(), kind: ExprKind::To(Box::new(src), Target::Named("list".into(), None)) };
        let call = ExprKind::Call(Box::new(self.mk(ExprKind::Field(Box::new(src), method.into()), start)), vec![f]);
        Ok(ExprKind::To(Box::new(self.mk(call, start)), Target::Named("set".into(), None)))
    }

    fn block(&mut self) -> PResult<Expr> {
        let open = self.span();
        let start = open.start;
        self.expect(Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        self.skip_nl();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            stmts.push(self.stmt()?);
            if !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
                self.expect(Tok::Newline, "a newline or `}`")?;
            }
            self.skip_nl();
        }
        self.close(Tok::RBrace, &open)?;
        Ok(self.mk(ExprKind::Block(stmts), start))
    }
}

/// `_`, or the `_1`, `_2` it was renamed to in expression position.
fn is_placeholder(n: &str) -> bool {
    n.strip_prefix('_').is_some_and(|d| d.bytes().all(|b| b.is_ascii_digit()))
}

/// The left side of `[a, [b, _]] = xs` as a pattern.
fn to_pat(e: &Expr) -> PResult<Pat> {
    match &e.kind {
        ExprKind::Ident(n) if is_placeholder(n) => Ok(Pat::Skip),
        ExprKind::Ident(n) => Ok(Pat::Name(n.clone())),
        ExprKind::List(items) => list_pat(items.iter().map(to_pat).collect::<PResult<_>>()?, e.span.clone()),
        ExprKind::Spread(e) => Ok(Pat::Rest(Box::new(to_pat(e)?))),
        _ => Err(Error::new("cannot assign to this", e.span.clone()).help("a list pattern holds names, `_`, `*rest` or more lists: `[a, [b, _]] = xs`")),
    }
}

/// A list pattern from its items, with `_` skipping and at most one `*rest`.
fn list_pat(items: Vec<Pat>, span: Span) -> PResult<Pat> {
    let items: Vec<Pat> = items
        .into_iter()
        .map(|p| match p {
            Pat::Name(n) if n == "_" => Pat::Skip,
            Pat::Rest(p) if matches!(&*p, Pat::Name(n) if n == "_") => Pat::Rest(Box::new(Pat::Skip)),
            p => p,
        })
        .collect();
    if items.iter().filter(|p| matches!(p, Pat::Rest(_))).count() > 1 {
        return Err(Error::new("only one `*rest` per list pattern", span).help("`[a, *rest] = xs` or `[*init, z] = xs`"));
    }
    Ok(Pat::List(items, span))
}

/// `a op b`, where `x + 20%` means `x * (1 + 20%)`, `x - 20%` means `x * (1 - 20%)` and `x ± 20%` means `x ± x * 20%`.
fn arith(op: BinOp, lhs: Expr, rhs: Expr) -> ExprKind {
    if op == BinOp::PlusMinus && matches!(rhs.kind, ExprKind::Percent(_)) {
        let span = rhs.span.clone();
        let err = Expr { kind: ExprKind::Binary(BinOp::Mul, Box::new(lhs.clone()), Box::new(rhs)), span };
        return ExprKind::Binary(op, Box::new(lhs), Box::new(err));
    }
    if matches!(op, BinOp::Add | BinOp::Sub) && matches!(rhs.kind, ExprKind::Percent(_)) {
        let span = rhs.span.clone();
        let one = Expr { kind: ExprKind::Int(1, 10), span: span.clone() };
        let factor = Expr { kind: ExprKind::Binary(op, Box::new(one), Box::new(rhs)), span };
        return ExprKind::Binary(BinOp::Mul, Box::new(lhs), Box::new(factor));
    }
    ExprKind::Binary(op, Box::new(lhs), Box::new(rhs))
}

/// The bytes of `b"..."`, whose contents start at byte `start`: string escapes plus `\xHH`; other text is UTF-8.
fn byte_string(raw: &str, start: usize) -> PResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut chars = raw.char_indices();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        let (_, e) = chars.next().expect("lexer guarantees escape target");
        out.push(match e {
            'n' => b'\n',
            't' => b'\t',
            'r' => b'\r',
            '0' => 0,
            '\\' | '"' | '{' | '}' => e as u8,
            'x' => match raw.get(i + 2..i + 4).filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit())).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(b) => {
                    chars.nth(1);
                    b
                }
                None => return Err(Error::new("`\\x` needs two hex digits", start + i..start + i + 2).help("write a byte as `\\x7f`")),
            },
            _ => {
                let at = start + i;
                return Err(Error::new(format!("unknown escape `\\{e}`"), at..at + 1 + e.len_utf8()));
            }
        });
    }
    Ok(out)
}

fn is_number(t: &Tok) -> bool {
    matches!(t, Tok::Int(_) | Tok::Based(_) | Tok::Big(_) | Tok::Float(_) | Tok::Dec(_))
}

fn starts_operand(t: &Tok) -> bool {
    use Tok::*;
    matches!(
        t,
        Ident(_)
            | Int(_)
            | Based(_)
            | Big(_)
            | Float(_)
            | Dec(_)
            | Dur(_)
            | Angle(_)
            | Clock(_)
            | Currency(_)
            | Str(_)
            | Bytes(_)
            | Regex(_)
            | LParen
            | LBracket
            | LBrace
            | Minus
            | Bang
            | Tilde
            | Sqrt
            | Bar
            | Or
            | Fn
            | If
            | True
            | False
            | Nil
    )
}

/// Byte index of a `:` starting a format spec in an interpolation (`x:.2f`), outside brackets and strings.
fn spec_colon(s: &str) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0, false, false);
    for (i, c) in s.char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (_, '\\') => esc = true,
                (_, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Byte index of the `}` closing an interpolation that starts at `from`, skipping nested braces and strings.
fn close_brace(s: &str, from: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (1, false, false);
    for (i, c) in s[from..].char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (_, '\\') => esc = true,
                (_, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(from + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// What an `op` definition calls the operator `t`; none for one it can't overload.
fn op_symbol(t: &Tok) -> Option<&'static str> {
    Some(match t {
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::StarStar => "**",
        Tok::Slash => "/",
        Tok::SlashSlash => "//",
        Tok::Percent => "%",
        Tok::Amp => "&",
        Tok::Bar => "|",
        Tok::Caret => "^",
        Tok::Shl => "<<",
        Tok::Shr => ">>",
        Tok::Eq => "==",
        Tok::Ne => "!=",
        Tok::Lt => "<",
        Tok::Le => "<=",
        Tok::Gt => ">",
        Tok::Ge => ">=",
        Tok::Bang => "!",
        Tok::Not => "not",
        Tok::Tilde => "~",
        Tok::To => "to",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    fn sexp(src: &str) -> String {
        let ast = super::parse(src).unwrap();
        format!("{:?}", ast[0].kind)
    }

    #[test]
    fn precedence() {
        assert!(sexp("1 + 2 * 3").starts_with("Binary(Add"));
        assert!(sexp("-2 ** 2").starts_with("Unary(Neg"));
        assert!(sexp("x = 5 km to mi").starts_with("Assign"));
    }

    #[test]
    fn units_touching_vs_spaced() {
        assert!(sexp("60 km/h").contains(r#"[("km", 1), ("h", -1)]"#));
        assert!(sexp("60 km / h").starts_with("Binary(Div"));
        assert!(sexp("a m to km").starts_with("To(Expr { kind: Qty(Expr { kind: Ident"));
        assert!(sexp("5 cm to in").contains(r#"Unit([("in", 1)])"#));
        assert!(sexp("1 kg*m/s^2").contains(r#"("s", -2)"#));
    }

    #[test]
    fn juxtaposition() {
        assert!(sexp("3m").starts_with("Juxt("));
        assert!(sexp("3 m").starts_with("Qty("));
        assert!(sexp("9.8m/s^2").contains(r#"[("m", 1), ("s", -2)]"#));
        assert!(sexp("1/2x").starts_with("Binary(Div") && sexp("1/2x").contains("Juxt("));
        assert!(sexp("2sin(50)").starts_with("Binary(Mul") && sexp("2sin(50)").contains("Call("));
        assert!(!sexp("2 sin(50)").contains("Juxt("));
        assert!(sexp("(a + b)x").starts_with("Binary(Mul"));
        assert!(sexp("2(3 + 4)").starts_with("Call("));
        assert!(sexp("sin 50").starts_with("Apply("));
        assert!(sexp("sin x").starts_with("Qty("));
    }

    #[test]
    fn interpolation_and_slices() {
        assert!(sexp(r#""a{1 + 2}b""#).starts_with("Binary(Add"));
        assert!(sexp(r#""no \{brace\}""#).starts_with("Str(\"no {brace}\")"));
        assert!(sexp("s[-3..]").starts_with("Slice("));
    }

    #[test]
    fn pipe_desugars_to_call() {
        let s = sexp("xs\n  |> map(f)\n  |> len");
        assert!(s.starts_with("Call(Expr { kind: Ident(\"len\")"), "{s}");
    }

    #[test]
    fn rejects_bad_input() {
        for src in ["1 = 2", r#""{1""#, "r\"(\"", "5 to hex(x)"] {
            assert!(super::parse(src).is_err(), "{src}");
        }
    }

    #[test]
    fn dots() {
        let err = |src: &str| super::parse(src).unwrap_err().msg;
        assert!(sexp(".5 m").starts_with("Qty("));
        assert!(sexp("a.b").starts_with("Field("));
        assert!(sexp("xs\n  .map(f)").starts_with("Call(Expr { kind: Field("));
        for src in ["a . b", "a · b", "a.[1]", "a.(b)"] {
            assert!(sexp(src).starts_with("Binary(Dot"), "{src}");
        }
        assert!(sexp("a.*b").starts_with("Each(Mul") && sexp("a .* b").starts_with("Each(Mul") && sexp("2.*x").starts_with("Each(Mul"));
        for src in ["a .b", "a. b"] {
            assert!(err(src).contains("both sides"), "{src}");
        }
        assert_eq!(err("x.5"), "a number can't touch the value before it");
        assert_eq!(err("1.5.5"), "a number can't touch the value before it");
        assert_eq!(super::parse("0...5").unwrap_err().help, ["did you mean `0..5`?"]);
        assert_eq!(err(".name"), "placeholders are `_`: write `_.name`");
        assert_eq!(err("a\n. b"), "placeholders are `_`: write `_.b`");
    }

    #[test]
    fn cross_word() {
        assert!(sexp("a x b").starts_with("Binary(Cross") && sexp("a × b").starts_with("Binary(Cross"));
        assert!(sexp("a x -b").starts_with("Binary(Cross"));
        assert!(sexp("2 x - 1").starts_with("Binary(Sub"));
        assert!(sexp("2 x").starts_with("Qty(") && sexp("axb").starts_with("Ident("));
        assert!(sexp("if n x { 1 }").starts_with("If("));
    }

    #[test]
    fn rows() {
        let items = |src: &str| match super::parse(src).unwrap().remove(0).kind {
            super::ExprKind::List(items) => items.len(),
            super::ExprKind::Lint(_, e) => match e.kind {
                super::ExprKind::List(items) => items.len(),
                k => panic!("{k:?}"),
            },
            k => panic!("{k:?}"),
        };
        for (src, n) in [
            ("[1 2 3]", 3),
            ("[1 -2]", 2),
            ("[1 - 2]", 1),
            ("[1-2]", 1),
            ("[a b]", 2),
            ("[a (b)]", 2),
            ("[f(x) g(y)]", 2),
            ("[1m 2m]", 2),
            ("[(1 m) (2 m)]", 2),
            ("[5 km]", 1),
            ("[1 m 2 m]", 2),
            ("[60 km/h 30 km/h]", 2),
            ("[3 sin(x)]", 2),
            ("[5! 3]", 2),
            ("[20% 30%]", 2),
            ("[a x b]", 1),
            ("[x y]", 2),
            ("[1 ft 11 in, 2]", 2),
            ("[5 km to m]", 1),
            ("[if a { 1 } else { 2 }]", 1),
            ("[|x| x + 1]", 1),
            ("[1, 2; 3, 4]", 2),
            ("[1 2; 3 4; 5 6]", 3),
            ("[1 2;\n 3 4\n]", 2),
            ("[1, 2;]", 1),
            ("[fn(x) { a; b }]", 1),
        ] {
            assert_eq!(items(src), n, "{src}");
        }
        assert!(matches!(super::parse("[a -b]").unwrap()[0].kind, super::ExprKind::Lint(..)));
        assert!(matches!(super::parse("[1 -2]").unwrap()[0].kind, super::ExprKind::List(_)));
        for src in ["[1 2; 3]", "[1 2\n 3 4]", "[;]", "[1;;2]"] {
            assert!(super::parse(src).is_err(), "{src}");
        }
    }

    #[test]
    fn caps_nesting() {
        let run = || {
            let n = super::MAX_NEST;
            assert!(super::parse(&format!("{}1{}", "(".repeat(n - 1), ")".repeat(n - 1))).is_ok());
            let deep = |k| {
                [
                    format!("{}1{}", "(".repeat(k), ")".repeat(k)),
                    format!("{}1", "1+".repeat(k)),
                    "-".repeat(k) + "1",
                    format!("|{}x{}| 1", "[".repeat(k), "]".repeat(k)),
                ]
            };
            for src in deep(n + 1).iter().chain(&deep(100_000)) {
                assert!(super::parse(src).is_err(), "{}", &src[..20]);
            }
        };
        // Debug frames are ~10x release ones.
        std::thread::Builder::new().stack_size(crate::STACK).spawn(run).unwrap().join().unwrap();
    }
}
