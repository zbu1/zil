//! Classic ciphers and codes: Caesar, ROT13, Atbash, Vigenère, Morse, Braille, NATO spelling. For fun, not secrecy.

use crate::modules::text::encoding::english;
use crate::modules::{Fail, Module};

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
            ("or let it rank the shifts", r#""Wkh hdjoh odqgv".caesar_solve[0].text"#),
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
    use super::{BRAILLE, DIGITS, Fail, LETTERS, MORSE, NATO, english, inverse26, map_letters, rails, shift, xor_key};
    use crate::signature::{As, Map};
    use crate::value::{Table, Value};

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

    /// every Caesar shift of s, the most English-looking first, as a table of shift and text
    ///
    /// shift is the one that encrypted it, so each text is s.caesar(-shift)
    #[builtin(examples = [r#""Wkh hdjoh odqgv".caesar_solve[0]"#], see = ["caesar", "xor_solve"])]
    fn caesar_solve(s: &str) -> Table {
        let mut rows: Vec<(i64, String)> = (0..26).map(|k| (k, shift(s, |_| -k))).collect();
        rows.sort_by(|a, b| english(b.1.as_bytes()).total_cmp(&english(a.1.as_bytes())));
        Table { cols: vec!["shift".into(), "text".into()], rows: rows.into_iter().map(|(k, t)| vec![Value::int(k), Value::str(t)]).collect() }
    }

    /// find the repeating key that bytes were XORed with, assuming the plain text is English, and return {key, text}
    ///
    /// Guesses the key length from the Hamming distance between blocks, then each key byte from letter frequencies. text is a string when it's valid UTF-8, else bytes. Needs a few lines of text per key byte to be reliable
    #[builtin(examples = [r#"xor_solve(xor(b"Burning 'em, if you ain't quick and nimble", "ICE"))"#], see = ["xor", "hamming", "caesar_solve"])]
    fn xor_solve(b: &[u8]) -> Result<As<Map>, Fail> {
        if b.is_empty() {
            return Err(Fail::Arg(0, "nothing to solve".into()));
        }
        let key = xor_key(b);
        let text: Vec<u8> = b.iter().zip(key.iter().cycle()).map(|(x, k)| x ^ k).collect();
        let text = String::from_utf8(text).map_or_else(|e| Value::bytes(e.into_bytes()), Value::str);
        Ok(Value::map([("key".to_string(), Value::bytes(key)), ("text".to_string(), text)].into_iter().collect()).into())
    }

    /// the affine cipher: letter number x (a = 0) becomes a × x + b, wrapping around the alphabet
    ///
    /// a must share no factor with 26, so that two letters never land on the same one
    #[builtin(examples = [r#""affine cipher".affine(5, 8)"#], see = ["unaffine", "caesar"])]
    fn affine(s: &str, a: i64, b: i64) -> Result<String, Fail> {
        inverse26(a)?;
        Ok(map_letters(s, |x| (a.rem_euclid(26) * x as i64 + b).rem_euclid(26) as u8))
    }

    /// decrypt text encrypted with affine and the same a and b
    #[builtin(examples = [r#""ihhwvc swfrcp".unaffine(5, 8)"#], see = ["affine"])]
    fn unaffine(s: &str, a: i64, b: i64) -> Result<String, Fail> {
        let inv = inverse26(a)?;
        Ok(map_letters(s, |x| (inv * (x as i64 - b.rem_euclid(26))).rem_euclid(26) as u8))
    }

    /// the rail fence cipher: write s in a zigzag down and up n rails, then read it off one rail at a time
    #[builtin(examples = [r#""we are discovered".rail_fence(3)"#], see = ["unrail_fence"])]
    fn rail_fence(s: &str, n: i64) -> Result<String, Fail> {
        let c: Vec<char> = s.chars().collect();
        Ok(rails(c.len(), n)?.into_iter().map(|i| c[i]).collect())
    }

    /// decrypt text encrypted with rail_fence and the same number of rails
    #[builtin(examples = [r#""wrivdeaedsoeec r".unrail_fence(3)"#], see = ["rail_fence"])]
    fn unrail_fence(s: &str, n: i64) -> Result<String, Fail> {
        let c: Vec<char> = s.chars().collect();
        let mut out = vec![' '; c.len()];
        for (j, i) in rails(c.len(), n)?.into_iter().enumerate() {
            out[i] = c[j];
        }
        Ok(out.into_iter().collect())
    }

    /// Bacon's cipher: each letter as five A's and B's, its place in the alphabet in binary (a = AAAAA, b = AAAAB, ...)
    ///
    /// Anything that isn't a letter is dropped
    #[builtin(examples = [r#""hi".bacon"#], see = ["unbacon", "morse"])]
    fn bacon(s: &str) -> String {
        let code = |c: u8| (0..5).rev().map(|i| if (c - b'a') >> i & 1 == 0 { 'A' } else { 'B' }).collect::<String>();
        s.bytes().filter(u8::is_ascii_alphabetic).map(|c| code(c.to_ascii_lowercase())).collect::<Vec<_>>().join(" ")
    }

    /// decode Bacon's cipher: the A's and B's in s, in groups of five, back to letters
    ///
    /// Upper and lower case count the same and other characters are skipped. A group past z decodes as ?
    #[builtin(examples = [r#""AABBB ABAAA".unbacon"#], see = ["bacon"])]
    fn unbacon(s: &str) -> String {
        let bits: Vec<u8> = s
            .chars()
            .filter_map(|c| match c.to_ascii_lowercase() {
                'a' => Some(0),
                'b' => Some(1),
                _ => None,
            })
            .collect();
        bits.as_chunks::<5>().0.iter().map(|g| g.iter().fold(0, |n, b| n * 2 + b)).map(|n| if n < 26 { (b'a' + n) as char } else { '?' }).collect()
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

/// The repeating XOR key that makes `b` look most like English, solved a byte at a time for each key length.
fn xor_key(b: &[u8]) -> Vec<u8> {
    let xored = |b: &[u8], key: &[u8]| b.iter().zip(key.iter().cycle()).map(|(x, k)| x ^ k).collect::<Vec<u8>>();
    let solve = |n: usize| -> Vec<u8> {
        let column = |i: usize| {
            let col: Vec<u8> = b.iter().skip(i).step_by(n).copied().collect();
            (0..=255u8).max_by(|&x, &y| english(&xored(&col, &[x])).total_cmp(&english(&xored(&col, &[y])))).unwrap_or(0)
        };
        (0..n).map(column).collect()
    };
    // Longer keys fit the text a little better by chance, so the shortest one that does nearly as well as the best wins.
    let keys: Vec<(Vec<u8>, f64)> = (1..=(b.len() / 8).clamp(1, 40)).map(solve).map(|k| (english(&xored(b, &k)), k)).map(|(s, k)| (k, s)).collect();
    let best = keys.iter().map(|k| k.1).fold(f64::MIN, f64::max);
    let key = keys.into_iter().find(|k| k.1 >= best - best.abs() * 0.05).map(|k| k.0).unwrap_or_default();
    let period = (1..key.len()).find(|&p| key.len() % p == 0 && key.iter().zip(key.iter().cycle().skip(p)).all(|(x, y)| x == y)).unwrap_or(key.len());
    key[..period].to_vec()
}

/// The order a rail fence with `n` rails reads `len` characters in.
fn rails(len: usize, n: i64) -> Result<Vec<usize>, Fail> {
    let n = usize::try_from(n).ok().filter(|n| *n >= 2).ok_or_else(|| Fail::Arg(1, format!("there must be at least 2 rails, got {n}")))?;
    let cycle = 2 * (n - 1);
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by_key(|i| (i % cycle).min(cycle - i % cycle));
    Ok(order)
}

/// The x with a × x = 1 mod 26, which the affine cipher needs to decrypt.
fn inverse26(a: i64) -> Result<i64, Fail> {
    (1..26)
        .find(|x| a.rem_euclid(26) * x % 26 == 1)
        .ok_or_else(|| Fail::Arg(1, format!("a must share no factor with 26, so be odd and not a multiple of 13, got {a}")))
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
        assert_eq!(show(r#""affine cipher".affine(5, 8)"#), "ihhwvc swfrcp");
        assert_eq!(show(r#""Ihhwvc".unaffine(5, 8)"#), "Affine");
        assert!(try_eval(r#""x".affine(13, 1)"#).is_err());
        assert_eq!(show(r#""WEAREDISCOVEREDFLEEATONCE".rail_fence(3)"#), "WECRLTEERDSOEEFEAOCAIVDEN");
        assert_eq!(show(r#""WECRLTEERDSOEEFEAOCAIVDEN".unrail_fence(3)"#), "WEAREDISCOVEREDFLEEATONCE");
        assert!(try_eval(r#""x".rail_fence(1)"#).is_err());
        assert_eq!(show(r#""Hi!".bacon"#), "AABBB ABAAA");
        assert_eq!(show(r#""aabbb abaaa bbbbb".unbacon"#), "hi?");
    }

    #[test]
    fn solvers() {
        assert_eq!(show(r#""Wkh hdjoh odqgv dw plgqljkw".caesar_solve[0]"#), r#"{shift: 3, text: "The eagle lands at midnight"}"#);
        let text = "Burning 'em, if you ain't quick and nimble. I go crazy when I hear a cymbal and a hi-hat with a souped up tempo";
        assert_eq!(show(&format!(r#"xor_solve(xor(b"{text}", "ICE")).key"#)), r#"b"ICE""#);
        assert_eq!(show(&format!(r#"xor_solve(xor(b"{text}", 0x5a)).text"#)), text);
        assert!(try_eval(r#"xor_solve(b"")"#).is_err());
    }
}
