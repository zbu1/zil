//! Translation through MyMemory (api.mymemory.translated.net), cached on disk so repeats work offline.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "translation",
    about: "translate text between languages, using an online service",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Translating text between languages, using an online service."),
        ("online", "\
Translating sends the text to the service. \
Translations are cached, so ones you've already done work offline."),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{lang, translate as online};
    use crate::modules::Fail;

    /// translate text into the language to, named by code or English name, like "es", "zh-CN" or "spanish"
    ///
    /// The language it's in is detected unless you give from
    #[builtin(shown = [r#""good morning".translate("es")"#, r#"translate("Guten Morgen", "english", "german")"#])]
    fn translate(text: &str, to: &str, from: Option<&str>) -> Result<String, Fail> {
        let to = lang(to).map_err(|e| Fail::Arg(1, e))?;
        let from = match from {
            Some(f) => lang(f).map_err(|e| Fail::Arg(2, e))?,
            None => "autodetect",
        };
        Ok(online(text, from, to)?)
    }
}

/// English names of common languages and their codes; anything code-shaped (`pt-BR`) passes through as is.
#[rustfmt::skip]
const LANGS: &[(&str, &str)] = &[
    ("arabic", "ar"), ("bengali", "bn"), ("bulgarian", "bg"), ("catalan", "ca"), ("chinese", "zh-CN"), ("croatian", "hr"), ("czech", "cs"),
    ("danish", "da"), ("dutch", "nl"), ("english", "en"), ("estonian", "et"), ("finnish", "fi"), ("french", "fr"), ("german", "de"),
    ("greek", "el"), ("hebrew", "he"), ("hindi", "hi"), ("hungarian", "hu"), ("icelandic", "is"), ("indonesian", "id"), ("irish", "ga"),
    ("italian", "it"), ("japanese", "ja"), ("korean", "ko"), ("latin", "la"), ("latvian", "lv"), ("lithuanian", "lt"), ("malay", "ms"),
    ("norwegian", "no"), ("persian", "fa"), ("polish", "pl"), ("portuguese", "pt"), ("romanian", "ro"), ("russian", "ru"), ("serbian", "sr"),
    ("slovak", "sk"), ("slovenian", "sl"), ("spanish", "es"), ("swahili", "sw"), ("swedish", "sv"), ("tagalog", "tl"), ("thai", "th"),
    ("turkish", "tr"), ("ukrainian", "uk"), ("urdu", "ur"), ("vietnamese", "vi"), ("welsh", "cy"),
];

/// A language code from a code or an English name.
fn lang(s: &str) -> Result<&str, String> {
    let lower = s.to_lowercase();
    if let Some((_, code)) = LANGS.iter().find(|(name, code)| *name == lower || code.to_lowercase() == lower) {
        return Ok(code);
    }
    // `es`, `fil`, `pt-BR`, `zh-TW`: let the service judge codes it might know.
    let (base, region) = s.split_once('-').unwrap_or((s, "AA"));
    if (2..=3).contains(&base.len()) && base.bytes().all(|b| b.is_ascii_alphabetic()) && region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Ok(s);
    }
    Err(format!("unknown language `{s}`{}", crate::error::did_you_mean(&lower, LANGS.iter().map(|l| l.0))))
}

/// From the disk cache, else online; the result is cached for next time.
fn translate(text: &str, from: &str, to: &str) -> Result<String, String> {
    if from == to {
        return Ok(text.into());
    }
    crate::modules::sys::cached("translations.json", &format!("{from}|{to}|{text}"), || online(text, from, to))
}

fn online(text: &str, from: &str, to: &str) -> Result<String, String> {
    let body = crate::modules::sys::get("https://api.mymemory.translated.net/get", &[("q", text), ("langpair", &format!("{from}|{to}"))])?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    // Errors arrive as HTTP 200 with a string status like "403" and a shouted message.
    if json["responseStatus"] != 200 {
        let why = json["responseDetails"].as_str().unwrap_or("unexpected response").to_lowercase();
        // Auto-detect found the target language already.
        if why.contains("two distinct languages") {
            return Ok(text.into());
        }
        return Err(why);
    }
    Ok(json["responseData"]["translatedText"].as_str().ok_or("unexpected response")?.to_string())
}

#[cfg(test)]
mod tests {
    use super::lang;
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn languages() {
        assert_eq!(lang("Spanish"), Ok("es"));
        assert_eq!(lang("ZH-cn"), Ok("zh-CN"));
        assert_eq!(lang("pt-BR"), Ok("pt-BR"));
        assert_eq!(lang("fil"), Ok("fil"));
        assert!(lang("spansh").unwrap_err().contains("`spanish`"));
        assert!(lang("english please").is_err());
    }

    #[test]
    fn offline() {
        assert_eq!(show(r#""same".translate("en", "english")"#), "same");
        let e = try_eval(r#""zil test phrase, surely uncached".translate("es")"#).unwrap_err();
        assert!(e.msg.contains("network access is disabled"), "{}", e.msg);
    }
}
