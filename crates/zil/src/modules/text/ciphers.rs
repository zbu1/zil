//! Classic ciphers and codes: Caesar, ROT13, Atbash, Vigenère, Morse, Braille, NATO spelling. For fun, not secrecy.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::Value;

pub const MODULE: Module = Module {
    name: "ciphers",
    about: "classic letter-shifting ciphers, Morse code, Braille and the NATO spelling alphabet",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Classic ciphers and alphabets, for puzzles and fun: letter-shifting ciphers, Morse code, Braille and the NATO \
alphabet."),
        ("good to know", "\
None of these ciphers is secure, and all can be broken in seconds. `caesar`, `rot13`, `atbash` and `vigenere` \
change only letters, and leave spaces, digits and punctuation as they are."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("ciphers", &[
            ("SOS in Morse", r#""sos".morse"#),
            ("and back", r#""... --- ...".morse"#),
            ("hide a spoiler", r#""the butler did it".rot13"#),
            ("crack a Caesar cipher by trying every shift", r#"(1..26).map(|k| "Wkh hdjoh odqgv".caesar(-k)).grep("eagle")"#),
            ("encrypt with Vigenère and back", r#""attack at dawn".vigenere("lemon").unvigenere("lemon")"#),
            ("in Braille", r#""hello".braille"#),
            ("spell it over the phone", r#""zil 2".nato"#),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("caesar", "caesar(s: str, shift: int) -> str", "the Caesar cipher: move each letter shift places along the alphabet, wrapping from z back to a", &[r#""hello".caesar(3)"#, r#""khoor".caesar(-3)"#], &["rot13", "vigenere"])
        .detail("A negative shift moves back, undoing it"),
    doc("rot13", "rot13(s: str) -> str", "move each letter 13 places along the alphabet", &[r#""hello".rot13"#], &["caesar"])
        .detail("Since that's half of 26, doing it again gives back the original"),
    doc("atbash", "atbash(s: str) -> str", "swap each letter with its mirror in the alphabet: a with z, b with y, and so on", &[r#""wizard".atbash"#], &["caesar"]),
    doc("vigenere", "vigenere(s: str, key: str) -> str", "the Vigenère cipher: a Caesar shift that changes with each letter, taken in turn from the letters of the key (a shifts by 0, b by 1, ...)", &[r#""attack at dawn".vigenere("lemon")"#], &["unvigenere", "caesar"]),
    doc("unvigenere", "unvigenere(s: str, key: str) -> str", "decrypt text encrypted with vigenere and the same key", &[r#""lxfopv ef rnhr".unvigenere("lemon")"#], &["vigenere"]),
    doc("morse", "morse(s: str) -> str", "text to Morse code, or Morse code back to text", &[r#""sos".morse"#, r#"".... .. / - .... . .-. .".morse"#], &["braille"])
        .detail("Letters are separated by spaces and words by /"),
    doc("braille", "braille(s: str) -> str", "text to Braille characters, one per letter (uncontracted Grade 1), or Braille back to text", &[r#""hello world".braille"#, r#""⠓⠊".braille"#], &["morse", "nato"]),
    doc("nato", "nato(s: str) -> str", "spell text out in the NATO phonetic alphabet (Alfa, Bravo, Charlie, ...), as used over the phone or radio", &[r#""SOS".nato"#, r#""b2b".nato"#], &["morse"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(Value::str(match (name, args) {
        ("caesar", [Str(s), Int(k, _)]) => shift(s, |_| *k),
        ("rot13", [Str(s)]) => shift(s, |_| 13),
        ("atbash", [Str(s)]) => map_letters(s, |i| 25 - i),
        ("vigenere" | "unvigenere", [Str(s), Str(key)]) => {
            let key: Vec<i64> = key.chars().filter(char::is_ascii_alphabetic).map(|c| (c.to_ascii_lowercase() as u8 - b'a') as i64).collect();
            if key.is_empty() {
                return Err("key needs at least one letter".into());
            }
            let sign = if name == "vigenere" { 1 } else { -1 };
            shift(s, |n| sign * key[n % key.len()])
        }
        ("morse", [Str(s)]) if s.chars().all(|c| ".-/ ".contains(c)) => {
            let word = |w: &str| w.split_whitespace().map(|code| MORSE.iter().find(|m| m.1 == code).map_or('?', |m| m.0)).collect::<String>();
            s.split('/').map(word).collect::<Vec<_>>().join(" ")
        }
        ("morse", [Str(s)]) => {
            let word = |w: &str| w.chars().filter_map(|c| MORSE.iter().find(|m| m.0 == c.to_ascii_uppercase()).map(|m| m.1)).collect::<Vec<_>>().join(" ");
            s.split_whitespace().map(word).collect::<Vec<_>>().join(" / ")
        }
        ("braille", [Str(s)]) => s
            .chars()
            .map(|c| match BRAILLE.chars().position(|b| b == c) {
                Some(i) => LETTERS.as_bytes()[i] as char,
                None => LETTERS.find(c.to_ascii_lowercase()).map_or(c, |i| BRAILLE.chars().nth(i).unwrap()),
            })
            .collect(),
        ("nato", [Str(s)]) => {
            let words: Vec<&str> = s
                .chars()
                .filter_map(|c| match c.to_ascii_lowercase() {
                    c @ 'a'..='z' => Some(NATO[c as usize - 'a' as usize]),
                    c @ '0'..='9' => Some(DIGITS[c as usize - '0' as usize]),
                    ' ' => Some("/"),
                    _ => None,
                })
                .collect();
            words.join(" ")
        }
        _ => return Err(Fail::BadArgs),
    }))
}

/// Shifts the n-th letter by `by(n)`, keeping case; other characters pass through.
fn shift(s: &str, by: impl Fn(usize) -> i64) -> String {
    let mut n = 0;
    map_letters(s, |i| {
        let k = by(n);
        n += 1;
        (i as i64 + k).rem_euclid(26) as u8
    })
}

fn map_letters(s: &str, mut f: impl FnMut(u8) -> u8) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (b'a' + f(c as u8 - b'a')) as char,
            'A'..='Z' => (b'A' + f(c as u8 - b'A')) as char,
            _ => c,
        })
        .collect()
}

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz ";
const BRAILLE: &str = "⠁⠃⠉⠙⠑⠋⠛⠓⠊⠚⠅⠇⠍⠝⠕⠏⠟⠗⠎⠞⠥⠧⠺⠭⠽⠵⠀";

#[rustfmt::skip]
const MORSE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."), ('F', "..-."), ('G', "--."), ('H', "...."),
    ('I', ".."), ('J', ".---"), ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"), ('P', ".--."),
    ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"), ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"),
    ('Y', "-.--"), ('Z', "--.."),
    ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"), ('5', "....."), ('6', "-...."),
    ('7', "--..."), ('8', "---.."), ('9', "----."),
    ('.', ".-.-.-"), (',', "--..--"), ('?', "..--.."), ('!', "-.-.--"), ('\'', ".----."), ('/', "-..-."), ('(', "-.--."),
    (')', "-.--.-"), ('&', ".-..."), (':', "---..."), ('=', "-...-"), ('+', ".-.-."), ('-', "-....-"), ('"', ".-..-."), ('@', ".--.-."),
];

const NATO: [&str; 26] = [
    "Alfa", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliett", "Kilo", "Lima", "Mike", "November", "Oscar", "Papa", "Quebec",
    "Romeo", "Sierra", "Tango", "Uniform", "Victor", "Whiskey", "X-ray", "Yankee", "Zulu",
];
const DIGITS: [&str; 10] = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine"];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn ciphers() {
        assert_eq!(show(r#""Hello, World!".caesar(3)"#), "Khoor, Zruog!");
        assert_eq!(show(r#""abc".caesar(-1)"#), "zab");
        assert_eq!(show(r#""Hello".rot13.rot13"#), "Hello");
        assert_eq!(show(r#""wizard".atbash"#), "draziw");
        assert_eq!(show(r#""ATTACK AT DAWN".vigenere("LEMON")"#), "LXFOPV EF RNHR");
        assert_eq!(show(r#""lxfopv ef rnhr".unvigenere("lemon")"#), "attack at dawn");
        assert_eq!(show(r#""SOS hi".morse"#), "... --- ... / .... ..");
        assert_eq!(show(r#""... --- ... / .... ..".morse"#), "SOS HI");
        assert_eq!(show(r#""hi there".braille.braille"#), "hi there");
        assert_eq!(show(r#""Hi 5".nato"#), "Hotel India / Five");
        assert!(try_eval(r#""x".vigenere("123")"#).is_err());
    }
}
