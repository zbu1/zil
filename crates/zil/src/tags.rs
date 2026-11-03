//! The fixed list of tags functions and modules carry, each with its meaning; `help("tags")` shows it.

pub const TAGS: &[(&str, &str)] = &[
    ("conversion", "turning a value into another form or unit"),
    ("parsing", "reading text into values"),
    ("formatting", "turning values into text"),
    ("encoding", "reversible text and byte encodings"),
    ("crypto", "ciphers, hashes and other cryptography"),
    ("random", "randomness and generated values"),
    ("statistics", "averages, spreads and distributions"),
    ("geometry", "shapes, angles and coordinates"),
    ("sorting", "ordering values"),
    ("searching", "finding values inside others"),
    ("aggregation", "folding many values into one"),
    ("time", "dates, times and durations"),
    ("money", "currencies and prices"),
    ("files", "reading and writing files and folders"),
    ("network", "reaching other machines"),
];
