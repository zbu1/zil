//! How a session shows its results. Display reads the running session's, so outside one it sees the defaults.

use crate::value::Value;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Significant digits in a float, 1 to 17.
    pub precision: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { precision: 6 }
    }
}

/// A setting: the one definition `config`, the JS options and the Rust API all read.
pub struct Key {
    pub name: &'static str,
    pub ty: &'static str,
    pub doc: &'static str,
    get: fn(&Settings) -> Value,
    set: fn(&mut Settings, &Value) -> Option<()>,
}

pub const KEYS: &[Key] = &[Key {
    name: "precision",
    ty: "int",
    doc: "significant digits in a float, 1 to 17",
    get: |s| Value::int(s.precision as i64),
    set: |s, v| {
        s.precision = match v {
            Value::Int(n @ 1..=17, _) => *n as usize,
            _ => None?,
        };
        Some(())
    },
}];

fn key(name: &str) -> Result<&'static Key, String> {
    KEYS.iter()
        .find(|k| k.name == name)
        .ok_or_else(|| format!("no setting `{name}`{}", crate::error::did_you_mean(name, KEYS.iter().map(|k| k.name).chain(["permissions"]))))
}

impl Settings {
    pub fn get(&self, name: &str) -> Result<Value, String> {
        Ok((key(name)?.get)(self))
    }

    /// `name = v`, checked against the setting's type and range: `s.set("precision", &Value::int(4))`.
    pub fn set(&mut self, name: &str, v: &Value) -> Result<(), String> {
        let k = key(name)?;
        (k.set)(self, v).ok_or_else(|| format!("{name} is {}, got {}", k.doc, crate::error::short(v)))
    }

    /// Every setting and its value, as `config` shows them.
    pub fn to_map(&self) -> Value {
        Value::map(KEYS.iter().map(|k| (k.name.to_string(), (k.get)(self))).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    fn err(src: &str) -> String {
        try_eval(src).unwrap_err().msg
    }

    #[test]
    fn config() {
        assert!(show("config").starts_with(r#"{precision: 6, permissions: ["files.read", "#));
        assert_eq!(show(r#"config.precision = 3; "{config.precision}: {1 / 3}""#), "3: 0.333");
        assert_eq!(err("config.precision = 0"), "precision is significant digits in a float, 1 to 17, got `0`");
        assert_eq!(try_eval("config.precison").unwrap_err().help, ["did you mean `precision`?"]);
        assert_eq!(err("config.precison = 2"), err("config.precison"));
        assert!(err("config = 1").starts_with("`config` is reserved"));
        assert_eq!(show("config.keys"), r#"["precision", "permissions"]"#);
    }
}
