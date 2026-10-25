//! Placeholder text and names.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "placeholder",
    about: "filler text and made-up names for mockups and test data",
    #[rustfmt::skip]
    examples: &[
        ("placeholder", &[
            ("filler for a heading", "lorem(3)"),
            ("how many words in the whole paragraph", "lorem().split.len"),
            ("name the squad", "team_name()"),
            ("a test user", "person_name()"),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{FIRST_NAMES, LAST_NAMES, LOREM, pick, team_name as team};
    use crate::modules::Fail;

    /// the classic lorem ipsum paragraph of fake Latin that designers use as filler, or just its first n words (starting over if n is longer)
    #[builtin(examples = ["lorem(5)"], see = ["fortune"])]
    fn lorem(words: Option<i64>) -> Result<String, Fail> {
        let Some(n) = words else { return Ok(LOREM.into()) };
        if !(1..=100_000).contains(&n) {
            return Err(Fail::Arg(0, format!("word count must be 1 to 100000, got {n}")));
        }
        let words: Vec<_> = LOREM.split(' ').cycle().take(n as usize).collect();
        Ok(format!("{}.", words.join(" ").trim_end_matches([',', '.'])))
    }

    /// a random team name in one of several styles, like "The Crimson Otters", "Taco Squad" or "Lagos Lobsters"
    #[builtin(examples = ["team_name()"], see = ["person_name"])]
    fn team_name() -> String {
        team()
    }

    /// a random first and last name
    #[builtin(examples = ["person_name()"], see = ["team_name"])]
    fn person_name() -> String {
        format!("{} {}", pick(FIRST_NAMES), pick(LAST_NAMES))
    }
}

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. \
Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

const TEAM_ADJECTIVES: &[&str] = &[
    "Crimson",
    "Mighty",
    "Flying",
    "Electric",
    "Golden",
    "Rogue",
    "Silent",
    "Thundering",
    "Cosmic",
    "Iron",
    "Wild",
    "Midnight",
    "Atomic",
    "Fearless",
    "Turbo",
    "Sneaky",
    "Caffeinated",
    "Undefeated",
    "Rusty",
    "Lazy",
    "Furious",
    "Glorious",
    "Sleepy",
    "Fuzzy",
    "Majestic",
    "Grumpy",
    "Dancing",
    "Invisible",
    "Galactic",
    "Spicy",
    "Neon",
    "Feral",
    "Velvet",
    "Stormy",
    "Jolly",
    "Clumsy",
    "Radiant",
    "Unstoppable",
    "Wobbly",
    "Sparkly",
    "Chaotic",
    "Legendary",
    "Funky",
    "Hungry",
    "Dramatic",
];
const TEAM_NOUNS: &[&str] = &[
    "Otters",
    "Falcons",
    "Wolves",
    "Penguins",
    "Dragons",
    "Badgers",
    "Llamas",
    "Comets",
    "Pirates",
    "Ninjas",
    "Hedgehogs",
    "Krakens",
    "Squirrels",
    "Titans",
    "Ducks",
    "Honey Badgers",
    "Crabs",
    "Yetis",
    "Narwhals",
    "Goblins",
    "Raccoons",
    "Moose",
    "Platypuses",
    "Sharks",
    "Pandas",
    "Capybaras",
    "Flamingos",
    "Walruses",
    "Possums",
    "Vikings",
    "Wizards",
    "Robots",
    "Meatballs",
    "Potatoes",
    "Pickles",
    "Tacos",
    "Gnomes",
    "Unicorns",
    "Bananas",
    "Hamsters",
    "Geese",
    "Lobsters",
    "Sloths",
    "Thunderbolts",
    "Dumplings",
];
const FIRST_NAMES: &[&str] = &[
    "Alex",
    "Sam",
    "Jordan",
    "Taylor",
    "Morgan",
    "Casey",
    "Riley",
    "Jamie",
    "Avery",
    "Quinn",
    "Maria",
    "James",
    "Priya",
    "Wei",
    "Fatima",
    "Diego",
    "Aisha",
    "Liam",
    "Yuki",
    "Olga",
    "Kofi",
    "Elena",
    "Noah",
    "Sofia",
    "Omar",
    "Hana",
    "Lucas",
    "Amara",
    "Ivan",
    "Mei",
    "Chloe",
    "Mateo",
    "Zara",
    "Ravi",
    "Ingrid",
    "Tomás",
    "Leila",
    "Arjun",
    "Nia",
    "Hiroshi",
    "Sven",
    "Ananya",
    "Kwame",
    "Isabella",
    "Mohammed",
    "Freya",
    "Lucia",
    "Tariq",
    "Siobhan",
    "Valentina",
    "Jin",
    "Rosa",
    "Emeka",
    "Astrid",
    "Malik",
    "Camila",
    "Dmitri",
    "Leilani",
    "Kai",
    "Nadia",
    "Pablo",
    "Saanvi",
    "Tenzin",
    "Zeynep",
    "Bongani",
    "Ewa",
    "Rahul",
    "Ama",
];
const LAST_NAMES: &[&str] = &[
    "Smith",
    "Garcia",
    "Nguyen",
    "Kim",
    "Patel",
    "Müller",
    "Rossi",
    "Silva",
    "Cohen",
    "Okafor",
    "Tanaka",
    "Ivanova",
    "Johnson",
    "Lopez",
    "Chen",
    "Haddad",
    "Novak",
    "Andersen",
    "Murphy",
    "Kowalski",
    "Dubois",
    "Santos",
    "Ahmed",
    "Brown",
    "Park",
    "Yilmaz",
    "Larsen",
    "Mensah",
    "Sato",
    "Wang",
    "Singh",
    "Kumar",
    "Fernández",
    "Hernández",
    "Jansen",
    "Adeyemi",
    "Kariuki",
    "Nakamura",
    "Petrov",
    "Horvat",
    "Popescu",
    "Nowak",
    "Rahman",
    "Mendoza",
    "Castillo",
    "Okonkwo",
    "Kaur",
    "Takahashi",
    "O'Brien",
    "MacLeod",
    "Bianchi",
    "Moreau",
    "Schmidt",
    "Lindqvist",
    "Dlamini",
    "Abebe",
    "Tran",
    "Pham",
    "Reyes",
    "Papadopoulos",
    "Kaya",
    "Khan",
    "Rivera",
];

fn pick(list: &[&'static str]) -> &'static str {
    list[crate::host::rng(|r| r.usize(..list.len()))]
}

const GROUPS: &[&str] =
    &["Squad", "Crew", "Gang", "Collective", "Brigade", "Society", "Club", "Posse", "Syndicate", "Alliance", "Patrol", "Guild", "Federation"];
const CLUBS: &[&str] = &["FC", "United", "Athletic", "Rovers", "Wanderers", "All-Stars", "City"];

/// A team name in one of several styles: "The Crimson Otters", "The Wobbly Walruses", "Lagos Lobsters",
/// "Neon Hamsters FC", "Taco Squad", "Team Spicy", "Pickles & Penguins", "Osaka Wanderers".
fn team_name() -> String {
    let (adj, noun) = (pick(TEAM_ADJECTIVES), pick(TEAM_NOUNS));
    // Same first letter as the noun when one exists, for alliteration.
    let rhyme = |words: Vec<&'static str>| {
        let same: Vec<_> = words.iter().copied().filter(|w| w.chars().next() == noun.chars().next()).collect();
        pick(if same.is_empty() { &words } else { &same })
    };
    match crate::host::rng(|r| r.u8(..8)) {
        0 => format!("The {adj} {noun}"),
        1 => format!("The {} {noun}", rhyme(TEAM_ADJECTIVES.to_vec())),
        2 => format!("{} {noun}", rhyme(crate::modules::geo::city_names().collect())),
        3 => format!("{adj} {noun} {}", pick(CLUBS)),
        4 => format!("{} {}", singular(noun), pick(GROUPS)),
        5 => format!("Team {adj}"),
        6 => format!("{noun} & {}", pick(TEAM_NOUNS)),
        _ => format!("{} {}", pick(&crate::modules::geo::city_names().take(200).collect::<Vec<_>>()), pick(CLUBS)),
    }
}

/// "Geese" is "Goose", "Wolves" is "Wolf", "Walruses" is "Walrus", "Tacos" is "Taco".
fn singular(noun: &str) -> String {
    match noun {
        "Geese" => "Goose".into(),
        "Moose" => noun.into(),
        _ if noun.ends_with("ves") => format!("{}f", &noun[..noun.len() - 3]),
        _ if noun.ends_with("uses") || noun.ends_with("oes") => noun[..noun.len() - 2].into(),
        _ => noun.trim_end_matches('s').into(),
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn lorem() {
        assert_eq!(show("lorem(5)"), "Lorem ipsum dolor sit amet.");
        assert_eq!(show("lorem(8)"), "Lorem ipsum dolor sit amet, consectetur adipiscing elit.");
        assert_eq!(show("lorem(70).split.len"), "70");
        assert!(try_eval("lorem(0)").is_err());
    }

    #[test]
    fn names() {
        for _ in 0..500 {
            let t = super::team_name();
            assert!(!t.is_empty() && !t.contains("  ") && t.trim() == t, "{t:?}");
        }
        let one: Vec<_> = ["Geese", "Moose", "Wolves", "Walruses", "Platypuses", "Potatoes", "Tacos", "Honey Badgers"].map(super::singular).to_vec();
        assert_eq!(one, ["Goose", "Moose", "Wolf", "Walrus", "Platypus", "Potato", "Taco", "Honey Badger"]);
        assert_eq!(show("person_name().split.len"), "2");
    }
}
