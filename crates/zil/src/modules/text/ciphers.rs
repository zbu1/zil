//! Classic ciphers and codes: Caesar, ROT13, Atbash, Vigenère, Morse, Braille, NATO spelling. For fun, not secrecy.

use crate::modules::Module;

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
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{BRAILLE, DIGITS, LETTERS, MORSE, NATO, map_letters, shift};

    /// the Caesar cipher: move each letter shift places along the alphabet, wrapping from z back to a
    ///
    /// A negative shift moves back, undoing it
    #[builtin(examples = [r#""hello".caesar(3)"#, r#""khoor".caesar(-3)"#], see = ["rot13", "vigenere"])]
    fn caesar(s: &str, shift: i64) -> String {
        super::shift(s, |_| shift)
    }

    /// move each letter 13 places along the alphabet
    ///
    /// Since that's half of 26, doing it again gives back the original
    #[builtin(examples = [r#""hello".rot13"#], see = ["caesar"])]
    fn rot13(s: &str) -> String {
        shift(s, |_| 13)
    }

    /// swap each letter with its mirror in the alphabet: a with z, b with y, and so on
    #[builtin(examples = [r#""wizard".atbash"#], see = ["caesar"])]
    fn atbash(s: &str) -> String {
        map_letters(s, |i| 25 - i)
    }

    fn keyed(s: &str, key: &str, sign: i64) -> Result<String, String> {
        let key: Vec<i64> = key.chars().filter(char::is_ascii_alphabetic).map(|c| (c.to_ascii_lowercase() as u8 - b'a') as i64).collect();
        if key.is_empty() {
            return Err("key needs at least one letter".into());
        }
        Ok(shift(s, |n| sign * key[n % key.len()]))
    }

    /// the Vigenère cipher: a Caesar shift that changes with each letter, taken in turn from the letters of the key (a shifts by 0, b by 1, ...)
    #[builtin(examples = [r#""attack at dawn".vigenere("lemon")"#], see = ["unvigenere", "caesar"])]
    fn vigenere(s: &str, key: &str) -> Result<String, String> {
        keyed(s, key, 1)
    }

    /// decrypt text encrypted with vigenere and the same key
    #[builtin(examples = [r#""lxfopv ef rnhr".unvigenere("lemon")"#], see = ["vigenere"])]
    fn unvigenere(s: &str, key: &str) -> Result<String, String> {
        keyed(s, key, -1)
    }

    /// text to Morse code, or Morse code back to text
    ///
    /// Letters are separated by spaces and words by /
    #[builtin(examples = [r#""sos".morse"#, r#"".... .. / - .... . .-. .".morse"#], see = ["braille"])]
    fn morse(s: &str) -> String {
        if s.chars().all(|c| ".-/ ".contains(c)) {
            let word = |w: &str| w.split_whitespace().map(|code| MORSE.iter().find(|m| m.1 == code).map_or('?', |m| m.0)).collect::<String>();
            return s.split('/').map(word).collect::<Vec<_>>().join(" ");
        }
        let word = |w: &str| w.chars().filter_map(|c| MORSE.iter().find(|m| m.0 == c.to_ascii_uppercase()).map(|m| m.1)).collect::<Vec<_>>().join(" ");
        s.split_whitespace().map(word).collect::<Vec<_>>().join(" / ")
    }

    /// text to Braille characters, one per letter (uncontracted Grade 1), or Braille back to text
    #[builtin(examples = [r#""hello world".braille"#, r#""⠓⠊".braille"#], see = ["morse", "nato"])]
    fn braille(s: &str) -> String {
        s.chars()
            .map(|c| match BRAILLE.chars().position(|b| b == c) {
                Some(i) => LETTERS.as_bytes()[i] as char,
                None => LETTERS.find(c.to_ascii_lowercase()).map_or(c, |i| BRAILLE.chars().nth(i).unwrap_or(c)),
            })
            .collect()
    }

    /// spell text out in the NATO phonetic alphabet (Alfa, Bravo, Charlie, ...), as used over the phone or radio
    #[builtin(examples = [r#""SOS".nato"#, r#""b2b".nato"#], see = ["morse"])]
    fn nato(s: &str) -> String {
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
