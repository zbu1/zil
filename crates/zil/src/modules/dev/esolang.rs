//! Esoteric languages: Brainfuck and Ook! interpreters, as CTF puzzles hide text in them.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Fail, Module};
use crate::signature::Or;

pub const MODULE: Module = Module {
    name: "esolang",
    about: "run Brainfuck and Ook! programs, joke languages that CTF puzzles hide text in",
    #[rustfmt::skip]
    examples: &[
        ("esolang", &[
            ("print a letter", r#"brainfuck("++++++++[>++++++++<-]>+.")"#),
            ("echo the input back", r#"brainfuck(",[.,]", "hi")"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{ook_to_bf, run};
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::modules::Fail;
    use crate::signature::Or;

    /// run a Brainfuck program and return what it prints
    ///
    /// The tape has 30000 cells of one byte each, which wrap from 255 to 0. `,` reads the next byte of input, or 0 once it runs out. Characters other than the eight commands are comments. The output is a string when it's valid UTF-8, else bytes
    #[builtin(examples = [r#"brainfuck("++++++++[>++++++++<-]>+.+.")"#, r#"brainfuck(",[.,]", "echo")"#], see = ["ook"])]
    fn brainfuck(it: &mut Interp, src: &str, input: Option<Or<&str, &[u8]>>, span: &Span) -> Result<Or<String, Vec<u8>>, Fail> {
        run(it, src.as_bytes(), input, span)
    }

    /// run an Ook! program, Brainfuck written with the words Ook. Ook? and Ook!, and return what it prints
    #[builtin(examples = [r#"ook("Ook. Ook. Ook. Ook. Ook! Ook. Ook. Ook?")"#], see = ["brainfuck"])]
    fn ook(it: &mut Interp, src: &str, input: Option<Or<&str, &[u8]>>, span: &Span) -> Result<Or<String, Vec<u8>>, Fail> {
        run(it, &ook_to_bf(src)?, input, span)
    }
}

/// Each pair of `Ook` punctuation marks as its Brainfuck command.
fn ook_to_bf(src: &str) -> Result<Vec<u8>, Fail> {
    let marks: Vec<u8> = src.match_indices("Ook").filter_map(|(i, _)| src.as_bytes().get(i + 3).copied()).filter(|c| b".?!".contains(c)).collect();
    if marks.len() % 2 == 1 {
        return Err(Fail::Arg(0, "Ook! words come in pairs, but there's an odd one".into()));
    }
    marks
        .chunks(2)
        .map(|p| match p {
            b".?" => Ok(b'>'),
            b"?." => Ok(b'<'),
            b".." => Ok(b'+'),
            b"!!" => Ok(b'-'),
            b"!." => Ok(b'.'),
            b".!" => Ok(b','),
            b"!?" => Ok(b'['),
            b"?!" => Ok(b']'),
            _ => Err(Fail::Arg(0, "Ook? Ook? isn't a command".into())),
        })
        .collect()
}

/// Runs Brainfuck, one step per command against the session's limits.
fn run(it: &mut Interp, src: &[u8], input: Option<Or<&str, &[u8]>>, span: &Span) -> Result<Or<String, Vec<u8>>, Fail> {
    let code: Vec<u8> = src.iter().copied().filter(|c| b"<>+-.,[]".contains(c)).collect();
    let mut jump = vec![0; code.len()];
    let mut open = vec![];
    for (i, &c) in code.iter().enumerate() {
        match c {
            b'[' => open.push(i),
            b']' => {
                let j = open.pop().ok_or_else(|| Fail::Arg(0, "a ] has no [ before it".into()))?;
                (jump[i], jump[j]) = (j, i);
            }
            _ => {}
        }
    }
    if !open.is_empty() {
        return Err(Fail::Arg(0, "a [ is never closed".into()));
    }
    let input = match input {
        Some(Or::A(s)) => s.as_bytes(),
        Some(Or::B(b)) => b,
        None => &[],
    };
    let (mut tape, mut ptr, mut pc, mut out, mut read) = (vec![0u8; 30000], 0usize, 0, Vec::new(), input.iter());
    while pc < code.len() {
        it.charge(1, span)?;
        match code[pc] {
            b'>' => ptr += 1,
            b'<' => ptr = ptr.checked_sub(1).ok_or_else(|| Fail::Arg(0, "moved left of the first cell".into()))?,
            b'+' => tape[ptr] = tape[ptr].wrapping_add(1),
            b'-' => tape[ptr] = tape[ptr].wrapping_sub(1),
            b'.' => {
                out.push(tape[ptr]);
                crate::state::check_len(out.len(), "bytes").map_err(|e| Fail::Arg(0, e))?;
            }
            b',' => tape[ptr] = read.next().copied().unwrap_or(0),
            b'[' if tape[ptr] == 0 => pc = jump[pc],
            b']' if tape[ptr] != 0 => pc = jump[pc],
            _ => {}
        }
        if ptr == tape.len() {
            return Err(Fail::Arg(0, "moved right of the last cell".into()));
        }
        pc += 1;
    }
    Ok(String::from_utf8(out).map_or_else(|e| Or::B(e.into_bytes()), Or::A))
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn brainfuck() {
        let hello = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.";
        assert_eq!(show(&format!("brainfuck({hello:?})")), "Hello World!");
        assert_eq!(show(r#"brainfuck(",[.,]", "echo")"#), "echo");
        assert_eq!(show(r#"brainfuck("-.")"#), r#"b"\xff""#);
        assert_eq!(show(r#"ook("Ook. Ook! Ook! Ook.", "x")"#), "x");
        assert!(try_eval(r#"brainfuck("[")"#).is_err());
        assert!(try_eval(r#"brainfuck("]")"#).is_err());
        assert!(try_eval(r#"brainfuck("<")"#).is_err());
        assert!(try_eval(r#"ook("Ook.")"#).is_err());
    }
}
