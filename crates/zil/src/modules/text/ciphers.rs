//! Classic ciphers and codes, and the letter statistics used to break them. For fun, not secrecy.

use crate::modules::text::encoding::{Data, FREQ, english, shannon};
use crate::modules::{Fail, Module};
use crate::signature::Map;
use crate::value::Value;
use indexmap::IndexMap;
use std::cmp::Reverse;

pub const MODULE: Module = Module {
    name: "ciphers",
    about: "classic ciphers, Morse code, Braille and the NATO spelling alphabet, plus statistics for breaking ciphers",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Classic ciphers and alphabets, for puzzles and fun: letter-shifting ciphers, Morse code, Braille and the NATO \
alphabet. It also has the statistics codebreakers use on an unknown text: how often each symbol appears, how \
uneven that is, and where patterns repeat, which reveal the kind of cipher and the length of its key."),
        ("good to know", "\
None of these ciphers is secure, and all can be broken in seconds. `caesar`, `rot13`, `atbash` and `vigenere` \
change only letters, and leave spaces, digits and punctuation as they are.

The statistics take a string, a list of byte values or bytes, and treat every symbol as different, so `A` and `a` \
count apart and so do spaces. Clean the text first (with `upper` or `grep`) when that isn't what you want. Their \
reference values are for English and about 26 letters, so they say less about other kinds of data."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("ciphers", &[
            ("SOS in Morse", r#""sos".morse"#),
            ("and back", r#""... --- ...".morse"#),
            ("hide a spoiler", r#""the butler did it".rot13"#),
            ("crack a Caesar cipher by trying every shift", r#"(1..26).map(|k| "Wkh hdjoh odqgv".caesar(-k)).grep("eagle")"#),
            ("or let it rank the shifts", r#""Wkh hdjoh odqgv".caesar_solve[0].text"#),
            ("crack any letter-for-letter substitution", r#""Gur dhvpx oebja sbk whzcf bire gur ynml qbt naq gura eha njnl sebz gur sneznre".substitution_solve.text"#),
            ("encrypt with Vigenère and back", r#""attack at dawn".vigenere("lemon").unvigenere("lemon")"#),
            ("break Vigenère without the key", r#"t = "it was the best of times and it was the worst of times and the people of the city knew that the winter would be long and that the war would not end soon but they gathered in the square every evening to share what little bread they had and to tell stories of the days before"; t.vigenere("lemon").vigenere_solve"#),
            ("how often each letter appears", r#""hello world".freq"#),
            ("size up an unknown text", r#""Wkh hdjoh odqgv dw plgqljkw".analyze"#),
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
    use super::{
        BRAILLE, DIGITS, Data, Fail, LETTERS, MORSE, NATO, avg_ioc, english, english_chi, english_chi_of, gram_len, grams, inverse26, ioc_of, letter_counts,
        map_letters, max_key, rails, shannon, shift, substitution_key, symbol, symbols, tally, xor_key,
    };
    use crate::interp::Interp;
    use crate::lexer::Span;
    use crate::signature::{As, Map};
    use crate::value::{Table, Value};
    use indexmap::IndexMap;
    use std::cmp::Reverse;

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

    /// find the key of a simple substitution cipher (each letter always swapped for the same other letter) by hill climbing, assuming English, and return {key, text}
    ///
    /// key is the 26 letters that a to z of the cipher text stand for. Needs a few sentences of text to be reliable, and may leave a rare letter or two wrong
    #[builtin(examples = [r#""Gur dhvpx oebja sbk whzcf bire gur ynml qbt naq gura eha njnl sebz gur sneznre".substitution_solve.text"#], see = ["caesar_solve", "xor_solve"])]
    fn substitution_solve(it: &mut Interp, s: &str, span: &Span) -> Result<As<Map>, Fail> {
        if !s.bytes().any(|c| c.is_ascii_alphabetic()) {
            return Err(Fail::Arg(0, "nothing to solve".into()));
        }
        let key = substitution_key(s, &mut || it.charge(1, span))?;
        let text = map_letters(s, |i| key[i as usize] - b'a');
        Ok(Value::map(
            [("key".to_string(), Value::str(String::from_utf8_lossy(&key).into_owned())), ("text".to_string(), Value::str(text))].into_iter().collect(),
        )
        .into())
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

    /// how often each symbol appears in s, as a table of symbol, count and percent, the most common first
    ///
    /// Every symbol counts as it is, so `A` and `a` are different. s can be a string, a list of byte values or bytes
    #[builtin(examples = [r#""hello world".freq"#], see = ["ngrams", "ioc", "entropy"])]
    fn freq(s: Data) -> Result<Table, Fail> {
        let (sym, text) = symbols(&s)?;
        let rows = tally(&sym).into_iter().map(|(c, n)| vec![symbol(c, text), Value::int(n as i64), Value::Float(n as f64 * 100.0 / sym.len() as f64)]);
        Ok(Table { cols: vec!["symbol".into(), "count".into(), "percent".into()], rows: rows.collect() })
    }

    /// every run of n symbols in s with how often it appears, as a table of gram and count, the most common first
    ///
    /// n is 2 by default. Runs overlap, so "aaa" has two of "aa"
    #[builtin(examples = [r#""the then there".ngrams(3)"#], see = ["freq", "kasiski"])]
    fn ngrams(s: Data, n: Option<i64>) -> Result<Table, Fail> {
        let (sym, text) = symbols(&s)?;
        let mut rows: Vec<_> = grams(&sym, gram_len(n)?).into_iter().map(|(g, at)| (g, at.len())).collect();
        rows.sort_by_key(|r| Reverse(r.1));
        Ok(Table { cols: vec!["gram".into(), "count".into()], rows: rows.into_iter().map(|(g, n)| vec![super::gram(g, text), Value::int(n as i64)]).collect() })
    }

    /// the index of coincidence of s: the chance that two symbols picked from it at random are the same
    ///
    /// English text and any cipher that only swaps letters for others score near 0.066, and text spread evenly over 26 letters near 0.038. Ciphers that use several alphabets, like Vigenère, fall in between
    #[builtin(examples = [r#""the quick brown fox jumps over the lazy dog".ioc"#], see = ["friedman", "chi_squared", "key_lengths"])]
    fn ioc(s: Data) -> Result<f64, Fail> {
        Ok(ioc_of(&symbols(&s)?.0))
    }

    /// how far the letters of s are from English letter frequencies, as a chi-squared value, where lower is closer
    ///
    /// With no reference only the letters a to z are scored, without regard to case. Otherwise reference maps each symbol to how often it should appear, in any units, and symbols it doesn't list are ignored
    #[builtin(examples = [r#""the quick brown fox".chi_squared"#, r#""abab".chi_squared({a: 1, b: 1})"#], see = ["ioc", "freq", "caesar_solve"])]
    fn chi_squared(s: Data, reference: Option<&Map>) -> Result<f64, Fail> {
        let (sym, text) = symbols(&s)?;
        let Some(m) = reference else {
            return english_chi_of(&sym).ok_or_else(|| Fail::Arg(0, "there are no letters to score".into()));
        };
        let expected = super::reference(m, text)?;
        let found: Vec<(usize, f64)> = expected.iter().map(|(c, w)| (sym.iter().filter(|x| x == &c).count(), *w)).collect();
        let n: usize = found.iter().map(|f| f.0).sum();
        let total: f64 = found.iter().map(|f| f.1).sum();
        Ok(found.iter().map(|&(seen, w)| (seen as f64 - n as f64 * w / total).powi(2) / (n as f64 * w / total)).sum())
    }

    /// the length of a repeating key, estimated from the index of coincidence by Friedman's formula
    ///
    /// Not rounded, since it is only a rough guess that works best on long texts of 26 letters. Check it against kasiski and key_lengths
    #[builtin(examples = [r#""lxfopvefrnhrlxfopvefrnhrlxfopvefrnhr".friedman"#], see = ["key_lengths", "kasiski", "ioc"])]
    fn friedman(s: Data) -> Result<f64, Fail> {
        let sym = symbols(&s)?.0;
        let (n, o) = (sym.len() as f64, ioc_of(&sym));
        let d = (n - 1.0) * o - n / 26.0 + 0.0667;
        if sym.len() < 2 || d <= 0.0 {
            return Err(Fail::Arg(0, "too short or too evenly spread to estimate a key length".into()));
        }
        Ok((0.0667 - 1.0 / 26.0) * n / d)
    }

    /// each key length up to max with the average index of coincidence of the columns it splits s into, as a table of length and ioc, the highest first
    ///
    /// max is 20 by default. If the key had that length, each column was shifted by one letter, so its ioc is near 0.066 for English. Multiples of the real length score about as high, so prefer the shortest of the top ones
    #[builtin(examples = [r#""lxfopvefrnhrlxfopvefrnhrlxfopvefrnhr".key_lengths(8)[0]"#], see = ["friedman", "kasiski", "vigenere_solve"])]
    fn key_lengths(s: Data, max: Option<i64>) -> Result<Table, Fail> {
        let sym = symbols(&s)?.0;
        let max = max_key(max)?.min(sym.len().max(1));
        let mut rows: Vec<(usize, f64)> = (1..=max).map(|l| (l, avg_ioc(&sym, l))).collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(Table { cols: vec!["length".into(), "ioc".into()], rows: rows.into_iter().map(|(l, i)| vec![Value::int(l as i64), Value::Float(i)]).collect() })
    }

    /// the Kasiski examination of s: runs of n symbols that appear more than once, and the key lengths their distances point to, as {repeats, factors}
    ///
    /// n is 3 by default. repeats is a table of gram, positions (counting from 0) and gaps between them. In a Vigenère cipher, a repeat usually comes from the same word meeting the same part of the key, so the gaps are multiples of the key length. factors is a table of length and count, the number of gaps each length up to max (20 by default) divides, the highest first
    #[builtin(examples = [r#""abcabcabc".kasiski"#], see = ["key_lengths", "friedman", "ngrams"])]
    fn kasiski(s: Data, n: Option<i64>, max: Option<i64>) -> Result<As<Map>, Fail> {
        let (sym, text) = symbols(&s)?;
        let (n, max) = (gram_len(n.or(Some(3)))?, max_key(max)?);
        let (mut rows, mut gaps) = (vec![], vec![]);
        for (g, at) in grams(&sym, n).into_iter().filter(|(_, at)| at.len() > 1) {
            let gap: Vec<usize> = at.windows(2).map(|w| w[1] - w[0]).collect();
            let list = |v: &[usize]| Value::list(v.iter().map(|&x| Value::int(x as i64)).collect());
            rows.push(vec![super::gram(g, text), list(&at), list(&gap)]);
            gaps.extend(gap);
        }
        let mut factors: Vec<(usize, usize)> = (2..=max).map(|f| (f, gaps.iter().filter(|g| *g % f == 0).count())).filter(|f| f.1 > 0).collect();
        factors.sort_by_key(|f| Reverse(f.1));
        let repeats = Table { cols: vec!["gram".into(), "positions".into(), "gaps".into()], rows };
        let factors = Table {
            cols: vec!["length".into(), "count".into()],
            rows: factors.into_iter().map(|(f, c)| vec![Value::int(f as i64), Value::int(c as i64)]).collect(),
        };
        let out: IndexMap<String, Value> =
            [("repeats".to_string(), Value::table(repeats)), ("factors".to_string(), Value::table(factors))].into_iter().collect();
        Ok(Value::map(out).into())
    }

    /// find the key of a Vigenère cipher by finding its length, then solving each Caesar shift by letter frequencies, assuming English, and return {key, text}
    ///
    /// max is the longest key to try, 20 by default, but never more than a twentieth of the letters, since fewer per key letter are too few to tell. Only letters are used
    #[builtin(examples = [r#""lxfopvefrnhrlxfopvefrnhrlxfopvefrnhrlxfopvefrnhr".vigenere_solve"#], see = ["vigenere", "unvigenere", "key_lengths", "caesar_solve"])]
    fn vigenere_solve(s: &str, max: Option<i64>) -> Result<As<Map>, Fail> {
        let letters: Vec<u32> = s.bytes().filter(u8::is_ascii_alphabetic).map(|c| u32::from(c.to_ascii_lowercase() - b'a')).collect();
        if letters.len() < 2 {
            return Err(Fail::Arg(0, "nothing to solve".into()));
        }
        let iocs: Vec<f64> = (1..=max_key(max)?.min((letters.len() / 20).max(1))).map(|l| avg_ioc(&letters, l)).collect();
        // Multiples of the real length score as high, so the shortest one near the best wins.
        let best = iocs.iter().copied().fold(0.0, f64::max);
        let len = iocs.iter().position(|&i| i >= best * 0.9).unwrap_or(0) + 1;
        let key: Vec<usize> = (0..len)
            .map(|i| {
                let counts = letter_counts(letters.iter().skip(i).step_by(len).map(|&c| c as u8));
                (0..26).min_by(|&a, &b| english_chi(&counts, a).total_cmp(&english_chi(&counts, b))).unwrap_or(0)
            })
            .collect();
        let text = shift(s, |n| -(key[n % len] as i64));
        let key: String = key.iter().map(|&k| (b'a' + k as u8) as char).collect();
        Ok(Value::map([("key".to_string(), Value::str(key)), ("text".to_string(), Value::str(text))].into_iter().collect()).into())
    }

    /// a quick look at the statistics of s, as a map of length, symbols, entropy, ioc, chi_squared, top and likely
    ///
    /// symbols is how many different ones there are, top is the five most common, and chi_squared is nil without letters. likely is only a hint at what s could be, judged by its ioc and entropy, so check it with the other statistics
    #[builtin(examples = [r#""Wkh hdjoh odqgv dw plgqljkw".analyze"#], see = ["freq", "ioc", "chi_squared", "entropy"])]
    fn analyze(s: Data) -> Result<As<Map>, Fail> {
        let (sym, text) = symbols(&s)?;
        let counts = tally(&sym);
        let (entropy, ioc) = (shannon(&s.bytes()?), ioc_of(&sym));
        let likely = match () {
            _ if sym.len() < 20 => "too short to tell",
            _ if entropy > 7.0 => "random-looking, like compressed or modern encrypted data",
            _ if ioc >= 0.06 => "close to English, so plain text or a cipher that only swaps or moves letters",
            _ if ioc >= 0.045 => "flatter than English, perhaps a cipher with several alphabets such as Vigenère",
            _ => "nearly flat, perhaps random data or a strong cipher",
        };
        let top = counts.iter().take(5).map(|&(c, n)| vec![symbol(c, text), Value::int(n as i64)]).collect();
        let out: IndexMap<String, Value> = [
            ("length".to_string(), Value::int(sym.len() as i64)),
            ("symbols".to_string(), Value::int(counts.len() as i64)),
            ("entropy".to_string(), Value::Float(entropy)),
            ("ioc".to_string(), Value::Float(ioc)),
            ("chi_squared".to_string(), english_chi_of(&sym).map_or(Value::Nil, Value::Float)),
            ("top".to_string(), Value::table(Table { cols: vec!["symbol".into(), "count".into()], rows: top })),
            ("likely".to_string(), Value::str(likely)),
        ]
        .into_iter()
        .collect();
        Ok(Value::map(out).into())
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

/// The symbols of `d`, a string's characters or the values of a list or bytes, and whether they are text.
fn symbols(d: &Data) -> Result<(Vec<u32>, bool), Fail> {
    match &d.0 {
        Value::Str(s) => Ok((s.chars().map(u32::from).collect(), true)),
        _ => Ok((d.bytes()?.into_iter().map(u32::from).collect(), false)),
    }
}

fn symbol(c: u32, text: bool) -> Value {
    if text { Value::str(char::from_u32(c).unwrap_or('?').to_string()) } else { Value::int(i64::from(c)) }
}

fn gram(g: &[u32], text: bool) -> Value {
    if text {
        Value::str(g.iter().filter_map(|&c| char::from_u32(c)).collect::<String>())
    } else {
        Value::list(g.iter().map(|&c| Value::int(i64::from(c))).collect())
    }
}

/// Each distinct symbol with its count, the most common first, ties in order of appearance.
fn tally(sym: &[u32]) -> Vec<(u32, usize)> {
    let mut m: IndexMap<u32, usize> = IndexMap::new();
    sym.iter().for_each(|&c| *m.entry(c).or_default() += 1);
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by_key(|&(_, n)| Reverse(n));
    v
}

/// Each run of `n` symbols with the places it starts at, in order of first appearance.
fn grams(sym: &[u32], n: usize) -> IndexMap<&[u32], Vec<usize>> {
    let mut m: IndexMap<&[u32], Vec<usize>> = IndexMap::new();
    sym.windows(n).enumerate().for_each(|(i, g)| m.entry(g).or_default().push(i));
    m
}

fn gram_len(n: Option<i64>) -> Result<usize, Fail> {
    usize::try_from(n.unwrap_or(2)).ok().filter(|&n| n >= 2).ok_or_else(|| Fail::Arg(1, "n must be at least 2".into()))
}

fn max_key(max: Option<i64>) -> Result<usize, Fail> {
    usize::try_from(max.unwrap_or(20)).ok().filter(|&n| n >= 1).ok_or_else(|| Fail::Arg(1, "max must be at least 1".into()))
}

/// The chance that two symbols picked from `sym` are the same.
fn ioc_of(sym: &[u32]) -> f64 {
    let n = sym.len() as f64;
    if sym.len() < 2 {
        return 0.0;
    }
    tally(sym).iter().map(|&(_, c)| (c * (c - 1)) as f64).sum::<f64>() / (n * (n - 1.0))
}

/// The mean `ioc_of` of the `len` columns that `sym` splits into when dealt out in turn.
fn avg_ioc(sym: &[u32], len: usize) -> f64 {
    (0..len).map(|i| ioc_of(&sym.iter().skip(i).step_by(len).copied().collect::<Vec<_>>())).sum::<f64>() / len as f64
}

/// How many of each letter, given as 0 to 25.
fn letter_counts(letters: impl Iterator<Item = u8>) -> [usize; 26] {
    let mut counts = [0; 26];
    letters.for_each(|c| counts[c as usize] += 1);
    counts
}

/// Chi-squared of letter counts against English, the count of letter j + k standing for the English letter j.
fn english_chi(counts: &[usize; 26], k: usize) -> f64 {
    let n = counts.iter().sum::<usize>() as f64;
    let total: f64 = FREQ.iter().sum();
    (0..26).map(|j| (counts[(j + k) % 26] as f64 - n * FREQ[j] / total).powi(2) / (n * FREQ[j] / total)).sum()
}

/// `english_chi` of the letters in `sym` alone, or None if it has none.
fn english_chi_of(sym: &[u32]) -> Option<f64> {
    let counts = letter_counts(sym.iter().filter_map(|&c| u8::try_from(c).ok()).filter(u8::is_ascii_alphabetic).map(|c| c.to_ascii_lowercase() - b'a'));
    (counts.iter().sum::<usize>() > 0).then(|| english_chi(&counts, 0))
}

/// The symbols of a reference map, each with its expected share.
fn reference(m: &Map, text: bool) -> Result<Vec<(u32, f64)>, Fail> {
    let bad = |k: &str, why: &str| Fail::Arg(1, format!("{k:?} {why}"));
    let m = m.borrow();
    m.iter()
        .map(|(k, v)| {
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { return Err(bad(k, "is not a single symbol")) };
            let c = u32::from(c);
            if !text && c > 255 {
                return Err(bad(k, "is not a byte value"));
            }
            crate::value::num(v).filter(|w| *w > 0.0).map(|w| (c, w)).ok_or_else(|| bad(k, "needs a positive number"))
        })
        .collect()
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

const BIGRAMS: &str = "th he in er an re on at en nd ti es or te of ed is it al ar st to nt ng se ha as ou io le ve co me de hi ri ro ic ne ea ra ce li ch ll be ma si om ur ly ho ow ld wa ck wn ke ug ui et ss el ta ut ec ns fo no pe di yo ge wi ot sh ad ur ep rt us ee ca ol id un ac so lo do";
const TRIGRAMS: &str = "the and ing ion tio ent ati for her ter hat tha ere ate his con res ver all ons nce men ith ted ers pro thi wit are ess not ive was ect rea com eve per int est sta cti ica ist ear ain one our iti rat ome you ave ont out ere nde hav ill rin ver oul uld sho hin hic ich eir ver";
const QUADGRAMS: &str = "tion ther with ment ions that ould ight have ever this from ting atio ence ough they were hich ated ingt ssio nthe ofth andt inth ngth eres able thin here";

/// Weights for every n-gram in the lists, indexed by its letters in base 26, longer n-grams counting more.
fn ngram_table() -> Vec<i32> {
    let mut t = vec![0; 26 * 26 + 26 * 26 * 26 + 26usize.pow(4)];
    t[..26 * 26].fill(-3);
    for (list, w) in [(BIGRAMS, 2), (TRIGRAMS, 3), (QUADGRAMS, 6)] {
        for g in list.split(' ') {
            t[g.bytes().fold(0, |n, c| n * 26 + (c - b'a') as usize) + [0, 0, 0, 26 * 26, 26 * 26 + 26usize.pow(3)][g.len()]] = w;
        }
    }
    t
}

/// How English-like a run of letters (a to z, as 0 to 25) is, by its common bigrams, trigrams and quadgrams.
fn ngram_score(table: &[i32], t: &[u8]) -> i32 {
    let at = |g: &[u8], base: usize| table[base + g.iter().fold(0, |n, &c| n * 26 + c as usize)];
    t.iter().map(|&c| (26 - b"etaoinshrdlcumwfgypbvkjxqz".iter().position(|&p| p - b'a' == c).unwrap_or(25) as i32) / 6).sum::<i32>()
        + t.windows(2).map(|g| at(g, 0)).sum::<i32>()
        + t.windows(3).map(|g| at(g, 26 * 26)).sum::<i32>()
        + t.windows(4).map(|g| at(g, 26 * 26 + 26usize.pow(3))).sum::<i32>()
}

/// The decryption alphabet for `s`: from a frequency-ranked guess, swap pairs of letters while that raises `ngram_score`, then shake the best key and climb again.
// ponytail: a few dozen common n-grams, not a full quadgram table; short texts may keep a wrong letter, swap in a real table to fix
fn substitution_key(s: &str, tick: &mut dyn FnMut() -> Result<(), Fail>) -> Result<[u8; 26], Fail> {
    let cipher: Vec<u8> = s.bytes().filter(u8::is_ascii_alphabetic).map(|c| c.to_ascii_lowercase() - b'a').collect();
    let mut counts = [0usize; 26];
    cipher.iter().for_each(|&c| counts[c as usize] += 1);
    let mut by_count: Vec<u8> = (0..26).collect();
    by_count.sort_by_key(|&c| std::cmp::Reverse(counts[c as usize]));
    let mut key = [0u8; 26];
    for (c, p) in by_count.iter().zip(b"etaoinshrdlcumwfgypbvkjxqz") {
        key[*c as usize] = *p - b'a';
    }
    let table = ngram_table();
    let score = |key: &[u8; 26]| ngram_score(&table, &cipher.iter().map(|&c| key[c as usize]).collect::<Vec<u8>>());
    let mut climb = |key: &mut [u8; 26]| -> Result<i32, Fail> {
        let mut best = score(key);
        loop {
            let before = best;
            for i in 0..26 {
                for j in i + 1..26 {
                    tick()?;
                    key.swap(i, j);
                    let now = score(key);
                    if now > best {
                        best = now;
                    } else {
                        key.swap(i, j);
                    }
                }
            }
            if best == before {
                return Ok(best);
            }
        }
    };
    let mut best = climb(&mut key)?;
    let mut best_key = key;
    let mut rng = 0x9e3779b97f4a7c15u64;
    let mut next = move |n: u64| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % n) as usize
    };
    for _ in 0..40 {
        key = best_key;
        for _ in 0..4 {
            key.swap(next(26), next(26));
        }
        let now = climb(&mut key)?;
        if now > best {
            best = now;
            best_key = key;
        }
    }
    Ok(best_key.map(|p| p + b'a'))
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
    fn statistics() {
        assert_eq!(show(r#""hello".ioc"#), "0.1");
        assert_eq!(show(r#""hello world".freq[0]"#), r#"{symbol: "l", count: 3, percent: 27.2727}"#);
        assert_eq!(show(r#"b"aab".freq[0].symbol"#), "97");
        assert_eq!(show(r#""aaa".ngrams[0]"#), r#"{gram: "aa", count: 2}"#);
        assert_eq!(show(r#""abcabcabc".kasiski.factors[0]"#), "{length: 3, count: 4}");
        assert_eq!(show(r#""abcabcabc".kasiski.repeats[0].positions"#), "[0, 3, 6]");
        assert_eq!(show(r#""abab".chi_squared({a: 1, b: 1})"#), "0");
        assert!(try_eval(r#""123".chi_squared"#).is_err());
        assert!(try_eval(r#""a".chi_squared({ab: 1})"#).is_err());
        assert!(try_eval(r#""ab".ngrams(1)"#).is_err());
        assert_eq!(show(r#""x".analyze.likely"#), "too short to tell");
        let plain = "it was the best of times and it was the worst of times and the people of the city knew that the winter would be long and that the war would not end soon but they gathered in the square every evening to share what little bread they had and to tell stories of the days before";
        let cipher = format!(r#""{plain}".vigenere("lemon")"#);
        assert_eq!(show(&format!("{cipher}.vigenere_solve.key")), "lemon");
        assert_eq!(show(&format!("{cipher}.vigenere_solve.text")), plain);
        assert_eq!(show(&format!(r#"{cipher}.replace(" ", "").key_lengths(8)[0].length"#)), "5");
        assert!(try_eval(r#""1 2".vigenere_solve"#).is_err());
    }

    #[test]
    fn solvers() {
        assert_eq!(show(r#""Wkh hdjoh odqgv dw plgqljkw".caesar_solve[0]"#), r#"{shift: 3, text: "The eagle lands at midnight"}"#);
        let text = "Burning 'em, if you ain't quick and nimble. I go crazy when I hear a cymbal and a hi-hat with a souped up tempo";
        assert_eq!(show(&format!(r#"xor_solve(xor(b"{text}", "ICE")).key"#)), r#"b"ICE""#);
        assert_eq!(show(&format!(r#"xor_solve(xor(b"{text}", 0x5a)).text"#)), text);
        assert!(try_eval(r#"xor_solve(b"")"#).is_err());
        let plain = "it was the best of times and it was the worst of times and the people of the city knew that the winter would be long and that the war would not end soon but they gathered in the square every evening to share what little bread they had and to tell stories of the days before";
        let solved = show(&format!(r#""{plain}".atbash.substitution_solve.text"#));
        let right = solved.trim_matches('"').chars().zip(plain.chars()).filter(|(a, b)| a == b).count();
        assert!(right * 100 >= plain.len() * 90, "{solved}");
        assert!(try_eval(r#""123".substitution_solve"#).is_err());
    }
}
