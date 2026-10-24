//! Command mode: after a command name at the start of a statement, the rest of the line is shell words.
//! `ls -la src` desugars to `ls("src", {l: true, a: true})`; goals/SHELL.md has the design.

use super::{PResult, Parser, close_brace};
use crate::Error;
use crate::ast::{Expr, ExprKind};
use crate::lexer::{Span, Tok, string_end};

/// Builtins with a command form.
pub const COMMANDS: &[&str] = &["cd", "ls", "pwd", "cat", "grep", "wc", "xxd", "strings", "mkdir", "rm", "mv", "cp"];

enum Word {
    /// An argument and its canonical source.
    Arg(Expr, String),
    /// `-la`: the letters, and where they were.
    Flags(String, Span),
}

impl Parser<'_> {
    /// At a statement's first token: a command line, or `None` when the name is used as a value (`ls(...)`, `ls = 5`).
    pub(super) fn command(&mut self) -> PResult<Option<Expr>> {
        let Tok::Ident(name) = self.peek().clone() else { return Ok(None) };
        if !COMMANDS.contains(&name.as_str()) {
            return Ok(None);
        }
        let (at, span) = (self.pos, self.span());
        let next = self.peek_at(1).clone();
        let touching = self.toks[at + 1].1.start == span.end;
        let ident = Expr { kind: ExprKind::Ident(name.clone()), span: span.clone() };
        // `ls` and `ls |> f` call it with no arguments.
        if matches!(next, Tok::Newline | Tok::Eof | Tok::RBrace | Tok::RParen | Tok::RBracket | Tok::Pipe) {
            self.bump();
            let call = Expr { kind: ExprKind::Call(Box::new(ident.clone()), vec![]), span: span.clone() };
            self.commands.push((span.clone(), Ok(format!("{name}()"))));
            return Ok(Some(Expr { kind: ExprKind::Command(name, Ok(Box::new(call)), Ok(Box::new(ident))), span }));
        }
        let value = matches!(
            next,
            Tok::Assign
                | Tok::OpAssign(_)
                | Tok::Comma
                | Tok::Eq
                | Tok::Ne
                | Tok::Le
                | Tok::Ge
                | Tok::And
                | Tok::Or
                | Tok::In
                | Tok::To
                | Tok::Of
                | Tok::Mod
                | Tok::Colon
        );
        if value || (touching && matches!(next, Tok::LParen | Tok::LBracket | Tok::Dot | Tok::Bang)) {
            return Ok(None);
        }
        let text = &self.src[span.end..span.end + shell_len(&self.src[span.end..])];
        let end = span.end + text.trim_end().len();
        let stop = (at..self.toks.len()).find(|&i| self.toks[i].1.start >= end).unwrap_or(self.toks.len() - 1);

        // The ordinary meaning, for when the name is a variable: `cat foo` with `cat = 5` is `5 foo`.
        let (holes, outer) = (self.holes, std::mem::replace(&mut self.stop, stop));
        self.stmt_start = usize::MAX;
        let alt = self.expr(0).and_then(|e| if self.pos == stop { Ok(e) } else { Err(self.unexpected(self.pos, "a newline or `;` between statements")) });
        (self.holes, self.stop, self.pos, self.prev_end) = (holes, outer, stop, end);

        let call = self.words(span.end, end).map(|words| {
            let (mut args, mut canon, mut flags, mut flag_span) = (vec![], vec![], String::new(), None);
            for w in words {
                match w {
                    Word::Arg(e, c) => {
                        args.push(e);
                        canon.push(c);
                    }
                    Word::Flags(letters, at) => {
                        flags.extend(letters.chars().filter(|c| !flags.contains(*c)).collect::<Vec<_>>());
                        flag_span.get_or_insert(at);
                    }
                }
            }
            if let Some(at) = flag_span {
                let t = |c: char| (c.to_string(), Expr { kind: ExprKind::Bool(true), span: at.clone() });
                args.push(Expr { kind: ExprKind::Map(flags.chars().map(t).collect()), span: at });
                canon.push(format!("{{{}}}", flags.chars().map(|c| format!("{c}: true")).collect::<Vec<_>>().join(", ")));
            }
            (Expr { kind: ExprKind::Call(Box::new(ident), args), span: span.start..end }, format!("{name}({})", canon.join(", ")))
        });
        if let (Err(e), Err(_)) = (&call, &alt) {
            return Err(e.clone());
        }
        let whole = span.start..end;
        self.commands.push((whole.clone(), call.as_ref().map(|c| c.1.clone()).map_err(Clone::clone)));
        let kind = ExprKind::Command(name, call.map(|c| Box::new(c.0)).map_err(Box::new), alt.map(Box::new).map_err(Box::new));
        Ok(Some(Expr { kind, span: whole }))
    }

    /// The shell words in `src[from..to]`.
    fn words(&self, from: usize, to: usize) -> PResult<Vec<Word>> {
        let s = &self.src[..to];
        let b = s.as_bytes();
        let space = |i: usize| i == to || matches!(b[i], b' ' | b'\t' | b'\r' | b'\n');
        let word_end = |mut i: usize| {
            while !space(i) && !matches!(b[i], b'|' | b'<' | b'>') {
                i += 1;
            }
            i
        };
        let (mut i, mut out) = (from, vec![]);
        loop {
            while i < to && space(i) {
                i += 1;
            }
            if i == to {
                return Ok(out);
            }
            let start = i;
            let word = match b[i] {
                b'"' => {
                    let close = i + 1 + string_end(&s[i + 1..]).ok_or_else(|| Error::new("unclosed string", start..start + 1))?;
                    i = close + 1;
                    Word::Arg(Expr { kind: self.string(&s[start + 1..close], start + 1)?, span: start..i }, s[start..i].to_string())
                }
                b'r' if b.get(i + 1) == Some(&b'"') => {
                    let close = i + 2 + s[i + 2..].find('"').ok_or_else(|| Error::new("unclosed string", start..start + 2))?;
                    i = close + 1;
                    Word::Arg(Expr { kind: ExprKind::Str(s[start + 2..close].into()), span: start..i }, s[start..i].to_string())
                }
                b'{' => {
                    let close = close_brace(s, i + 1).ok_or_else(|| Error::new("unclosed `{`", start..start + 1))?;
                    let inner = &s[i + 1..close];
                    i = close + 1;
                    if top_comma(inner) {
                        return Err(Error::new("brace expansion isn't supported", start..i).help("to pass several arguments, give a list: `{[\"a\", \"b\"]}`"));
                    }
                    if inner.trim().is_empty() {
                        return Err(Error::new("empty `{}` in a command", start..i).help("put an expression inside: `{path}`"));
                    }
                    Word::Arg(self.sub_expr(inner, start + 1)?, inner.trim().to_string())
                }
                b'\'' => return Err(Error::new("single quotes don't make strings", start..start + 1).help("write `\"...\"`, or `r\"...\"` for raw text")),
                b'$' => {
                    let var = &s[i + 1..word_end(i)];
                    let var = if var.is_empty() { "NAME" } else { var };
                    return Err(Error::new("`$` is the dollar sign, not a variable", start..start + 1)
                        .help(format!("for an environment variable, write `{{env(\"{var}\")}}`")));
                }
                b'|' | b'<' | b'>' => {
                    return Err(Error::new("pipes and redirection between commands aren't supported yet", start..start + 1)
                        .help("use `|>` to pass the result to zil: `ls src |> where(|f| f.size > 1 KB)`"));
                }
                b'(' => return Err(Error::new("`(` can't start a command word", start..start + 1).help("to pass a value, write `{expr}`")),
                b'-' => {
                    i = word_end(i);
                    let letters = &s[start + 1..i];
                    if letters.is_empty() {
                        let rest = s[i..].trim_start();
                        if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                            let flag = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
                            return Err(Error::new("flags touch their dash", start..start + 1).help(format!("write `-{flag}`")));
                        }
                        Word::Arg(Expr { kind: ExprKind::Str("-".into()), span: start..i }, "\"-\"".into())
                    } else if !letters.bytes().all(|c| c.is_ascii_alphabetic()) {
                        return Err(
                            Error::new("flags are single letters, like `-la`", start..i).help(format!("to pass `{}` as an argument, quote it", &s[start..i]))
                        );
                    } else {
                        Word::Flags(letters.into(), start..i)
                    }
                }
                _ => {
                    i = word_end(i);
                    let w = &s[start..i];
                    if let Some(k) = w.find(['"', '{', '}', '\'']) {
                        return Err(Error::new("quote the whole word", start + k..start + k + 1).help("write `\"out_{i}.txt\"` to build a word from values"));
                    }
                    let lit = Expr { kind: ExprKind::Str(w.into()), span: start..i };
                    if w.contains(['*', '?']) {
                        // `*.rs` is `*glob("*.rs").name`: expanded natively, never by a shell.
                        let at = |kind| Expr { kind, span: start..i };
                        let glob = at(ExprKind::Call(Box::new(at(ExprKind::Ident("glob".into()))), vec![lit]));
                        Word::Arg(at(ExprKind::Spread(Box::new(at(ExprKind::Field(Box::new(glob), "name".into()))))), format!("*glob({}).name", quote(w)))
                    } else {
                        Word::Arg(lit, quote(w))
                    }
                }
            };
            if !space(i) && !matches!(b[i], b'|' | b'<' | b'>') {
                return Err(Error::new("a quoted word ends at a space", i..i + 1).help("quote the whole word: `\"out_{i}.txt\"`"));
            }
            out.push(word);
        }
    }
}

/// How much of `s`, the text after a command name, is its words: up to a newline, `;`, `#`, `|>` or an unmatched closer.
fn shell_len(s: &str) -> usize {
    let b = s.as_bytes();
    let (mut i, mut depth) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'"' => match string_end(&s[i + 1..]) {
                Some(n) => i += n + 1,
                None => return b.len(),
            },
            b'r' if b.get(i + 1) == Some(&b'"') && (i == 0 || b[i - 1].is_ascii_whitespace()) => match s[i + 2..].find('"') {
                Some(n) => i += n + 2,
                None => return b.len(),
            },
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => return i,
            b')' | b']' | b'}' => depth -= 1,
            b'\n' | b';' | b'#' if depth == 0 => return i,
            b'|' if depth == 0 && b.get(i + 1) == Some(&b'>') => return i,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// A `,` outside brackets and strings: `{a,b}` is bash brace expansion.
fn top_comma(s: &str) -> bool {
    let b = s.as_bytes();
    let (mut i, mut depth) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'"' => i += string_end(&s[i + 1..]).map_or(b.len(), |n| n + 1),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// `w` as a zil string literal.
fn quote(w: &str) -> String {
    let mut out = String::from('"');
    for c in w.chars() {
        if matches!(c, '\\' | '"' | '{' | '}') {
            out.push('\\');
        }
        out.push(c);
    }
    out + "\""
}

#[cfg(test)]
mod tests {
    use super::super::{desugar, parse};

    #[test]
    fn desugars_words_and_flags() {
        for (src, want) in [
            ("ls", "ls()"),
            ("ls -la src", r#"ls("src", {l: true, a: true})"#),
            ("ls -l src -al", r#"ls("src", {l: true, a: true})"#),
            ("cd ../foo", r#"cd("../foo")"#),
            (r#"cat "my file.txt" r"C:\raw" C:\bare"#, r#"cat("my file.txt", r"C:\raw", "C:\\bare")"#),
            ("rm *.tmp {old}", r#"rm(*glob("*.tmp").name, old)"#),
            ("cd -", r#"cd("-")"#),
            ("ls src |> len", r#"ls("src") |> len"#),
            ("ls |> len", "ls() |> len"),
            ("if true { ls src }", r#"if true { ls("src") }"#),
            ("ls src # all of it", r#"ls("src") # all of it"#),
            ("x = ls(\"src\")\nls = 5\nls.str\nls(1)", "x = ls(\"src\")\nls = 5\nls.str\nls(1)"),
            ("5 cd", "5 cd"),
        ] {
            assert_eq!(desugar(src).unwrap(), want, "{src}");
        }
    }

    #[test]
    fn bash_habits_error() {
        for (src, msg) in [
            ("ls 'src'", "single quotes"),
            ("ls {a,b}", "brace expansion"),
            ("ls $HOME", "dollar sign"),
            ("ls - l", "touch their dash"),
            ("ls --all", "single letters"),
            ("ls src | grep rs", "pipes"),
            ("ls out_{i}.txt", "quote the whole word"),
        ] {
            let e = desugar(src).unwrap_err();
            assert!(e.msg.contains(msg), "{src}: {}", e.msg);
        }
    }

    #[test]
    fn ordinary_meaning_kept_when_words_fail() {
        // `ls - l` errors as a command but parses as subtraction, so a variable named `ls` still works.
        assert!(parse("ls - l").is_ok());
        assert!(parse("ls 'x'").is_err());
    }
}
