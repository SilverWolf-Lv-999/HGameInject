use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::toml::{TomlValue, parse_document, serialize_string};
use crate::error::InjectResult;

pub const CLIENT_CONFIG_NAME: &str = "ClientConfig.toml";

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub logLevel: String,
    pub debugOutput: bool,
    pub attachConsole: bool,
    pub notifications: bool,
    pub saveOnExit: bool,
    pub webPort: u16,
    pub autoOpenBrowser: bool,
    pub dllPath: String,
    pub targetDirectories: Vec<String>,
    pub windowTitleFilter: String,
    pub refreshIntervalSeconds: u64,
    pub keyBindings: BTreeMap<String, i64>,
    dirty: bool,
}

impl Default for ClientConfig {
    fn default() -> ClientConfig {
        ClientConfig {
            logLevel: "info".to_owned(),
            debugOutput: true,
            attachConsole: false,
            notifications: true,
            saveOnExit: true,
            webPort: 23119,
            autoOpenBrowser: true,
            dllPath: String::new(),
            targetDirectories: default_target_directories(),
            windowTitleFilter: String::new(),
            refreshIntervalSeconds: 8,
            keyBindings: BTreeMap::new(),
            dirty: false,
        }
    }
}

impl ClientConfig {
    pub fn load(directory: &Path) -> ClientConfig {
        let path = directory.join(CLIENT_CONFIG_NAME);
        match std::fs::read_to_string(&path) {
            Ok(content) => ClientConfig::parse(&content),
            Err(_) => {
                let config = ClientConfig::default();
                let _ = config.save_to(&path);
                config
            }
        }
    }

    pub fn parse(content: &str) -> ClientConfig {
        let mut config = ClientConfig::default();
        for (key, value) in parse_document(content) {
            match key.as_str() {
                "logLevel" => {
                    if let TomlValue::Str(text) = value {
                        config.logLevel = text;
                    }
                }
                "debugOutput" => config.debugOutput = value.as_bool().unwrap_or(config.debugOutput),
                "attachConsole" => {
                    config.attachConsole = value.as_bool().unwrap_or(config.attachConsole);
                }
                "notifications" => {
                    config.notifications = value.as_bool().unwrap_or(config.notifications);
                }
                "saveOnExit" => config.saveOnExit = value.as_bool().unwrap_or(config.saveOnExit),
                "webPort" => {
                    if let Some(port) = value.as_int() {
                        if port > 0 && port <= u16::MAX as i64 {
                            config.webPort = port as u16;
                        }
                    }
                }
                "autoOpenBrowser" => {
                    config.autoOpenBrowser =
                        value.as_bool().unwrap_or(config.autoOpenBrowser);
                }
                "dllPath" => {
                    if let TomlValue::Str(text) = value {
                        config.dllPath = text;
                    }
                }
                "targetDirectories" => {
                    if let TomlValue::Array(values) = value {
                        config.targetDirectories.clear();
                        for entry in values {
                            if let TomlValue::Str(text) = entry {
                                config.targetDirectories.push(text);
                            }
                        }
                    }
                }
                "windowTitleFilter" => {
                    if let TomlValue::Str(text) = value {
                        config.windowTitleFilter = text;
                    }
                }
                "refreshIntervalSeconds" => {
                    if let Some(interval) = value.as_int() {
                        if (1..=600).contains(&interval) {
                            config.refreshIntervalSeconds = interval as u64;
                        }
                    }
                }
                "keyBindings" => {
                    if let TomlValue::Table(entries) = value {
                        for (command, keyCode) in entries {
                            if let Some(keyCode) = keyCode.as_int() {
                                config.keyBindings.insert(command, keyCode);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        config
    }

    pub fn save(&self, directory: &Path) -> InjectResult<()> {
        self.save_to(&directory.join(CLIENT_CONFIG_NAME))
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
        output.push_str("# HGameInject client configuration\n");
        output.push_str(&format!("logLevel = {}\n", serialize_string(&self.logLevel)));
        output.push_str(&format!("debugOutput = {}\n", self.debugOutput));
        output.push_str(&format!("attachConsole = {}\n", self.attachConsole));
        output.push_str(&format!("notifications = {}\n", self.notifications));
        output.push_str(&format!("saveOnExit = {}\n", self.saveOnExit));
        output.push_str(&format!("webPort = {}\n", self.webPort));
        output.push_str(&format!(
            "autoOpenBrowser = {}\n",
            self.autoOpenBrowser
        ));
        output.push_str(&format!("dllPath = {}\n", serialize_string(&self.dllPath)));
        output.push_str(&format!(
            "windowTitleFilter = {}\n",
            serialize_string(&self.windowTitleFilter)
        ));
        output.push_str(&format!(
            "refreshIntervalSeconds = {}\n",
            self.refreshIntervalSeconds
        ));
        output.push_str("targetDirectories = [");
        for (index, directory) in self.targetDirectories.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(&serialize_string(directory));
        }
        output.push_str("]\n");
        output.push_str("keyBindings = {");
        let mut first = true;
        for (command, keyCode) in &self.keyBindings {
            if !first {
                output.push_str(", ");
            }
            first = false;
            output.push_str(&format!("{} = {keyCode}", serialize_string(command)));
        }
        output.push_str("}\n");
        output
    }

    pub fn bind(&mut self, command: &str, keyCode: i64) {
        self.keyBindings.insert(command.to_owned(), keyCode);
        self.dirty = true;
    }

    pub fn unbind(&mut self, command: &str) {
        if self.keyBindings.remove(command).is_some() {
            self.dirty = true;
        }
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

    pub fn path(directory: &Path) -> PathBuf {
        directory.join(CLIENT_CONFIG_NAME)
    }
}

pub fn default_target_directories() -> Vec<String> {
    vec!["D:/games/other".to_owned()]
}

pub fn config_directory() -> PathBuf {
    let root = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    root.join("common").join("config")
}
