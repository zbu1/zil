//! Fancy text: SpOnGeBoB case, l33t, uwu, Pig Latin, zalgo, upside down, emoji, 𝐛𝐨𝐥𝐝/𝒮𝒸𝓇𝒾𝓅𝓉/ｗｉｄｅ Unicode styles.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "fancy",
    about: "silly text: mocking case, l33t, uwu, Pig Latin, zalgo, upside-down, 𝐛𝐨𝐥𝐝 and 𝒮𝒸𝓇𝒾𝓅𝓉 letters, emoji",
    #[rustfmt::skip]
    examples: &[
        ("fancy", &[
            ("reply to a bad take", r#""tabs are better than spaces".mock"#),
            ("every letter style", r#"styles().map(|st| "Zil 2".style(st))"#),
            ("table flip", r#""(╯°□°)╯ " + "zil".flip"#),
            ("a commit message", r#"":rocket: ship it :tada:".emojify"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{FLIP, STYLES, emoji as named, pig, restyle};
    use crate::modules::Fail;
    use crate::value::Value;

    /// aLtErNaTiNg CaSe, the internet's way of quoting someone sarcastically
    #[builtin(examples = [r#""this is fine".mock"#], see = ["upper", "leet"])]
    fn mock(s: &str) -> String {
        let mut up = false;
        s.chars()
            .map(|c| {
                if !c.is_alphabetic() {
                    return c.to_string();
                }
                up = !up;
                if up { c.to_lowercase().to_string() } else { c.to_uppercase().to_string() }
            })
            .collect()
    }

    /// l33t sp34k: letters swapped for look-alike digits
    #[builtin(examples = [r#""elite hacker".leet"#], see = ["mock"])]
    fn leet(s: &str) -> String {
        s.chars()
            .map(|c| match c.to_ascii_lowercase() {
                'a' => '4',
                'e' => '3',
                'i' => '1',
                'o' => '0',
                's' => '5',
                't' => '7',
                'b' => '8',
                'g' => '9',
                _ => c,
            })
            .collect()
    }

    /// rewrite text in cutesy internet "uwu" speak
    #[builtin(examples = [r#""hello friend, I really love this".uwu"#], see = ["mock"])]
    fn uwu(s: &str) -> String {
        let s = regex::Regex::new(r"[rl]").unwrap().replace_all(s, "w");
        let s = regex::Regex::new(r"[RL]").unwrap().replace_all(&s, "W");
        let s = regex::Regex::new(r"([nN])([aeiou])").unwrap().replace_all(&s, "${1}y$2");
        let s = s.replace("ove", "uv");
        let face = ["uwu", "owo", ">w<", "^w^"][s.len() % 4];
        format!("{s} {face}")
    }

    /// translate to Pig Latin, the word game where the first consonants move to the end and "ay" is added
    ///
    /// Capitals and punctuation stay put
    #[builtin(examples = [r#""Hello, string theory!".pig_latin"#], see = ["caesar"])]
    fn pig_latin(s: &str) -> String {
        let re = regex::Regex::new(r"[A-Za-z]+").unwrap();
        re.replace_all(s, |c: &regex::Captures| pig(&c[0])).into_owned()
    }

    /// Z̷a̸l̵g̶o̴ text: pile random accent marks onto every character
    #[builtin(examples = [r#""he comes".zalgo"#, r#""ok".zalgo(1)"#], see = ["flip"])]
    fn zalgo(s: &str, #[default(3)] marks: i64) -> Result<String, Fail> {
        if !(0..=50).contains(&marks) {
            return Err(Fail::Arg(1, "marks must be an int from 0 to 50".into()));
        }
        let mut out = String::new();
        for c in s.chars() {
            out.push(c);
            if !c.is_whitespace() {
                out.extend((0..marks).map(|_| char::from_u32(crate::host::rng(|r| r.u32(0x300..0x370))).unwrap()));
            }
        }
        Ok(out)
    }

    /// turn text upside down, using look-alike characters in reverse order
    #[builtin(examples = [r#""hello world".flip"#], see = ["reverse", "zalgo"])]
    fn flip(s: &str) -> String {
        s.chars()
            .rev()
            .map(|c| {
                FLIP.iter()
                    .find_map(|(a, b)| {
                        if *a == c {
                            Some(*b)
                        } else if *b == c {
                            Some(*a)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(c)
            })
            .collect()
    }

    /// swap letters and digits for Unicode look-alikes, like bold or script
    ///
    /// They keep their look when pasted where formatting isn't allowed, like a social media bio. style names one from styles(). Screen readers may read them poorly
    #[builtin(examples = [r#""Hello".style("bold")"#, r#""vaporwave".style("wide")"#], see = ["styles", "banner", "fonts"])]
    fn style(s: &str, style: &str) -> Result<String, Fail> {
        if !STYLES.contains(&style) {
            return Err(Fail::Arg(1, format!("unknown style {style:?}; try {}", STYLES.join(", "))));
        }
        Ok(s.chars().map(|c| restyle(c, style)).collect())
    }

    /// the names of the letter styles style() can use
    #[builtin(examples = ["styles()"], see = ["style"])]
    fn styles() -> Vec<Value> {
        STYLES.iter().copied().map(Value::str).collect()
    }

    /// the emoji with that name, like "taco", or nil if there's none
    #[builtin(examples = [r#"emoji("taco")"#, r#"emoji("fire")"#], see = ["emojify"])]
    fn emoji(name: &str) -> Option<&'static str> {
        named(name)
    }

    /// replace :name: codes in text with their emoji, as chat apps do
    ///
    /// Codes it doesn't know are left alone
    #[builtin(examples = [r#"":fire: deploy on friday :skull:".emojify"#], see = ["emoji"])]
    fn emojify(s: &str) -> String {
        let re = regex::Regex::new(r":([a-z0-9_+-]+):").unwrap();
        re.replace_all(s, |c: &regex::Captures| named(&c[1]).map_or_else(|| c[0].to_string(), String::from)).into_owned()
    }
}

/// One word: vowels add "way", consonant clusters move to the end before "ay"; a leading capital stays leading.
fn pig(w: &str) -> String {
    let lower = w.to_lowercase();
    let vowel = |c: char| "aeiou".contains(c);
    let split = match lower.find(vowel) {
        Some(0) => return format!("{w}way"),
        // "qu" stays together: "quiet" → "ietquay".
        Some(i) if lower[..i].ends_with('q') && lower[i..].starts_with('u') => i + 1,
        Some(i) => i,
        None => return format!("{w}ay"),
    };
    let out = format!("{}{}ay", &lower[split..], &lower[..split]);
    if w.chars().next().is_some_and(char::is_uppercase) {
        let mut c = out.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    } else {
        out
    }
}

fn emoji(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase().replace([' ', '-'], "_");
    EMOJI.iter().find(|e| e.0.split(' ').any(|n| n == name)).map(|e| e.1)
}

#[rustfmt::skip]
const FLIP: &[(char, char)] = &[
    ('a', 'ɐ'), ('b', 'q'), ('c', 'ɔ'), ('d', 'p'), ('e', 'ǝ'), ('f', 'ɟ'), ('g', 'ƃ'), ('h', 'ɥ'), ('i', 'ᴉ'), ('j', 'ɾ'), ('k', 'ʞ'), ('l', 'l'), ('m', 'ɯ'),
    ('n', 'u'), ('r', 'ɹ'), ('t', 'ʇ'), ('v', 'ʌ'), ('w', 'ʍ'), ('y', 'ʎ'), ('A', '∀'), ('C', 'Ɔ'), ('E', 'Ǝ'), ('F', 'Ⅎ'), ('G', '⅁'), ('J', 'ſ'), ('L', '˥'),
    ('M', 'W'), ('P', 'Ԁ'), ('T', '⊥'), ('U', '∩'), ('V', 'Λ'), ('Y', '⅄'), ('1', 'Ɩ'), ('2', 'ᄅ'), ('3', 'Ɛ'), ('4', 'ㄣ'), ('5', 'ϛ'), ('6', '9'), ('7', 'ㄥ'),
    ('.', '˙'), (',', '\''), ('?', '¿'), ('!', '¡'), ('(', ')'), ('[', ']'), ('{', '}'), ('<', '>'), ('_', '‾'), ('&', '⅋'),
];

#[rustfmt::skip]
const EMOJI: &[(&str, &str)] = &[
    ("smile", "😄"), ("grin", "😁"), ("joy laughing", "😂"), ("rofl", "🤣"), ("wink", "😉"), ("blush", "😊"), ("heart_eyes", "😍"), ("kiss", "😘"),
    ("thinking", "🤔"), ("neutral", "😐"), ("eyeroll", "🙄"), ("smirk", "😏"), ("sweat_smile", "😅"), ("cry", "😢"), ("sob", "😭"), ("angry", "😠"),
    ("rage", "😡"), ("scream", "😱"), ("sunglasses cool", "😎"), ("nerd", "🤓"), ("sleeping", "😴"), ("upside_down", "🙃"), ("shush", "🤫"), ("mindblown exploding_head", "🤯"),
    ("party partying", "🥳"), ("clown", "🤡"), ("skull dead", "💀"), ("ghost", "👻"), ("alien", "👽"), ("robot", "🤖"), ("poop", "💩"), ("devil", "😈"),
    ("thumbsup +1 yes", "👍"), ("thumbsdown -1 no", "👎"), ("clap", "👏"), ("wave", "👋"), ("pray thanks", "🙏"), ("muscle strong", "💪"), ("ok_hand", "👌"),
    ("point_up", "☝️"), ("eyes", "👀"), ("brain", "🧠"), ("facepalm", "🤦"), ("shrug", "🤷"),
    ("heart", "❤️"), ("broken_heart", "💔"), ("sparkles", "✨"), ("star", "⭐"), ("fire", "🔥"), ("100", "💯"), ("boom", "💥"), ("zap lightning", "⚡"),
    ("rainbow", "🌈"), ("sun sunny", "☀️"), ("cloud", "☁️"), ("rain", "🌧️"), ("snow snowflake", "❄️"), ("moon", "🌙"), ("earth globe", "🌍"),
    ("rocket", "🚀"), ("tada", "🎉"), ("confetti", "🎊"), ("gift", "🎁"), ("trophy", "🏆"), ("medal", "🏅"), ("crown", "👑"), ("gem", "💎"), ("moneybag money", "💰"),
    ("bulb idea", "💡"), ("bug", "🐛"), ("wrench", "🔧"), ("hammer", "🔨"), ("gear", "⚙️"), ("lock", "🔒"), ("key", "🔑"), ("bell", "🔔"), ("hourglass", "⏳"),
    ("warning", "⚠️"), ("x cross", "❌"), ("check white_check_mark", "✅"), ("question", "❓"), ("exclamation", "❗"), ("stop", "🛑"), ("construction", "🚧"),
    ("computer laptop", "💻"), ("phone", "📱"), ("email", "📧"), ("memo", "📝"), ("book", "📖"), ("calendar", "📅"), ("chart", "📈"), ("package", "📦"),
    ("coffee", "☕"), ("tea", "🍵"), ("beer", "🍺"), ("wine", "🍷"), ("pizza", "🍕"), ("taco", "🌮"), ("burger hamburger", "🍔"), ("fries", "🍟"),
    ("sushi", "🍣"), ("ramen", "🍜"), ("cake", "🎂"), ("cookie", "🍪"), ("doughnut donut", "🍩"), ("apple", "🍎"), ("banana", "🍌"), ("avocado", "🥑"),
    ("cat", "🐱"), ("dog", "🐶"), ("fox", "🦊"), ("panda", "🐼"), ("penguin", "🐧"), ("crab rust ferris", "🦀"), ("snake python", "🐍"), ("whale", "🐳"),
    ("unicorn", "🦄"), ("dragon", "🐉"), ("turtle", "🐢"), ("octopus", "🐙"), ("bee", "🐝"), ("owl", "🦉"), ("frog", "🐸"), ("monkey", "🐒"),
    ("tree", "🌳"), ("cactus", "🌵"), ("rose", "🌹"), ("sunflower", "🌻"), ("mushroom", "🍄"), ("music", "🎵"), ("guitar", "🎸"), ("video_game", "🎮"),
    ("dice die", "🎲"), ("soccer", "⚽"), ("basketball", "🏀"), ("car", "🚗"), ("bike", "🚲"), ("airplane plane", "✈️"), ("ship", "🚢"), ("house home", "🏠"),
];

const STYLES: &[&str] =
    &["bold", "italic", "bold_italic", "script", "fraktur", "double", "sans", "sans_bold", "mono", "wide", "circled", "small_caps", "strike", "underline"];

/// One character in a style; anything the style has no look-alike for passes through.
fn restyle(c: char, style: &str) -> String {
    let from = |base: u32, start: char| char::from_u32(base + (c as u32 - start as u32)).unwrap();
    // Mathematical Alphanumeric Symbols: capital A, small a and digit 0 per style; a few letters live in Letterlike Symbols instead.
    #[rustfmt::skip]
    let (upper, lower, digit, holes): (u32, u32, Option<u32>, &[(char, char)]) = match style {
        "bold" => (0x1D400, 0x1D41A, Some(0x1D7CE), &[]),
        "italic" => (0x1D434, 0x1D44E, None, &[('h', 'ℎ')]),
        "bold_italic" => (0x1D468, 0x1D482, None, &[]),
        "script" => (0x1D49C, 0x1D4B6, None, &[('B', 'ℬ'), ('E', 'ℰ'), ('F', 'ℱ'), ('H', 'ℋ'), ('I', 'ℐ'), ('L', 'ℒ'), ('M', 'ℳ'), ('R', 'ℛ'), ('e', 'ℯ'), ('g', 'ℊ'), ('o', 'ℴ')]),
        "fraktur" => (0x1D504, 0x1D51E, None, &[('C', 'ℭ'), ('H', 'ℌ'), ('I', 'ℑ'), ('R', 'ℜ'), ('Z', 'ℨ')]),
        "double" => (0x1D538, 0x1D552, Some(0x1D7D8), &[('C', 'ℂ'), ('H', 'ℍ'), ('N', 'ℕ'), ('P', 'ℙ'), ('Q', 'ℚ'), ('R', 'ℝ'), ('Z', 'ℤ')]),
        "sans" => (0x1D5A0, 0x1D5BA, Some(0x1D7E2), &[]),
        "sans_bold" => (0x1D5D4, 0x1D5EE, Some(0x1D7EC), &[]),
        "mono" => (0x1D670, 0x1D68A, Some(0x1D7F6), &[]),
        "circled" => (0x24B6, 0x24D0, None, &[('0', '⓪')]),
        "wide" => return match c {
            '!'..='~' => from(0xFF01, '!').to_string(),
            _ => c.to_string(),
        },
        "small_caps" => return match c {
            'a'..='z' => "ᴀʙᴄᴅᴇꜰɢʜɪᴊᴋʟᴍɴᴏᴘǫʀꜱᴛᴜᴠᴡxʏᴢ".chars().nth(c as usize - 'a' as usize).unwrap().to_string(),
            _ => c.to_string(),
        },
        "strike" | "underline" if c.is_whitespace() => return c.to_string(),
        "strike" => return format!("{c}\u{336}"),
        "underline" => return format!("{c}\u{332}"),
        _ => unreachable!("style checked against STYLES"),
    };
    if let Some(&(_, h)) = holes.iter().find(|h| h.0 == c) {
        return h.to_string();
    }
    match (c, digit) {
        ('A'..='Z', _) => from(upper, 'A'),
        ('a'..='z', _) => from(lower, 'a'),
        ('0'..='9', Some(d)) => from(d, '0'),
        ('1'..='9', None) if style == "circled" => from(0x2460, '1'),
        _ => c,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn fancy() {
        assert_eq!(show(r#""this is fine".mock"#), "tHiS iS fInE");
        assert_eq!(show(r#""leet sauce".leet"#), "l337 54uc3");
        assert!(show(r#""hello, really".uwu"#).starts_with("hewwo, weawwy"));
        assert_eq!(show(r#""Hello, string theory! Eat quiet rhythm".pig_latin"#), "Ellohay, ingstray eorythay! Eatway ietquay rhythmay");
        assert_eq!(show(r#"["ab".zalgo(2).len, "a b".zalgo(1).len, "x".zalgo(0)]"#), r#"[6, 5, "x"]"#);
        assert_eq!(show(r#"["hello!".flip, "hello!".flip.flip]"#), r#"["¡ollǝɥ", "hello!"]"#);
        assert_eq!(show(r#"[emoji("Taco"), emoji("rust"), emoji("nope")]"#), r#"["🌮", "🦀", nil]"#);
        assert_eq!(show(r#"":fire: :nope: ok".emojify"#), "🔥 :nope: ok");
        assert_eq!(
            show(r#"["Hi 0".style("bold"), "Hero".style("script"), "CHAZ".style("double"), "a1 0".style("circled")]"#),
            r#"["𝐇𝐢 𝟎", "ℋℯ𝓇ℴ", "ℂℍ𝔸ℤ", "ⓐ① ⓪"]"#
        );
        assert_eq!(show(r#"["ab 1!".style("wide"), "Hi z".style("small_caps"), "a b".style("strike").len]"#), r#"["ａｂ １！", "Hɪ ᴢ", 5]"#);
        assert_eq!(show(r#"styles().map(|s| "x".style(s)).len"#), "14");
    }
}
