use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::config::toml::TomlValue;
use crate::error::{InjectError, InjectResult};

pub const STORE_FILE_EXTENSION: &str = ".json";
pub const EMPTY_OBJECT: &str = "{}";

pub struct ConfigStore {
    directory: PathBuf,
    modules: RwLock<BTreeMap<String, Map<String, Value>>>,
    dirty: RwLock<Vec<String>>,
}

impl ConfigStore {
    pub fn new(directory: PathBuf) -> ConfigStore {
        ConfigStore {
            directory,
            modules: RwLock::new(BTreeMap::new()),
            dirty: RwLock::new(Vec::new()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load_module(&self, name: &str) -> InjectResult<()> {
        if self.modules.read().contains_key(name) {
            return Ok(());
        }
        let path = self.file_for(name);
        let patch = match std::fs::read_to_string(&path) {
            Ok(content) => {
                let parsed: Value = serde_json::from_str(&content)
                    .map_err(|error| InjectError::Config(error.to_string()))?;
                match parsed {
                    Value::Object(entries) => entries,
                    _ => Map::new(),
                }
            }
            Err(_) => Map::new(),
        };
        self.modules.write().insert(name.to_owned(), patch);
        Ok(())
    }

    pub fn seed(&self, name: &str, patch: Map<String, Value>) {
        let mut modules = self.modules.write();
        if !modules.contains_key(name) {
            modules.insert(name.to_owned(), Map::new());
        }
        if let Some(entry) = modules.get_mut(name) {
            for (key, value) in patch {
                entry.insert(key, value);
            }
        }
    }

    pub fn seed_toml(&self, name: &str, patch: &[(String, TomlValue)]) {
        let mut map = Map::new();
        for (key, value) in patch {
            map.insert(key.clone(), toml_to_json(value));
        }
        self.seed(name, map);
    }

    pub fn registers_module(&self, name: &str) -> bool {
        self.modules.read().contains_key(name)
    }

    pub fn online_modules(&self) -> Vec<String> {
        self.modules.read().keys().cloned().collect()
    }

    pub fn read<T: DeserializeOwned>(&self, name: &str, key: &str, fallback: T) -> T {
        self.modules
            .read()
            .get(name)
            .and_then(|module| module.get(key))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or(fallback)
    }

    pub fn write<T: Serialize>(&self, name: &str, key: &str, value: &T) -> InjectResult<()> {
        let encoded = serde_json::to_value(value)?;
        self.write_value(name, key, encoded)
    }

    pub fn write_value(&self, name: &str, key: &str, value: Value) -> InjectResult<()> {
        {
            let mut modules = self.modules.write();
            let module = modules.entry(name.to_owned()).or_insert_with(Map::new);
            match value {
                Value::Null => {
                    module.remove(key);
                }
                other => {
                    module.insert(key.to_owned(), other);
                }
            }
        }
        let mut dirty = self.dirty.write();
        if !dirty.iter().any(|entry| entry == name) {
            dirty.push(name.to_owned());
        }
        Ok(())
    }

    pub fn patch(&self, name: &str) -> Map<String, Value> {
        self.modules
            .read()
            .get(name)
            .cloned()
            .unwrap_or_else(Map::new)
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty.read().is_empty()
    }

    pub fn commit(&self, name: &str) -> InjectResult<()> {
        let patch = self.patch(name);
        let path = self.file_for(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content =
            serde_json::to_string_pretty(&patch).map_err(|error| InjectError::Config(error.to_string()))?;
        std::fs::write(path, content)?;
        self.dirty.write().retain(|entry| entry != name);
        Ok(())
    }

    pub fn commit_dirty(&self) -> InjectResult<()> {
        let pending: Vec<String> = self.dirty.read().clone();
        for name in pending {
            self.commit(&name)?;
        }
        Ok(())
    }

    pub fn file_for(&self, name: &str) -> PathBuf {
        let sanitized = sanitize(name);
        self.directory
            .join(format!("{sanitized}{STORE_FILE_EXTENSION}"))
    }
}

pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn toml_to_json(value: &TomlValue) -> Value {
    match value {
        TomlValue::Str(text) => Value::String(text.clone()),
        TomlValue::Int(number) => Value::Number((*number).into()),
        TomlValue::Float(number) => serde_json::Number::from_f64(*number)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        TomlValue::Bool(flag) => Value::Bool(*flag),
        TomlValue::Array(values) => Value::Array(values.iter().map(toml_to_json).collect()),
        TomlValue::Table(entries) => {
            let mut map = Map::new();
            for (key, value) in entries {
                map.insert(key.clone(), toml_to_json(value));
            }
            Value::Object(map)
        }
    }
}

pub fn empty_object() -> Value {
    serde_json::from_str(EMPTY_OBJECT).unwrap_or(Value::Object(Map::new()))
}
