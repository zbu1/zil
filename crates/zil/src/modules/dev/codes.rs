//! Check digits (Luhn, ISBN, IBAN) and lookup tables (HTTP statuses, MIME types).

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "codes",
    about: "catch typos in card numbers, ISBNs and IBANs, and look up HTTP status codes and file types",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Check digits for card, book and bank account numbers, and lookups for HTTP status codes and file types. A check digit \
is an extra digit worked out from the others, so mistyping or swapping a digit almost always makes the whole number \
invalid."),
        ("good to know", "\
Passing the check only means a number is well formed, not that the card, book or account exists."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("codes", &[
            ("a typo in a card number?", r#"luhn("4111 1111 1111 1112")"#),
            ("valid ISBN?", r#"isbn("978-0-306-40615-7")"#),
            ("what's a 418?", "http_status(418)"),
            ("Content-Type for a file", r#"mime("report.pdf")"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("check digits", &["luhn", "isbn", "iban"]),
        ("lookups", &["http_status", "mime"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{HTTP, MIME};
    use crate::signature::Or;

    /// whether a number passes the Luhn check, the check digit on credit cards and phone IMEIs
    ///
    /// Spaces and dashes are ignored
    #[builtin(examples = [r#"luhn("4111 1111 1111 1111")"#, "luhn(79927398710)"], see = ["isbn", "iban"])]
    fn luhn(s: Or<&str, i64>) -> bool {
        let s = match s {
            Or::A(s) => s.to_string(),
            Or::B(n) => n.to_string(),
        };
        let digits: Option<Vec<u32>> = s.chars().filter(|c| !matches!(c, ' ' | '-')).map(|c| c.to_digit(10)).collect();
        match digits {
            Some(d) if d.len() >= 2 => {
                let sum: u32 = d.iter().rev().enumerate().map(|(i, &x)| if i % 2 == 1 { if x * 2 > 9 { x * 2 - 9 } else { x * 2 } } else { x }).sum();
                sum.is_multiple_of(10)
            }
            _ => false,
        }
    }

    /// whether an ISBN-10 or ISBN-13 has a valid check digit
    #[builtin(examples = [r#"isbn("0-306-40615-2")"#, r#"isbn("9780306406157")"#], see = ["luhn"])]
    fn isbn(s: &str) -> bool {
        let cs: Vec<char> = s.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
        let d = |c: &char| c.to_digit(10);
        match cs.len() {
            10 => {
                let check = if matches!(cs[9], 'X' | 'x') { Some(10) } else { d(&cs[9]) };
                let body: Option<Vec<u32>> = cs[..9].iter().map(d).collect();
                matches!((body, check), (Some(b), Some(c)) if (b.iter().enumerate().map(|(i, x)| (10 - i as u32) * x).sum::<u32>() + c) % 11 == 0)
            }
            13 => cs
                .iter()
                .map(d)
                .collect::<Option<Vec<u32>>>()
                .is_some_and(|ds| ds.iter().enumerate().map(|(i, x)| if i % 2 == 1 { 3 * x } else { *x }).sum::<u32>() % 10 == 0),
            _ => false,
        }
    }

    /// whether an international bank account number (IBAN) passes its check (the whole number mod 97 must be 1)
    #[builtin(examples = [r#"iban("GB82 WEST 1234 5698 7654 32")"#], see = ["luhn"])]
    fn iban(s: &str) -> bool {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
        let ok = s.len() >= 15 && s.len() <= 34 && s.chars().all(|c| c.is_ascii_alphanumeric()) && s[..2].chars().all(|c| c.is_ascii_alphabetic());
        // Move the first four characters to the end, letters become 10..35, and the number mod 97 must be 1.
        let rem = s[4.min(s.len())..].chars().chain(s.chars().take(4)).try_fold(0u64, |acc, c| {
            let n = c.to_digit(36)? as u64;
            Some(if n >= 10 { (acc * 100 + n) % 97 } else { (acc * 10 + n) % 97 })
        });
        ok && rem == Some(1)
    }

    /// the standard name for an HTTP status code, like "Not Found" for 404, or nil if it isn't one
    #[builtin(examples = ["http_status(404)", "(200..=204).map(http_status)"], see = ["mime"])]
    fn http_status(code: i64) -> Option<&'static str> {
        HTTP.iter().find(|h| h.0 == code).map(|h| h.1)
    }

    /// the MIME type for a file name or extension, like "image/png", which is what a web server sends as Content-Type, or nil if unknown
    #[builtin(examples = [r#"mime("png")"#, r#"mime("data.json")"#], see = ["ext"])]
    fn mime(name: &str) -> Option<&'static str> {
        let ext = name.rsplit('.').next().unwrap_or(name).to_lowercase();
        MIME.iter().find(|m| m.0.split(' ').any(|e| e == ext)).map(|m| m.1)
    }
}

#[rustfmt::skip]
const HTTP: &[(i64, &str)] = &[
    (100, "Continue"), (101, "Switching Protocols"), (103, "Early Hints"),
    (200, "OK"), (201, "Created"), (202, "Accepted"), (203, "Non-Authoritative Information"), (204, "No Content"), (205, "Reset Content"), (206, "Partial Content"),
    (300, "Multiple Choices"), (301, "Moved Permanently"), (302, "Found"), (303, "See Other"), (304, "Not Modified"), (307, "Temporary Redirect"), (308, "Permanent Redirect"),
    (400, "Bad Request"), (401, "Unauthorized"), (402, "Payment Required"), (403, "Forbidden"), (404, "Not Found"), (405, "Method Not Allowed"), (406, "Not Acceptable"),
    (407, "Proxy Authentication Required"), (408, "Request Timeout"), (409, "Conflict"), (410, "Gone"), (411, "Length Required"), (412, "Precondition Failed"),
    (413, "Content Too Large"), (414, "URI Too Long"), (415, "Unsupported Media Type"), (416, "Range Not Satisfiable"), (417, "Expectation Failed"), (418, "I'm a teapot"),
    (421, "Misdirected Request"), (422, "Unprocessable Content"), (423, "Locked"), (424, "Failed Dependency"), (425, "Too Early"), (426, "Upgrade Required"),
    (428, "Precondition Required"), (429, "Too Many Requests"), (431, "Request Header Fields Too Large"), (451, "Unavailable For Legal Reasons"),
    (500, "Internal Server Error"), (501, "Not Implemented"), (502, "Bad Gateway"), (503, "Service Unavailable"), (504, "Gateway Timeout"), (505, "HTTP Version Not Supported"),
    (506, "Variant Also Negotiates"), (507, "Insufficient Storage"), (508, "Loop Detected"), (510, "Not Extended"), (511, "Network Authentication Required"),
];

#[rustfmt::skip]
const MIME: &[(&str, &str)] = &[
    ("html htm", "text/html"), ("css", "text/css"), ("js mjs", "text/javascript"), ("txt", "text/plain"), ("csv", "text/csv"), ("md", "text/markdown"),
    ("xml", "application/xml"), ("json", "application/json"), ("pdf", "application/pdf"), ("zip", "application/zip"), ("gz", "application/gzip"),
    ("tar", "application/x-tar"), ("wasm", "application/wasm"), ("yaml yml", "application/yaml"), ("toml", "application/toml"),
    ("doc", "application/msword"), ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ("xls", "application/vnd.ms-excel"), ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ("ppt", "application/vnd.ms-powerpoint"), ("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
    ("png", "image/png"), ("jpg jpeg", "image/jpeg"), ("gif", "image/gif"), ("webp", "image/webp"), ("avif", "image/avif"), ("svg", "image/svg+xml"),
    ("ico", "image/vnd.microsoft.icon"), ("bmp", "image/bmp"), ("tif tiff", "image/tiff"),
    ("mp3", "audio/mpeg"), ("wav", "audio/wav"), ("ogg", "audio/ogg"), ("flac", "audio/flac"), ("m4a", "audio/mp4"),
    ("mp4", "video/mp4"), ("webm", "video/webm"), ("mov", "video/quicktime"), ("mkv", "video/x-matroska"), ("avi", "video/x-msvideo"),
    ("woff", "font/woff"), ("woff2", "font/woff2"), ("ttf", "font/ttf"), ("otf", "font/otf"),
];

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn codes() {
        assert_eq!(
            show(r#"[luhn("4111 1111 1111 1111"), luhn("4111-1111-1111-1112"), luhn(79927398713), luhn("abc"), luhn("0")]"#),
            "[true, false, true, false, false]"
        );
        assert_eq!(
            show(r#"[isbn("0-306-40615-2"), isbn("0306406153"), isbn("978-0-306-40615-7"), isbn("9780306406158"), isbn("080442957X")]"#),
            "[true, false, true, false, true]"
        );
        assert_eq!(
            show(r#"[iban("GB82 WEST 1234 5698 7654 32"), iban("GB82 WEST 1234 5698 7654 33"), iban("DE89370400440532013000"), iban("xx")]"#),
            "[true, false, true, false]"
        );
        assert_eq!(show(r#"[http_status(418), http_status(999)]"#), r#"["I'm a teapot", nil]"#);
        assert_eq!(show(r#"[mime("PNG"), mime("a/b.tar.gz"), mime(".json"), mime("nope")]"#), r#"["image/png", "application/gzip", "application/json", nil]"#);
    }
}
