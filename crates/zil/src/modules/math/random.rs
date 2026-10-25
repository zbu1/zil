//! Chance: random numbers, picks and shuffles, dice and coins, probabilities in plain words, and oracles of questionable reliability.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, Section, doc};
use crate::value::{Value, num};
use indexmap::IndexMap;
use std::hash::{DefaultHasher, Hash, Hasher};

pub const MODULE: Module = Module {
    name: "random",
    about: "random numbers, picks, shuffles and dice, chances in plain words, and fortunes for fun",
    examples: EXAMPLES,
    fns: FNS,
    #[rustfmt::skip]
    groups: &[
        ("numbers and lists", &["rand", "choice", "shuffle", "seed"]),
        ("dice and coins", &["roll", "coin"]),
        ("probability", &["birthday_paradox", "odds"]),
        ("oracles", &["fortune", "eight_ball", "yes_or_no", "excuse"]),
    ],
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const EXAMPLES: &[Section] = &[
    ("random", &[
        ("roll a die", "rand(1, 6)"),
        ("a decimal between 1 and 2", "rand(1.0, 2.0)"),
        ("pick one", r#"["rock", "paper", "scissors"].choice"#),
        ("shuffle", "(1..=5).shuffle"),
        ("a D&D stat: 4d6, keep the best 3", r#"roll("4d6kh3")"#),
        ("roll 2d20, keep the higher, add 5", r#"roll("2d20kh1+5").total"#),
        ("five coin flips", "coin(5)"),
        ("the chance two of 23 people share a birthday", "birthday_paradox(23)"),
        ("how unlikely is the jackpot?", "odds(1 / 292201338)"),
        ("ask the ball", r#"eight_ball("should I get a dog?")"#),
        ("why you're late", "excuse()"),
    ]),
];

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("rand", "rand() -> float / rand(a: num, b: num) -> num", "a random decimal from 0 up to but not including 1", &["rand()", "rand(1, 6)"], &["choice", "shuffle"])
        .detail("With a and b, a random number from a to b, including both: whole numbers if both are integers, otherwise decimals"),
    doc("seed", "seed(n: int) -> nil", "make the session's random choices repeatable", &["seed(7); rand(1, 100)"], &["rand"])
        .detail("After the same seed, rand, shuffle, maze and the rest give the same results again"),
    doc("choice", "choice(xs: list) -> any", "a random item from the list", &[r#"["rock", "paper", "scissors"].choice"#], &["rand", "shuffle"]),
    doc("shuffle", "shuffle(xs: list) -> list", "a copy of the list in random order", &["(1..6).shuffle"], &["choice"]),
    doc("roll", "roll(dice?: str) -> map", "roll dice written as in tabletop games, like \"3d6+2\", and return {total, rolls}", &[r#"roll("3d6+2")"#, r#"roll("d%").total"#], &["rand", "coin"])
        .detail("\"3d6+2\" is three six-sided dice plus 2, \"d20\" one twenty-sided die, \"d%\" a percentile die, \"4d6kh3\" keeps the highest 3 of 4 dice and \"2d20kl1\" the lowest 1. One six-sided die by default"),
    doc("birthday_paradox", "birthday_paradox(people: int, days?: int) -> float", "the chance that at least two of n people share a birthday", &["birthday_paradox(23)", "birthday_paradox(70)", "birthday_paradox(10, 100)"], &["odds", "choose"])
        .detail("Give days to ask the same about any number of equally likely values, like hash collisions"),
    doc("odds", "odds(p: num) -> str", "a probability as \"1 in N\", with a familiar event that's about as likely, to give a feel for it", &["odds(1 / 1000)", "odds(0.5)", "odds(2 ** -64)"], &["birthday_paradox"]),
    doc("fortune", "fortune() -> str", "a random fortune-cookie saying", &["fortune()"], &["eight_ball"]),
    doc("eight_ball", "eight_ball(question?: str) -> str", "an answer from a Magic 8-Ball toy", &[r#"eight_ball("is it Friday?")"#], &["yes_or_no", "fortune"])
        .detail("Asking the same question again the same day gets the same answer"),
    doc("coin", "coin(n?: int) -> str|list", "\"heads\" or \"tails\", or a list of n flips", &["coin()", "coin(3)"], &["rand"]),
    doc("yes_or_no", "yes_or_no(question?: str) -> str", "a random \"yes\" or \"no\", a little more likely to be yes on Fridays", &["yes_or_no()"], &["eight_ball"]),
    doc("excuse", "excuse() -> str", "a random excuse for why the code doesn't work", &["excuse()"], &["fortune"]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    let pick = |list: &[&'static str]| Value::str(list[crate::host::rng(|r| r.usize(..list.len()))]);
    Ok(match (name, args) {
        ("rand", []) => Float(crate::host::rng(|r| r.f64())),
        ("rand", [Int(a, _), Int(b, _)]) if a <= b => Value::int(crate::host::rng(|r| r.i64(*a..=*b))),
        ("rand", [a, b]) if num(a).is_some() && num(b).is_some() => {
            let (a, b) = (num(a).unwrap(), num(b).unwrap());
            Float(a + crate::host::rng(|r| r.f64()) * (b - a))
        }
        ("seed", [Int(n, _)]) => {
            crate::host::rng(|r| r.seed(*n as u64));
            Nil
        }
        ("choice", [List(l)]) => {
            let l = l.borrow();
            if l.is_empty() {
                return Err("empty list".into());
            }
            l[crate::host::rng(|r| r.usize(..l.len()))].clone()
        }
        ("shuffle", [List(l)]) => {
            let mut v = l.borrow().clone();
            crate::host::rng(|r| r.shuffle(&mut v));
            Value::list(v)
        }
        ("roll", []) => roll("1d6")?,
        ("roll", [Str(s)]) => roll(s).map_err(|e| Fail::Arg(0, e))?,
        ("birthday_paradox", [Int(n, _), rest @ ..]) if rest.len() <= 1 => {
            let days = match rest {
                [] => 365,
                [Int(d, _)] if *d >= 1 => *d,
                _ => return Err(Fail::Arg(1, "days must be a positive int".into())),
            };
            if *n < 0 {
                return Err(Fail::Arg(0, format!("people must be 0 or more, got {n}")));
            }
            if *n > days {
                return Ok(Float(1.0));
            }
            let (nf, df) = (*n as f64, days as f64);
            // ponytail: past a million people, the series to the n^3 term instead of the exact product; good to ~1e-6
            let none_shared: f64 = if *n > 1_000_000 {
                (-(nf * (nf - 1.0)) / (2.0 * df) - nf.powi(3) / (6.0 * df * df)).exp()
            } else {
                (0..*n).map(|i| (days - i) as f64 / df).product()
            };
            Float(1.0 - none_shared)
        }
        ("odds", [p]) => {
            let p = num(p).filter(|p| *p > 0.0 && *p <= 1.0).ok_or(Fail::Arg(0, "expected a probability above 0, up to 1".into()))?;
            let n = 1.0 / p;
            let (what, m) = EVENTS.iter().min_by(|a, b| ((a.1 / n).ln().abs()).total_cmp(&(b.1 / n).ln().abs())).unwrap();
            Value::str(format!("1 in {}: about as likely as {what} (1 in {})", big_num(n), big_num(*m)))
        }
        ("fortune", []) => pick(FORTUNES),
        ("eight_ball", []) => pick(EIGHT_BALL),
        ("eight_ball", [Str(q)]) => {
            let mut h = DefaultHasher::new();
            (q.trim().to_lowercase(), crate::host::now().date()).hash(&mut h);
            Value::str(EIGHT_BALL[h.finish() as usize % EIGHT_BALL.len()])
        }
        ("coin", []) => pick(&["heads", "tails"]),
        ("coin", [Int(n, _)]) if (0..=10_000).contains(n) => Value::list((0..*n).map(|_| pick(&["heads", "tails"])).collect()),
        ("yes_or_no", [] | [Str(_)]) => {
            let friday = crate::host::now().weekday() == jiff::civil::Weekday::Friday;
            Value::str(if crate::host::rng(|r| r.f64()) < if friday { 0.8 } else { 0.5 } { "yes" } else { "no" })
        }
        ("excuse", []) => pick(EXCUSES),
        _ => return Err(Fail::BadArgs),
    })
}

/// "4d6kh3+2-1d4": dice terms and constants, each with a sign.
fn roll(spec: &str) -> Result<Value, String> {
    // Spaces may surround + and -, but "2d6 3" must not read as 2d63.
    let s = regex::Regex::new(r"\s*([+-])\s*").unwrap().replace_all(spec.trim(), "$1").to_lowercase();
    let term = regex::Regex::new(r"^([+-]?)(?:(\d*)d(\d+|%)(?:k([hl]?)(\d+))?|(\d+))").unwrap();
    let bad = || format!("can't read {spec:?} as dice\nnote: like \"3d6+2\", \"d20\", \"4d6kh3\" or \"2d20kl1\"");
    let (mut rest, mut total, mut rolls) = (s.as_str(), 0i64, vec![]);
    if rest.is_empty() {
        return Err(bad());
    }
    while !rest.is_empty() {
        let c = term.captures(rest).filter(|c| !c[0].is_empty()).ok_or_else(bad)?;
        let sign = if &c[1] == "-" { -1 } else { 1 };
        if c.get(1).unwrap().as_str().is_empty() && rest.len() != s.len() {
            return Err(bad());
        }
        if let Some(k) = c.get(6) {
            total += sign * k.as_str().parse::<i64>().map_err(|_| bad())?;
        } else {
            let count: usize = if c[2].is_empty() { 1 } else { c[2].parse().map_err(|_| bad())? };
            let sides: i64 = if &c[3] == "%" { 100 } else { c[3].parse().map_err(|_| bad())? };
            if !(1..=1000).contains(&count) || !(1..=1_000_000).contains(&sides) {
                return Err("dice count must be 1-1000 and sides 1-1000000".into());
            }
            let mut dice: Vec<i64> = (0..count).map(|_| crate::host::rng(|r| r.i64(1..=sides))).collect();
            rolls.extend(dice.iter().map(|d| Value::int(*d)));
            if let Some(keep) = c.get(5) {
                let keep: usize = keep.as_str().parse().map_err(|_| bad())?;
                dice.sort_unstable();
                if &c[4] != "l" {
                    dice.reverse();
                }
                dice.truncate(keep);
            }
            total += sign * dice.iter().sum::<i64>();
        }
        rest = &rest[c[0].len()..];
    }
    let mut m = IndexMap::new();
    m.insert("total".to_string(), Value::int(total));
    m.insert("rolls".to_string(), Value::list(rolls));
    Ok(Value::map(m))
}

/// 1234567 → "1,234,567"; huge ones in words or scientific.
fn big_num(n: f64) -> String {
    if n < 1e15 {
        let s = format!("{:.0}", n);
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    } else {
        format!("{n:.2e}").replace("e", " × 10^")
    }
}

/// Commonly cited odds, rounded.
#[rustfmt::skip]
const EVENTS: &[(&str, f64)] = &[
    ("a coin landing heads", 2.0),
    ("rolling a six", 6.0),
    ("being dealt a pocket pair in Texas hold'em", 17.0),
    ("rolling double sixes", 36.0),
    ("identical twins", 250.0),
    ("flipping ten heads in a row", 1024.0),
    ("finding a four-leaf clover on the first try", 10_000.0),
    ("a hole in one for an amateur golfer", 12_500.0),
    ("being dealt a royal flush", 649_740.0),
    ("being struck by lightning this year in the US", 1_222_000.0),
    ("flipping 30 heads in a row", 1_073_741_824.0),
    ("winning the Powerball jackpot", 292_201_338.0),
    ("a perfect March Madness bracket", 9.2e18),
    ("guessing a random 128-bit key", 3.4e38),
    ("shuffling a deck into a given order", 8.07e67),
];

const EIGHT_BALL: &[&str] = &[
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes, definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful.",
];

const FORTUNES: &[&str] = &[
    "You will soon find money in a coat you forgot you owned.",
    "A nap is in your future. Do not fight it.",
    "The leftovers in the fridge are older than you think.",
    "You will walk into a room and forget why. Twice.",
    "Your houseplant believes in you. Water it anyway.",
    "A pigeon will judge you this week. Hold your head high.",
    "You will say \"you too\" to a waiter who said \"enjoy your meal\".",
    "Good news is on its way. So is spam.",
    "The socks you lost are living a better life without you.",
    "You will have a brilliant idea in the shower and lose it in the towel.",
    "Do not trust the last slice. It was left for a reason.",
    "An exciting opportunity awaits you behind the couch cushions.",
    "You will be asked to help someone move. Have an excuse ready.",
    "Your lucky number is 7. Or 4. Results may vary.",
    "A cat will ignore you today. Take it as a compliment.",
    "You will meet a tall, dark stranger. It is your shadow.",
    "The early bird gets the worm. You are free to sleep in.",
    "Help! I am trapped in a fortune cookie factory.",
    "That thing you've been putting off? Still there.",
    "Ignore the previous fortune.",
    "A great adventure awaits, right after this snack.",
    "You will finally fold that laundry. Not today, though.",
    "Somewhere, a dog is very happy to see someone. Be that someone.",
    "You are about to step on a Lego.",
    "Fortune not found. Please try again after coffee.",
    "Your future is bright. Wear sunscreen.",
    "Soon you will be hungry. This prediction is never wrong.",
    "Your microwave will beep at the worst possible moment.",
    "Today you will pull a door that says push.",
    "A small act of kindness will come back to you, possibly as a casserole.",
    "Beware of anyone who says \"quick question\".",
    "You will be right about something tiny and remember it forever.",
    "Someone you know is pretending to have read the book.",
    "Wealth is coming, mostly in the form of loyalty points.",
    "A package is in your future. It's the thing you forgot you ordered.",
    "Life is short. Eat dessert first.",
    "You will wave back at someone who was waving at the person behind you.",
    "The answer you seek is in the group chat you muted.",
    "A long-lost friend will reach out. They want to borrow something.",
    "Your karaoke moment is coming. Choose wisely.",
];

const EXCUSES: &[&str] = &[
    "My alarm didn't go off.",
    "The dog ate it.",
    "Traffic was unbelievable.",
    "Mercury is in retrograde.",
    "I thought it was tomorrow.",
    "My phone died, and a part of me died with it.",
    "I was abducted by aliens. They send their regards.",
    "A goose blocked the path and would not negotiate.",
    "I got stuck talking to a neighbor about their hedge.",
    "I was going to, but then I sat down.",
    "The cat was sitting on me and I couldn't move.",
    "I never got the message.",
    "It's been a really long week, and it's only Tuesday.",
    "I was busy being fabulous.",
    "There was a spider. I had to move out.",
    "I locked my keys in the car. With the car running.",
    "My horoscope told me to stay in bed.",
    "I was there in spirit.",
    "Autocorrect changed my plans.",
    "I was waiting for the right moment. It never came.",
    "Someone ate my lunch and I needed time to grieve.",
    "The weather looked suspicious.",
    "I got distracted by a really good sandwich.",
    "My twin did it.",
    "I was told there would be snacks.",
    "My socks didn't match and I couldn't go on.",
    "I was on hold for three hours.",
    "The GPS took me somewhere exciting.",
    "A squirrel stared me down and I lost my nerve.",
    "I was practicing self-care.",
    "The instructions were in a language I don't speak: flat-pack furniture.",
    "My plants needed emotional support.",
    "I tripped over nothing and needed a moment.",
    "Daylight saving time.",
    "I accidentally watched an entire season.",
    "It was like that when I got here.",
    "My grandma called, and nobody hangs up on grandma.",
    "I had a wardrobe malfunction of the soul.",
    "The bus left early. I saw it. It saw me.",
    "I'm not late, everyone else is early.",
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::{eval, show, try_eval};
    use crate::value::Value;

    #[test]
    fn random() {
        assert!(matches!(eval("rand(1, 6)"), Value::Int(1..=6, _)));
        assert_eq!(eval("[1, 2, 3].shuffle.sort").to_string(), "[1, 2, 3]");
        assert!(try_eval("choice([])").is_err());
        assert_eq!(show("seed(3); [rand(), maze(4, 2)]"), show("seed(3); [rand(), maze(4, 2)]"));
    }

    #[test]
    fn fortune() {
        assert_eq!(show(r#"eight_ball("same?")"#), show(r#"eight_ball(" SAME? ")"#));
        assert_eq!(show("coin(4).len"), "4");
        assert!(["yes", "no"].contains(&show("yes_or_no()").as_str()));
        assert!(try_eval("coin(-1)").is_err());
    }

    #[test]
    fn games() {
        for _ in 0..50 {
            assert!(matches!(eval(r#"roll("3d6+2").total"#), Value::Int(5..=20, _)));
            assert!(matches!(eval(r#"roll("4d6kh3").total"#), Value::Int(3..=18, _)));
            assert!(matches!(eval(r#"roll("2d20kl1-1").total"#), Value::Int(0..=19, _)));
            assert!(matches!(eval(r#"roll("d%").total"#), Value::Int(1..=100, _)));
        }
        assert_eq!(show(r#"[roll("4d6kh3").rolls.len, roll("5").total, roll().rolls.len]"#), "[4, 5, 1]");
        for bad in [r#"roll("")"#, r#"roll("3x6")"#, r#"roll("2d6 3")"#, r#"roll("0d6")"#, r#"roll("2d6++3")"#] {
            assert!(try_eval(r#"roll("2d6 + 3")"#).is_ok());
            assert!(try_eval(bad).is_err(), "{bad}");
        }
        assert_eq!(show("[birthday_paradox(23).round(3), birthday_paradox(1), birthday_paradox(366)]"), "[0.507, 0, 1]");
        assert_eq!(show("odds(1/1000)"), "1 in 1,000: about as likely as flipping ten heads in a row (1 in 1,024)");
        assert!(show("odds(2 ** -128)").contains("128-bit"));
    }
}
