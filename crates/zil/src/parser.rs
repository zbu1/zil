//! Recursive-descent parser: tokens into statements of `Expr`, including string `{}` interpolation.

use crate::Error;
use crate::ast::{BinOp, Expr, ExprKind, FnDef, Pat, Target, UnOp, UnitSpec};
use crate::lexer::{Span, Tok, lex_at, lex_loose};
use std::rc::Rc;

mod command;

type PResult<T> = Result<T, Error>;
/// Command lines' spans, each with its canonical call or why its words don't parse.
type Commands = Vec<(Span, PResult<String>)>;

pub fn parse(src: &str) -> PResult<Vec<Expr>> {
    Ok(program(src)?.0)
}

/// `src` with each command line rewritten as the call it means: `ls -la src` is `ls("src", {l: true, a: true})`.
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

/// The statements of `src`, and each command line's span with its canonical call.
fn program(src: &str) -> PResult<(Vec<Expr>, Commands)> {
    let mut p = Parser { src, toks: lex_loose(src)?, pos: 0, prev_end: 0, holes: 0, depth: 0, stop: usize::MAX, stmt_start: usize::MAX, commands: vec![] };
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
    toks: Vec<(Tok, Span)>,
    pos: usize,
    prev_end: usize,
    /// `_` placeholders seen so far, so `x |> f(_, 2)` knows to bind `_` instead of inserting `x`.
    holes: usize,
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
        Tok::Plus | Tok::Minus | Tok::PlusMinus => (24, 25),
        Tok::Star | Tok::Slash | Tok::SlashSlash | Tok::Percent | Tok::Of | Tok::Mod => (26, 27),
        Tok::StarStar => (30, 29),
        _ => return None,
    })
}
const ASSIGN_BP: u8 = 2;
const TO_BP: u8 = 3;
const PREFIX_BP: u8 = 28;
const POSTFIX_BP: u8 = 32;

fn binop(t: &Tok) -> BinOp {
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

    /// Token after any run of newlines, so `|>` / `.` can start a continuation line.
    fn peek_past_nl(&self) -> &Tok {
        let mut i = self.pos;
        while self.toks[i].0 == Tok::Newline {
            i += 1;
        }
        &self.toks[i].0
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
            if *self.peek() == Tok::Newline && matches!(self.peek_past_nl(), Tok::Pipe | Tok::Dot) {
                self.skip_nl();
            }
            let t = self.peek().clone();
            if matches!(t, Tok::LParen | Tok::LBracket | Tok::Dot) {
                if POSTFIX_BP < min_bp {
                    break;
                }
                let open = self.span();
                self.bump();
                let kind = match t {
                    Tok::LParen => ExprKind::Call(Box::new(lhs), self.list(Tok::RParen, &open)?),
                    Tok::LBracket => self.index(lhs, &open)?,
                    _ => ExprKind::Field(Box::new(lhs), self.ident()?),
                };
                lhs = self.mk(kind, start);
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
            let rhs = self.expr(rbp)?;
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
                    let value = self.mk(arith(op, lhs.clone(), rhs), start);
                    ExprKind::Assign(Box::new(lhs), Box::new(value))
                }
                // `x |> f(_, 2)` => `(|_| f(_, 2))(x)`
                Tok::Pipe if hole => {
                    let span = rhs.span.clone();
                    // Called on the spot and never stored, so it's never saved and its `src` never parsed again.
                    let src = format!("|_| {}", &self.src[span.clone()]).into();
                    let f = Expr { kind: ExprKind::Fn(Rc::new(FnDef { params: vec![Pat::Name("_".into())], body: rhs, src })), span };
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
                } else if *self.peek() == Tok::Percent && self.touching() && !starts_operand(self.peek_at(1)) {
                    self.bump();
                    ExprKind::Percent(Box::new(self.mk(kind, start)))
                } else if self.unit_next() {
                    let num = self.mk(kind, start);
                    let qty = ExprKind::Qty(Box::new(num), self.unit_spec(true)?);
                    // `5 ft 11 in`, `1 h 30 min`: adjacent quantities add, as one literal.
                    if matches!(self.peek(), Tok::Int(_) | Tok::Float(_) | Tok::Dec(_)) && matches!(self.peek_at(1), Tok::Ident(_) | Tok::In) {
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
            Tok::Regex(src) => match regex::Regex::new(&src) {
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
            // `a m`: a variable holding a number, given a unit.
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
                let e = self.expr(0)?;
                self.skip_nl();
                self.close(Tok::RParen, &tok_span)?;
                return Ok(e);
            }
            Tok::LBracket => ExprKind::List(self.list(Tok::RBracket, &tok_span)?),
            Tok::LBrace => {
                self.skip_nl();
                if matches!(self.peek(), Tok::RBrace) || matches!(self.peek_at(1), Tok::Colon) {
                    ExprKind::Map(self.map_body(&tok_span)?)
                } else {
                    self.set_builder(&tok_span)?
                }
            }
            Tok::Minus => ExprKind::Unary(UnOp::Neg, Box::new(self.expr(PREFIX_BP)?)),
            Tok::Bang => ExprKind::Unary(UnOp::Not, Box::new(self.expr(PREFIX_BP)?)),
            Tok::Tilde => ExprKind::Unary(UnOp::BitNot, Box::new(self.expr(PREFIX_BP)?)),
            // `|x, y| body`, or `|| body` with no params.
            t @ (Tok::Bar | Tok::Or) => {
                let params = if t == Tok::Or { Vec::new() } else { self.params(Tok::Bar)? };
                if t == Tok::Bar {
                    self.expect(Tok::Bar, "`|`")?;
                }
                let body = self.expr(0)?;
                ExprKind::Fn(Rc::new(FnDef { params, body, src: self.src[start..self.prev_end].into() }))
            }
            Tok::Fn => {
                self.expect(Tok::LParen, "`(`")?;
                let params = self.params(Tok::RParen)?;
                self.expect(Tok::RParen, "`)`")?;
                let body = self.block()?;
                ExprKind::Fn(Rc::new(FnDef { params, body, src: self.src[start..self.prev_end].into() }))
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
            _ => return Err(self.unexpected(at, "an expression")),
        };
        Ok(self.mk(kind, start))
    }

    /// After a number or variable: a unit follows. `in` is inches only when no expression
    /// follows it, so `5 in to cm` is inches but `5 in xs` is membership.
    fn unit_next(&self) -> bool {
        match self.peek() {
            Tok::Ident(_) => true,
            Tok::In => !matches!(
                self.peek_at(1),
                Tok::Ident(_)
                    | Tok::Int(_)
                    | Tok::Based(_)
                    | Tok::Big(_)
                    | Tok::Float(_)
                    | Tok::Dec(_)
                    | Tok::Dur(_)
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

    fn params(&mut self, close: Tok) -> PResult<Vec<Pat>> {
        let mut params = Vec::new();
        while *self.peek() != close {
            params.push(self.pattern()?);
            if *self.peek() != Tok::Comma {
                break;
            }
            self.bump();
        }
        Ok(params)
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
                Ok(if specs.len() == 1 { Target::Unit(specs.pop().unwrap()) } else { Target::Units(specs) })
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
        let mut acc = it.next().unwrap();
        for p in it {
            acc = Expr { kind: ExprKind::Binary(BinOp::Add, Box::new(acc), Box::new(p)), span: span.clone() };
        }
        Ok(acc.kind)
    }

    /// The one expression in `text`, which starts at byte `offset` of the source: a `{...}` in a string or command word.
    fn sub_expr(&self, text: &str, offset: usize) -> PResult<Expr> {
        let toks = lex_at(text, offset)?;
        let mut p =
            Parser { src: self.src, toks, pos: 0, prev_end: 0, holes: 0, depth: self.depth + 1, stop: usize::MAX, stmt_start: usize::MAX, commands: vec![] };
        p.skip_nl();
        let e = p.expr(0)?;
        p.skip_nl();
        p.expect(Tok::Eof, "`}`")?;
        Ok(e)
    }

    /// Comma-separated exprs up to `close`; newlines and a trailing comma allowed.
    fn list(&mut self, close: Tok, open: &Span) -> PResult<Vec<Expr>> {
        let mut items = Vec::new();
        self.skip_nl();
        while *self.peek() != close {
            items.push(self.item(0)?);
            self.skip_nl();
            if *self.peek() != Tok::Comma {
                break;
            }
            self.bump();
            self.skip_nl();
        }
        self.close(close, open)?;
        Ok(items)
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
        let f = Expr { span: body.span.clone(), kind: ExprKind::Fn(Rc::new(FnDef { params: vec![Pat::Name(var)], body, src: fn_src })) };
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

/// The left side of `[a, [b, _]] = xs` as a pattern.
fn to_pat(e: &Expr) -> PResult<Pat> {
    match &e.kind {
        ExprKind::Ident(n) if n == "_" => Ok(Pat::Skip),
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
