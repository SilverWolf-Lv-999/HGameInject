use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    Toggle,
    Range,
    Text,
    Select,
}

impl ConfigKind {
    pub const fn label(self) -> &'static str {
        match self {
            ConfigKind::Toggle => "toggle",
            ConfigKind::Range => "range",
            ConfigKind::Text => "text",
            ConfigKind::Select => "select",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConfigSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ConfigKind,
    pub default: ConfigDefault,
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
    pub choices: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigDefault {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(&'static str),
}

impl ConfigDefault {
    pub fn to_value(self) -> Value {
        match self {
            ConfigDefault::Bool(flag) => Value::Bool(flag),
            ConfigDefault::Int(number) => Value::Number(number.into()),
            ConfigDefault::Float(number) => serde_json::Number::from_f64(number)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            ConfigDefault::Text(text) => Value::String(text.to_owned()),
        }
    }

    pub const fn as_f64(self) -> Option<f64> {
        match self {
            ConfigDefault::Int(number) => Some(number as f64),
            ConfigDefault::Float(number) => Some(number),
            _ => None,
        }
    }
}

impl ConfigSchema {
    pub const fn toggle(key: &'static str, label: &'static str, default: bool) -> ConfigSchema {
        ConfigSchema {
            key,
            label,
            kind: ConfigKind::Toggle,
            default: ConfigDefault::Bool(default),
            minimum: 0.0,
            maximum: 0.0,
            step: 0.0,
            choices: &[],
        }
    }

    pub const fn range(
        key: &'static str,
        label: &'static str,
        default: f64,
        minimum: f64,
        maximum: f64,
        step: f64,
    ) -> ConfigSchema {
        ConfigSchema {
            key,
            label,
            kind: ConfigKind::Range,
            default: ConfigDefault::Float(default),
            minimum,
            maximum,
            step,
            choices: &[],
        }
    }

    pub const fn text(key: &'static str, label: &'static str, default: &'static str) -> ConfigSchema {
        ConfigSchema {
            key,
            label,
            kind: ConfigKind::Text,
            default: ConfigDefault::Text(default),
            minimum: 0.0,
            maximum: 0.0,
            step: 0.0,
            choices: &[],
        }
    }

    pub const fn select(
        key: &'static str,
        label: &'static str,
        default: &'static str,
        choices: &'static [&'static str],
    ) -> ConfigSchema {
        ConfigSchema {
            key,
            label,
            kind: ConfigKind::Select,
            default: ConfigDefault::Text(default),
            minimum: 0.0,
            maximum: 0.0,
            step: 0.0,
            choices,
        }
    }

    pub fn matches(&self, value: &Value) -> bool {
        match self.kind {
            ConfigKind::Toggle => value.is_boolean(),
            ConfigKind::Range => value.is_number(),
            ConfigKind::Text | ConfigKind::Select => value.is_string(),
        }
    }
}
