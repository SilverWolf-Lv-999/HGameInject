use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::config::toml::{TomlValue, parse_document, serialize_string};
use crate::error::InjectResult;

pub const COMMON_CONFIG_NAME: &str = "CommonConfig.toml";

#[derive(Debug, Clone, Default)]
pub struct CommonConfig {
    pub enabledModule: Vec<String>,
    pub keyBindings: BTreeMap<u32, String>,
    pub modulePatch: BTreeMap<String, Map<String, Value>>,
    dirty: bool,
}

impl CommonConfig {
    pub fn load(directory: &Path) -> CommonConfig {
        let path = directory.join(COMMON_CONFIG_NAME);
        match std::fs::read_to_string(&path) {
            Ok(content) => CommonConfig::parse(&content),
            Err(_) => {
                let config = CommonConfig::default();
                let _ = config.save_to(&path);
                config
            }
        }
    }

    pub fn parse(content: &str) -> CommonConfig {
        let mut config = CommonConfig::default();
        for (key, value) in parse_document(content) {
            match key.as_str() {
                "enabledModule" => {
                    if let TomlValue::Array(values) = value {
                        for entry in values {
                            if let TomlValue::Str(name) = entry {
                                config.enabledModule.push(name);
                            }
                        }
                    }
                }
                "keyBindings" => {
                    if let TomlValue::Table(entries) = value {
                        for (name, keyCode) in entries {
                            if let Some(keyCode) = keyCode.as_int() {
                                if keyCode >= 0 && keyCode <= u32::MAX as i64 {
                                    config.keyBindings.insert(keyCode as u32, name);
                                }
                            }
                        }
                    }
                }
                "module" => {
                    if let TomlValue::Table(modules) = value {
                        for (name, entries) in modules {
                            let mut patch = Map::new();
                            if let TomlValue::Table(entries) = entries {
                                for (entry, value) in entries {
                                    patch.insert(entry, toml_to_json(value));
                                }
                            }
                            config.modulePatch.insert(name, patch);
                        }
                    }
                }
                _ => {}
            }
        }
        config
    }

    pub fn save(&self, directory: &Path) -> InjectResult<()> {
        self.save_to(&directory.join(COMMON_CONFIG_NAME))
    }

    pub fn save_to(&self, path: &Path) -> InjectResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml())?;
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        let mut output = String::new();
        output.push_str("# HGameInject common configuration\n");
        output.push_str("enabledModule = [");
        for (index, name) in self.enabledModule.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(&serialize_string(name));
        }
        output.push_str("]\n");
        output.push_str("keyBindings = {");
        let mut first = true;
        for (keyCode, name) in &self.keyBindings {
            if !first {
                output.push_str(", ");
            }
            first = false;
            output.push_str(&format!("{} = {keyCode}", serialize_string(name)));
        }
        output.push_str("}\n");
        for (name, patch) in &self.modulePatch {
            output.push_str(&format!(
                "\n[module.{}]\n",
                serialize_string(name)
            ));
            for (entry, value) in patch {
                output.push_str(&format!(
                    "{} = {}\n",
                    serialize_string(entry),
                    value,
                ));
            }
        }
        output
    }

    pub fn enabled(&self, name: &str) -> bool {
        self.enabledModule.iter().any(|entry| entry == name)
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        let current = self.enabled(name);
        if current == enabled {
            return;
        }
        if enabled {
            self.enabledModule.push(name.to_owned());
        } else {
            self.enabledModule.retain(|entry| entry != name);
        }
        self.dirty = true;
    }

    pub fn set_key(&mut self, keyCode: u32, name: &str) {
        self.keyBindings.insert(keyCode, name.to_owned());
        self.dirty = true;
    }

    pub fn key(&self, name: &str) -> Option<u32> {
        self.keyBindings
            .iter()
            .find(|(_, entry)| entry.as_str() == name)
            .map(|(keyCode, _)| *keyCode)
    }

    pub fn patch(&mut self, name: &str) -> &mut Map<String, Value> {
        self.modulePatch
            .entry(name.to_owned())
            .or_insert_with(Map::new)
    }

    pub fn apply_patch(&mut self, name: &str, patch: Map<String, Value>) {
        if patch.is_empty() {
            return;
        }
        let entry = self.modulePatch.entry(name.to_owned()).or_insert_with(Map::new);
        for (key, value) in patch {
            entry.insert(key, value);
        }
        self.dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}

fn toml_to_json(value: TomlValue) -> Value {
    match value {
        TomlValue::Str(text) => json!(text),
        TomlValue::Int(number) => json!(number),
        TomlValue::Float(number) => json!(number),
        TomlValue::Bool(flag) => json!(flag),
        TomlValue::Array(values) => {
            Value::Array(values.into_iter().map(toml_to_json).collect())
        }
        TomlValue::Table(entries) => {
            let mut map = Map::new();
            for (key, value) in entries {
                map.insert(key, toml_to_json(value));
            }
            Value::Object(map)
        }
    }
}
