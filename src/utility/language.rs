use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

pub struct LanguageUtility;

static TRANSLATIONS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

impl LanguageUtility {
    pub fn initialize(path: &Path) {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let table = TRANSLATIONS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut table = table.write().unwrap();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(separator) = line.find('=') else {
                continue;
            };
            let key = line[..separator].trim();
            let value = line[separator + 1..].trim().trim_matches('"');
            if !key.is_empty() && !value.is_empty() {
                table.insert(key.to_owned(), value.to_owned());
            }
        }
    }

    pub fn translate(text: &str) -> String {
        TRANSLATIONS
            .get()
            .and_then(|table| {
                table
                    .read()
                    .unwrap()
                    .get(text)
                    .cloned()
            })
            .unwrap_or_else(|| text.to_owned())
    }

    pub fn set_translation(key: &str, value: &str) {
        let table = TRANSLATIONS.get_or_init(|| RwLock::new(HashMap::new()));
        table
            .write()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
    }

    pub fn count() -> usize {
        TRANSLATIONS
            .get()
            .map(|table| table.read().unwrap().len())
            .unwrap_or(0)
    }
}
