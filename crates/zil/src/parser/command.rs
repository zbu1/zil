//! Command mode: after a command name at the start of a statement, the rest of the line is shell words.
//! `ls -la src` desugars to `ls("src", {l: true, a: true})` and `!objdump -d a.out` to `exec(["objdump", "-d", "a.out"])`;
//! goals/SHELL.md has the design.

use super::{PResult, Parser, close_brace};
use crate::Error;
use crate::ast::{Expr, ExprKind};
use crate::lexer::{Span, Tok, string_end};

enum Word {
    /// A command's name, first in each `|` segment, and whether it's an external program.
    Name(String, bool, Span),
    /// An argument and its canonical source.
    Arg(Expr, String),
    /// `-la`: the letters, and where they were.
    Flags(String, Span),
    /// `|`, `<`, `>`, `>>` or `2>`.
    Op(&'static str, Span),
}

fn builtin(name: &str) -> bool {
    crate::modules::FLAGS.iter().any(|f| f.0 == name)
}

impl Parser<'_> {
    /// At a statement's first token: a command line, or `None` when the name is used as a value (`ls(...)`, `ls = 5`).
    pub(super) fn command(&mut self) -> PResult<Option<Expr>> {
        let Tok::Ident(name) = self.peek().clone() else { return Ok(None) };
        if !builtin(&name) {
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

        let call = self.line(span.start, end, true);
        if let (Err(e), Err(_)) = (&call, &alt) {
            return Err(e.clone());
        }
        let whole = span.start..end;
        self.commands.push((whole.clone(), call.as_ref().map(|c| c.1.clone()).map_err(Clone::clone)));
        let kind = ExprKind::Command(name, call.map(|c| Box::new(c.0)).map_err(Box::new), alt.map(Box::new).map_err(Box::new));
        Ok(Some(Expr { kind, span: whole }))
    }

    /// `!prog args`, at a statement's start (`bare`) or after `=`: always an external program.
    pub(super) fn external(&mut self, bare: bool) -> PResult<Expr> {
        let start = self.span().start;
        let end = start + 1 + shell_len(&self.src[start + 1..]);
        let end = start + self.src[start..end].trim_end().len();
        let (e, canon) = self.line(start, end, bare)?;
        self.pos = (self.pos..self.toks.len()).find(|&i| self.toks[i].1.start >= end).unwrap_or(self.toks.len() - 1);
        self.prev_end = end;
        self.commands.push((start..end, Ok(canon)));
        Ok(e)
    }

    /// The command line in `src[from..to]` and its canonical source. Each run of external programs is one `exec`, joined
    /// by OS pipes; a `bare` line, a whole statement, ending in one writes it to the terminal.
    fn line(&self, from: usize, to: usize, bare: bool) -> PResult<(Expr, String)> {
        let bare = bare && !self.src[to..].trim_start().starts_with("|>");
        let at = |kind| Expr { kind, span: from..to };
        let call = |f: &str, args| at(ExprKind::Call(Box::new(at(ExprKind::Ident(f.into()))), args));
        let ext = |seg: &[Word]| matches!(seg[0], Word::Name(_, true, _));
        let mut segs: Vec<Vec<Word>> = vec![];
        // Each `<`, `>`, `>>` or `2>`: its file, the file's source, which segment it was on, and where.
        let mut redirs: Vec<(&str, Expr, String, usize, Span)> = vec![];
        let mut words = self.words(from, to)?.into_iter();
        while let Some(w) = words.next() {
            match w {
                Word::Name(..) => segs.push(vec![w]),
                Word::Op("|", _) => {}
                Word::Op(op, span) => {
                    let Some(Word::Arg(file, c)) = words.next() else { return Err(Error::new(format!("`{op}` needs a file after it"), span)) };
                    let kind = |o: &'static str| if o == ">>" { ">" } else { o };
                    if redirs.iter().any(|r| kind(r.0) == kind(op)) {
                        return Err(Error::new(format!("only one `{op}` per line"), span));
                    }
                    redirs.push((op, file, c, segs.len() - 1, span));
                }
                w => segs.last_mut().expect("a line starts with a name").push(w),
            }
        }
        let last = segs.len() - 1;
        for (op, _, _, seg, span) in &redirs {
            let why = match *op {
                "<" if *seg != 0 => "`<` goes on the first command".into(),
                "<" => continue,
                _ if *seg != last => format!("`{op}` goes on the last command"),
                "2>" if !ext(&segs[last]) => "`2>` is for external programs; builtins report errors themselves".into(),
                _ => continue,
            };
            return Err(Error::new(why, span.clone()));
        }
        let file = |op: &str| redirs.iter().find(|r| r.0 == op);
        let mut acc = None;
        if let Some((_, f, c, ..)) = file("<") {
            let bytes = ext(&segs[0]) || matches!(&segs[0][0], Word::Name(n, ..) if n == "xxd" || n == "strings");
            let read = if bytes { "read_bytes" } else { "read_file" };
            acc = Some((call(read, vec![f.clone()]), format!("{read}({c})")));
        }
        let out = redirs.iter().find(|r| r.0 == ">" || r.0 == ">>");
        let mut segs = segs.into_iter().peekable();
        while let Some(seg) = segs.next() {
            // A builtin takes the piped value as its first argument.
            if !ext(&seg) {
                let (mut e, c) = builtin_call(seg, from..to);
                acc = Some(match acc {
                    None => (e, c),
                    Some((v, vc)) => {
                        let ExprKind::Call(_, args) = &mut e.kind else { unreachable!("a builtin segment is a call") };
                        args.insert(0, v);
                        (e, format!("{vc} |> {c}"))
                    }
                });
                continue;
            }
            let mut run = vec![seg];
            while segs.peek().is_some_and(|s| ext(s)) {
                run.extend(segs.next());
            }
            let (mut args, mut canon) = (vec![], vec![]);
            for seg in run {
                let (mut items, mut cs) = (vec![], vec![]);
                for w in seg {
                    match w {
                        Word::Name(n, _, span) => {
                            cs.push(quote(&n));
                            items.push(Expr { kind: ExprKind::Str(n.into()), span });
                        }
                        Word::Arg(e, c) => {
                            items.push(e);
                            cs.push(c);
                        }
                        Word::Flags(..) | Word::Op(..) => unreachable!("an external's words are all arguments"),
                    }
                }
                args.push(at(ExprKind::List(items)));
                canon.push(format!("[{}]", cs.join(", ")));
            }
            let end = segs.peek().is_none();
            let mut opts = vec![];
            opts.extend(acc.take().map(|(v, c)| ("stdin", v, c)));
            if end && let Some((_, f, c, ..)) = file("2>") {
                opts.push(("stderr", f.clone(), c.clone()));
            }
            if end && bare && out.is_none() {
                opts.push(("tty", at(ExprKind::Bool(true)), "true".into()));
            }
            if !opts.is_empty() {
                canon.push(format!("{{{}}}", opts.iter().map(|o| format!("{}: {}", o.0, o.2)).collect::<Vec<_>>().join(", ")));
                args.push(at(ExprKind::Map(opts.into_iter().map(|o| (o.0.to_string(), o.1)).collect())));
            }
            acc = Some((call("exec", args), format!("exec({})", canon.join(", "))));
        }
        let (e, c) = acc.expect("a line has a command");
        Ok(match out {
            Some((op, f, fc, ..)) => {
                let write = if *op == ">" { "write_file" } else { "append_file" };
                (call(write, vec![f.clone(), e]), format!("{write}({fc}, {c})"))
            }
            None => (e, c),
        })
    }

    /// The shell words in `src[from..to]`, a command line starting with its name.
    fn words(&self, from: usize, to: usize) -> PResult<Vec<Word>> {
        let s = &self.src[..to];
        let b = s.as_bytes();
        let space = |i: usize| i == to || matches!(b[i], b' ' | b'\t' | b'\r' | b'\n');
        let op = |i: usize| i < to && matches!(b[i], b'|' | b'<' | b'>');
        let word_end = |mut i: usize| {
            while !space(i) && !op(i) {
                i += 1;
            }
            i
        };
        let (mut i, mut out, mut ext) = (from, vec![], false);
        loop {
            while i < to && space(i) {
                i += 1;
            }
            if i == to {
                if let Some(Word::Op(op, at)) = out.last() {
                    let what = if *op == "|" { "a command" } else { "a file" };
                    return Err(Error::new(format!("`{op}` needs {what} after it"), at.clone()));
                }
                return Ok(out);
            }
            let start = i;
            let word = if matches!(out.last(), None | Some(Word::Op("|", _))) {
                // A segment's name: `!prog`, or after `|` any name that isn't a builtin command, is an external program.
                let bang = b[i] == b'!';
                i = word_end(i + bang as usize);
                let name = &s[start + bang as usize..i];
                if name.is_empty() {
                    return Err(match bang {
                        true => {
                            Error::new("`!` needs a program after it", start..start + 1).help("`!git status` runs a program; to negate a value, write `!(x)`")
                        }
                        false => Error::new("`|` needs a command after it", start..start + 1),
                    });
                }
                if let Some(k) = name.find('(') {
                    let help = match out.is_empty() {
                        true => format!("`!` before a name runs a program; to negate a value, write `!({}...)`", &name[..k]),
                        false => "use `|>` to pass the result to zil: `ls src |> where(|f| f.size > 1 KB)`".into(),
                    };
                    return Err(Error::new(format!("`{}` isn't a program", &name[..k]), start..i).help(help));
                }
                ext = bang || !builtin(name);
                Word::Name(name.into(), ext, start..i)
            } else {
                match b[i] {
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
                            return Err(
                                Error::new("brace expansion isn't supported", start..i).help("to pass several arguments, give a list: `{[\"a\", \"b\"]}`")
                            );
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
                    b'|' => {
                        i += 1;
                        Word::Op("|", start..i)
                    }
                    b'<' => {
                        i += 1;
                        Word::Op("<", start..i)
                    }
                    b'>' => {
                        i += 1 + (b.get(i + 1) == Some(&b'>')) as usize;
                        Word::Op(if i - start == 2 { ">>" } else { ">" }, start..i)
                    }
                    b'(' => return Err(Error::new("`(` can't start a command word", start..start + 1).help("to pass a value, write `{expr}`")),
                    // An external program's flags are plain arguments: `!objdump -d` passes "-d".
                    b'-' if !ext => {
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
                            return Err(Error::new("flags are single letters, like `-la`", start..i)
                                .help(format!("to pass `{}` as an argument, quote it", &s[start..i])));
                        } else {
                            Word::Flags(letters.into(), start..i)
                        }
                    }
                    _ => {
                        i = word_end(i);
                        let w = &s[start..i];
                        if w == "2" && b.get(i) == Some(&b'>') {
                            i += 1;
                            if b.get(i) == Some(&b'&') {
                                return Err(Error::new("`2>&1` isn't supported", start..i + 1).help("`2> file` writes stderr to a file"));
                            }
                            Word::Op("2>", start..i)
                        } else {
                            if let Some(k) = w.find(['"', '{', '}', '\'']) {
                                return Err(
                                    Error::new("quote the whole word", start + k..start + k + 1).help("write `\"out_{i}.txt\"` to build a word from values")
                                );
                            }
                            let lit = Expr { kind: ExprKind::Str(w.into()), span: start..i };
                            if w.contains(['*', '?']) {
                                // `*.rs` is `*glob("*.rs").name`: expanded natively, never by a shell.
                                let at = |kind| Expr { kind, span: start..i };
                                let glob = at(ExprKind::Call(Box::new(at(ExprKind::Ident("glob".into()))), vec![lit]));
                                let name = at(ExprKind::Field(Box::new(glob), "name".into()));
                                Word::Arg(at(ExprKind::Spread(Box::new(name))), format!("*glob({}).name", quote(w)))
                            } else {
                                Word::Arg(lit, quote(w))
                            }
                        }
                    }
                }
            };
            if !matches!(word, Word::Op(..)) && !space(i) && !op(i) {
                return Err(Error::new("a quoted word ends at a space", i..i + 1).help("quote the whole word: `\"out_{i}.txt\"`"));
            }
            out.push(word);
        }
    }
}

/// A builtin's segment as its call: `ls -la src` is `ls("src", {l: true, a: true})`.
fn builtin_call(seg: Vec<Word>, span: Span) -> (Expr, String) {
    let mut seg = seg.into_iter();
    let Some(Word::Name(name, _, at)) = seg.next() else { unreachable!("a segment starts with its name") };
    let (mut args, mut canon, mut flags, mut flag_span) = (vec![], vec![], String::new(), None);
    for w in seg {
        match w {
            Word::Arg(e, c) => {
                args.push(e);
                canon.push(c);
            }
            Word::Flags(letters, at) => {
                flags.extend(letters.chars().filter(|c| !flags.contains(*c)).collect::<Vec<_>>());
                flag_span.get_or_insert(at);
            }
            Word::Name(..) | Word::Op(..) => unreachable!("names and operators are split out by `line`"),
        }
    }
    // File commands read their files here, so the builtins stay value-first: `grep x f` is `grep(cat("f"), "x")`.
    let (read, keep) = match name.as_str() {
        "grep" => ("cat", 1),
        "wc" => ("cat", 0),
        "xxd" | "strings" => ("read_bytes", 0),
        _ => ("", 0),
    };
    if !read.is_empty() && args.len() > keep {
        let files: Vec<_> = args.drain(keep..).collect();
        let at = files[0].span.start..files[files.len() - 1].span.end;
        let callee = Expr { kind: ExprKind::Ident(read.into()), span: at.clone() };
        args.insert(0, Expr { kind: ExprKind::Call(Box::new(callee), files), span: at });
        let files = canon.drain(keep..).collect::<Vec<_>>().join(", ");
        canon.insert(0, format!("{read}({files})"));
    }
    if let Some(at) = flag_span {
        let t = |c: char| (c.to_string(), Expr { kind: ExprKind::Bool(true), span: at.clone() });
        args.push(Expr { kind: ExprKind::Map(flags.chars().map(t).collect()), span: at });
        canon.push(format!("{{{}}}", flags.chars().map(|c| format!("{c}: true")).collect::<Vec<_>>().join(", ")));
    }
    let ident = Expr { kind: ExprKind::Ident(name.clone()), span: at };
    (Expr { kind: ExprKind::Call(Box::new(ident), args), span }, format!("{name}({})", canon.join(", ")))
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
            ("grep -i err a.log *.txt", r#"grep(cat("a.log", *glob("*.txt").name), "err", {i: true})"#),
            ("grep err", r#"grep("err")"#),
            ("wc -l notes.txt", r#"wc(cat("notes.txt"), {l: true})"#),
            ("strings a.out", r#"strings(read_bytes("a.out"))"#),
            ("!objdump -d a.out", r#"exec(["objdump", "-d", "a.out"], {tty: true})"#),
            ("x = !git log --oneline -3", r#"x = exec(["git", "log", "--oneline", "-3"])"#),
            ("!rm {old} *.o", r#"exec(["rm", old, *glob("*.o").name], {tty: true})"#),
            ("ls src | grep rs", r#"ls("src") |> grep("rs")"#),
            ("ls src | grep rs |> len", r#"ls("src") |> grep("rs") |> len"#),
            ("!objdump -d a.out | grep call", r#"exec(["objdump", "-d", "a.out"]) |> grep("call")"#),
            ("!a -x | b | c", r#"exec(["a", "-x"], ["b"], ["c"], {tty: true})"#),
            ("!ls |> len", r#"exec(["ls"]) |> len"#),
            ("ls | sort |> len", r#"exec(["sort"], {stdin: ls()}) |> len"#),
            ("ls | sort | !uniq", r#"exec(["sort"], ["uniq"], {stdin: ls(), tty: true})"#),
            ("ls > files.txt", r#"write_file("files.txt", ls())"#),
            ("!make 2> err.txt >> log.txt", r#"append_file("log.txt", exec(["make"], {stderr: "err.txt"}))"#),
            ("grep x < a.log", r#"read_file("a.log") |> grep("x")"#),
            ("xxd < a.out", r#"read_bytes("a.out") |> xxd()"#),
            ("!sort < names.txt", r#"exec(["sort"], {stdin: read_bytes("names.txt"), tty: true})"#),
            ("!(x)", "!(x)"),
            ("y = !(x)", "y = !(x)"),
            ("1 + !x", "1 + !x"),
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
            ("ls src | where(|f| f.size > 1)", "isn't a program"),
            ("!f(x)", "isn't a program"),
            ("!", "needs a program"),
            ("ls |", "needs a command"),
            ("ls >", "needs a file"),
            ("ls > a > b", "only one"),
            ("ls | grep x < f", "first command"),
            ("ls > f | grep x", "last command"),
            ("ls 2> err", "external programs"),
            ("!make 2>&1", "isn't supported"),
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
