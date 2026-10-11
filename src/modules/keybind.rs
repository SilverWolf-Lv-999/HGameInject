use std::collections::HashMap;

use parking_lot::RwLock;

#[derive(Debug, Default)]
pub struct KeyBindings {
    bindings: RwLock<HashMap<u32, String>>,
}

impl KeyBindings {
    pub fn new() -> KeyBindings {
        KeyBindings {
            bindings: RwLock::new(HashMap::new()),
        }
    }

    pub fn bind(&self, key_code: u32, module: &str) {
        self.bindings.write().insert(key_code, module.to_owned());
    }

    pub fn unbind(&self, key_code: u32) {
        self.bindings.write().remove(&key_code);
    }

    pub fn module_for(&self, key_code: u32) -> Option<String> {
        self.bindings
            .read()
            .get(&key_code)
            .cloned()
    }

    pub fn key_for(&self, module: &str) -> Option<u32> {
        self.bindings
            .read()
            .iter()
            .find(|(_, name)| name.as_str() == module)
            .map(|(key_code, _)| *key_code)
    }

    pub fn entries(&self) -> Vec<(u32, String)> {
        self.bindings
            .read()
            .iter()
            .map(|(key_code, name)| (*key_code, name.clone()))
            .collect()
    }
}
