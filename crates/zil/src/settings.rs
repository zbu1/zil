//! How a session shows its results. Display reads the running session's, so outside one it sees the defaults.

use crate::value::Value;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Significant digits in a float, 1 to 17.
    pub precision: usize,
    /// `x / 0` gives `∞`, `-∞` or `undefined` instead of an error.
    pub div_zero_values: bool,
    /// `billion` is 10^12 and `milliard` 10^9, as in the long scale, instead of the short scale.
    pub long_scale: bool,
    /// The base plain integer results show in, 2 to 36; not a `config` key, the CLI's `--base` sets it.
    pub base: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { precision: 6, div_zero_values: false, long_scale: false, base: 10 }
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

pub const KEYS: &[Key] = &[
    Key {
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
    },
    Key {
        name: "div_zero",
        ty: "str",
        doc: "\"error\" or \"values\": what dividing by zero does",
        get: |s| Value::str(if s.div_zero_values { "values" } else { "error" }),
        set: |s, v| {
            s.div_zero_values = match v {
                Value::Str(x) if &**x == "values" => true,
                Value::Str(x) if &**x == "error" => false,
                _ => None?,
            };
            Some(())
        },
    },
    Key {
        name: "number_scale",
        ty: "str",
        doc: "\"short\" or \"long\": whether a billion is 10^9 or 10^12, in unit names and in words",
        get: |s| Value::str(if s.long_scale { "long" } else { "short" }),
        set: |s, v| {
            s.long_scale = match v {
                Value::Str(x) if &**x == "long" => true,
                Value::Str(x) if &**x == "short" => false,
                _ => None?,
            };
            Some(())
        },
    },
];

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
        assert!(show("config").starts_with(r#"{precision: 6, div_zero: "error", number_scale: "short", permissions: ["files.read", "#));
        assert_eq!(show(r#"config.precision = 3; "{config.precision}: {1 / 3}""#), "3: 0.333");
        assert_eq!(err("config.precision = 0"), "precision is significant digits in a float, 1 to 17, got `0`");
        assert_eq!(try_eval("config.precison").unwrap_err().help, ["did you mean `precision`?"]);
        assert_eq!(err("config.precison = 2"), err("config.precison"));
        assert!(err("config = 1").starts_with("`config` is reserved"));
        assert_eq!(show("config.keys"), r#"["precision", "div_zero", "number_scale", "permissions"]"#);
        assert_eq!(err("1 / 0"), "division by zero");
        assert_eq!(show(r#"config.div_zero = "values"; [1 / 0, -1 / 0, 0 / 0, 1 // 0]"#), "[∞, -∞, undefined, ∞]");
        assert!(err(r#"config.div_zero = "x""#).contains("div_zero"));
    }
}
