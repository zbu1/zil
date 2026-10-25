//! Identifiers and secrets: UUIDs made and inspected, ULIDs, nano IDs, passwords and passphrases.

use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc, text::encoding::hex};
use crate::value::Value;
use indexmap::IndexMap;
use jiff::Timestamp;

pub const MODULE: Module = Module {
    name: "ids",
    about: "unique IDs (UUIDs, ULIDs, nano IDs) and strong passwords",
    #[rustfmt::skip]
    examples: &[
        ("ids", &[
            ("a random ID", "uuid()"),
            ("when was this UUID made?", r#"uuid_info("01890a5d-ac96-774b-bcce-b302099a8057").timestamp"#),
            ("an ID that sorts by time", "ulid()"),
            ("a strong password", "password()"),
            ("one that's easier to type", "passphrase()"),
        ]),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("uuid", "uuid() -> str", "a new random UUID (version 4)", &["uuid()"], &["uuid_info", "ulid"])
        .detail("A 36-character ID random enough that two never match, so it can be made anywhere without checking with anyone"),
    doc("password", "password(len?: int) -> str", "a random password of letters, digits and symbols, 20 characters by default", &["password()", "password(12)"], &["passphrase"])
        .detail("Each character adds about 6.5 bits of strength. Each bit doubles the guesses an attacker needs, and 80 bits or more is out of reach even for offline guessing"),
    doc("passphrase", "passphrase(words?: int) -> str", "random words joined by dashes, easier to type and remember than a password, 6 words by default, each adding about 9.4 bits of strength", &["passphrase()", "passphrase(4)"], &["password"]),
    doc("ulid", "ulid() -> str", "a new ULID", &["ulid()"], &["uuid", "nanoid"])
        .detail("26 characters holding the time it was made and then 80 random bits, so sorting ULIDs sorts them by age"),
    doc("nanoid", "nanoid(len?: int) -> str", "a random ID made only of characters that are safe in URLs, 21 characters by default, about as unlikely to repeat as a UUID", &["nanoid()", "nanoid(8)"], &["uuid", "ulid"]),
    doc("uuid_info", "uuid_info(s: str) -> map", "what kind of UUID this is (its version and variant)",
        &[r#"uuid_info(uuid())"#, r#"uuid_info("c232ab00-9414-11ec-b3c8-9f6bdeced846")"#], &["uuid"])
        .detail("Versions 1 and 7 start with the time they were made, which this reads back"),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    use Value::*;
    Ok(match (name, args) {
        ("uuid", []) => {
            let mut b = [0u8; 16];
            crate::host::rng(|r| r.fill(&mut b));
            b[6] = (b[6] & 0x0f) | 0x40; // version 4
            b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
            let h = hex(&b);
            Value::str(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
        }
        ("password", []) => secret(20, PASSWORD_CHARS)?,
        ("password", [Int(n, _)]) if (4..=1000).contains(n) => secret(*n as usize, PASSWORD_CHARS)?,
        ("nanoid", []) => secret(21, NANOID_CHARS)?,
        ("nanoid", [Int(n, _)]) if (1..=1000).contains(n) => secret(*n as usize, NANOID_CHARS)?,
        ("password" | "nanoid", [Int(n, _)]) => {
            return Err(Fail::Arg(0, format!("length must be from {} to 1000, got {n}", if name == "password" { 4 } else { 1 })));
        }
        ("passphrase", []) => passphrase(6)?,
        ("passphrase", [Int(n, _)]) if (1..=100).contains(n) => passphrase(*n as usize)?,
        ("passphrase", [Int(n, _)]) => return Err(Fail::Arg(0, format!("word count must be from 1 to 100, got {n}"))),
        ("ulid", []) => {
            let ms = crate::host::timestamp().as_millisecond() as u128;
            let mut r = [0u8; 10];
            getrandom::getrandom(&mut r).map_err(|e| e.to_string())?;
            let n = (ms << 80) | r.iter().fold(0u128, |acc, b| acc << 8 | *b as u128);
            Value::str((0..26).rev().map(|i| CROCKFORD[(n >> (i * 5)) as usize & 31] as char).collect::<String>())
        }
        ("uuid_info", [Str(s)]) => uuid_info(s).ok_or_else(|| Fail::Arg(0, format!("`{s}` is not a UUID")))?,
        _ => return Err(Fail::BadArgs),
    })
}

fn uuid_info(s: &str) -> Option<Value> {
    let h: String = s.trim().trim_start_matches("urn:uuid:").chars().filter(|&c| !"{}-".contains(c)).collect();
    let n = u128::from_str_radix(&h, 16).ok().filter(|_| h.len() == 32)?;
    let b = n.to_be_bytes();
    let version = b[6] >> 4;
    let variant = match b[8] >> 5 {
        0..=3 => "NCS",
        4 | 5 => "RFC 9562",
        6 => "Microsoft",
        _ => "reserved",
    };
    let time = match version {
        // 100 ns ticks since 1582-10-15, split low/mid/high across the first 8 bytes.
        1 => {
            let ticks = (n >> 64 & 0xfff) << 48 | (n >> 80 & 0xffff) << 32 | n >> 96;
            Timestamp::from_microsecond((ticks as i64 - 0x01B2_1DD2_1381_4000) / 10).ok()
        }
        7 => Timestamp::from_millisecond((n >> 80) as i64).ok(),
        _ => None,
    };
    let mut m = IndexMap::from([("version".to_string(), Value::int(version as i64)), ("variant".to_string(), Value::str(variant))]);
    if let Some(t) = time {
        m.insert("timestamp".to_string(), Value::date(t.to_zoned(crate::host::tz())));
    }
    Some(Value::map(m))
}

const PASSWORD_CHARS: &str = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_=+?";
const NANOID_CHARS: &str = "useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Uniform secure index below `n`, by rejection so no index is favored.
fn secure_index(n: usize) -> Result<usize, String> {
    let limit = u16::MAX as usize + 1;
    let cut = limit - limit % n;
    loop {
        let mut b = [0u8; 2];
        getrandom::getrandom(&mut b).map_err(|e| e.to_string())?;
        let x = u16::from_le_bytes(b) as usize;
        if x < cut {
            return Ok(x % n);
        }
    }
}

fn secret(len: usize, alphabet: &str) -> Result<Value, String> {
    let cs: Vec<char> = alphabet.chars().collect();
    Ok(Value::str((0..len).map(|_| secure_index(cs.len()).map(|i| cs[i])).collect::<Result<String, _>>()?))
}

fn passphrase(n: usize) -> Result<Value, String> {
    let words: Vec<&str> = WORDS.split_whitespace().collect();
    Ok(Value::str((0..n).map(|_| secure_index(words.len()).map(|i| words[i])).collect::<Result<Vec<_>, _>>()?.join("-")))
}

/// Short, common, distinct words for passphrases.
const WORDS: &str = "acid acorn actor adapt adobe agent alarm album alert alias alien alley alpha amber amigo ample angel anger angle ankle apple april apron arena argue \
armor arrow aspen atlas atom attic audio avoid awake award bacon badge bagel baker balmy banjo barge basil batch beach beard beast begin bench berry \
bias bike bingo birch bison blade blank blast blaze blend blimp blink bliss block blond bloom blues bluff blunt board bonus boost booth boots bored \
bossy bound boxer brain brake brand brass brave bread brick bride brief brine brisk broom brush buddy buggy bulb bunch bunny cabin cable cacao cadet \
camel camera candy canoe canon cargo carol carve cedar chain chalk champ chant chaos charm chart chase cheek cheer chess chief child chili chimp chirp \
choir chomp chord chunk cider cinch civic claim clamp clash clasp class clay clerk click cliff climb cloak clock cloud clown coach coast cobra cocoa \
comet comic coral couch cough crane crate crawl crazy cream crest crisp crown crumb crust cubic cupid curly curry cycle daisy dance dandy decal decoy \
delta denim depot diary dice diner disco ditch diver dizzy dodge dolly donut dough dozen draft drama dream dress drift drill drink drive drone drum \
dusty eagle easel ebony eclair edge eject elbow elder elect elite elm ember emoji empty enjoy entry envoy equal error essay ethic event exact exile \
extra fable facet fairy faith false fancy feast fence ferry fever fiber field fifty final finch fjord flair flake flame flash flask fleet flint float \
flock flood floor flour fluid flute focal focus foggy forge forum fossil frame frost fruit fudge funky fuzzy gamer gauge gecko genie ghost giant giddy \
ginger glade gland glass gleam glide globe glove gnome golf goose gorge grace grain grape graph grasp gravy great greed grief grill grind groan groom \
grove growl guard guava guest guide guild gummy gusto habit hairy happy hardy harsh hasty haven hazel heart heavy hedge hefty helix hello heron hippo \
hobby holly honey hoop horse hotel house humid humor hurry husky hyena icing icon ideal igloo image index inlet input ivory jazzy jelly jewel jiffy \
jockey joker jolly judge juice jumbo jumpy kayak kebab kettle kiosk kitty knack knife knock koala label lace ladder lake lamp lance laser latch lava \
lemon level lilac limbo linen lion llama lobby lodge logic lotus lucky lunar lunch lyric macro magic mango manor maple march mask match mayor medal \
melon mercy merry metal meter mimic minty mocha model mojo money moose motor mound mouse movie muddy mural music nacho nadir naval neon nerve never \
ninja noble noise north notch novel nudge nutty oasis ocean olive omega onion opera orbit organ otter ounce outer oxide ozone paddy panda panel panic \
paper parka party pasta patch peach pearl pecan pedal penny peony perch piano pilot pinch pixel pizza plaid plank plaza plume plump poem polar polka \
poppy porch pouch power prism prize prune pulse puppy purse quack quail quart queen quest quick quiet quilt quirk quota rabbit radar radio rainy rally \
ranch raven razor rebel relay remix rhino rhyme ridge rival river roast robin robot rocky rodeo rogue roomy roost rover royal ruby rugby rumba rusty \
saber salad salsa salty sauna scale scarf scone scout scrap scuba sedan shark sheep shelf shell shiny shrub siren sixty skate skunk slate sleek slice \
slope sloth smile smoky snack snail snowy solar sonic spade spark spice spoon sport squid stack stamp steam stork storm stove sugar sunny super swamp \
swift syrup table taco talon tango tapir tasty teddy tempo thorn tiger toast token topaz torch totem tower track trail train trend tribe trout truck \
tulip tuna tunic turbo tweed twist ultra umbra uncle union unity urban usher vague valve vapor vault velvet venom venue verse video vigor vinyl viola \
viper visor vivid vocal voter wafer wagon waltz waste water whale wheat whisk windy witch wizard woody world woven wrath yacht yeast yodel yummy zebra \
zesty zippy zonal";

#[cfg(test)]
mod tests {
    use crate::interp::tests::{eval, show, try_eval};

    #[test]
    fn ids() {
        assert_eq!(eval("uuid().len").to_string(), "36");
        let words: Vec<_> = super::WORDS.split_whitespace().collect();
        assert_eq!(words.len(), 653);
        assert_eq!(words.iter().collect::<std::collections::HashSet<_>>().len(), words.len(), "duplicate passphrase words");
        assert_eq!(eval("[password().len, password(8).len, nanoid().len, ulid().len, passphrase(4).split(\"-\").len]").to_string(), "[20, 8, 21, 26, 4]");
        assert!(try_eval("password(3)").is_err());
    }

    #[test]
    fn uuid_info() {
        // RFC 9562 examples: both made at 2022-02-22 19:22:22 UTC.
        assert_eq!(show(r#"uuid_info("C232AB00-9414-11EC-B3C8-9F6BDECED846").timestamp.unix"#), "1645557742");
        assert_eq!(show(r#"uuid_info("017F22E2-79B0-7CC3-98C4-DC0C0C07398F").timestamp.unix"#), "1645557742");
        assert_eq!(show(r#"uuid_info(uuid())"#), r#"{version: 4, variant: "RFC 9562"}"#);
        assert!(try_eval(r#"uuid_info("nope")"#).is_err());
    }
}
