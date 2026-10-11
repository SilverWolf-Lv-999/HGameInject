use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use parking_lot::RwLock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::ConfigStore;
use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::schema::{ConfigKind, ConfigSchema};
use crate::protocol::{ConfigSchema as ProtocolSchema, ModuleSnapshot};

pub trait Module: Send + Sync {
    fn names(&self) -> &'static str;

    fn context(&self) -> &RwLock<ModuleContext>;

    fn attach(&self, _state: Arc<ModuleState>) {}

    fn schema(&self) -> &'static [ConfigSchema] {
        &[]
    }

    fn on_config(&self, _context: &ModuleContext, _key: &str) {}

    fn on_enable(&self, context: &ModuleContext);

    fn on_disable(&self, context: &ModuleContext);

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        let _ = (context, kind);
    }

    fn on_key(&self, context: &ModuleContext, key_code: u32) {
        let _ = (context, key_code);
    }
}

pub struct TargetModule {
    module: Box<dyn Module>,
    runtime: ModuleRuntime,
    state: RwLock<Arc<ModuleState>>,
}

impl TargetModule {
    pub fn new(module: Box<dyn Module>) -> TargetModule {
        TargetModule {
            module,
            runtime: ModuleRuntime::new(),
            state: RwLock::new(Arc::new(ModuleState::new())),
        }
    }

    pub fn module(&self) -> &dyn Module {
        self.module.as_ref()
    }

    pub fn runtime(&self) -> &ModuleRuntime {
        &self.runtime
    }

    pub fn name(&self) -> &'static str {
        self.module.names()
    }

    pub fn context(&self) -> &RwLock<ModuleContext> {
        self.module.context()
    }

    pub fn state(&self) -> ModuleState {
        ModuleState::clone_state(&self.state)
    }

    pub fn state_handle(&self) -> Arc<ModuleState> {
        Arc::clone(&self.state.read())
    }

    pub fn bind_state(&self, state: &Arc<ModuleState>) {
        *self.state.write() = Arc::clone(state);
    }

    pub fn schema(&self) -> &'static [ConfigSchema] {
        self.module.schema()
    }

    pub fn event_flags(&self) -> u32 {
        self.context().read().info().eventFlags
    }

    pub fn snapshot(&self) -> ModuleSnapshot {
        let info = self.context().read().info();
        let state = self.state();
        let mut values = serde_json::Map::new();
        let mut schema = Vec::with_capacity(self.schema().len());
        for entry in self.schema() {
            values.insert(
                entry.key.to_owned(),
                state.read_json(entry.key, entry.default.to_value()),
            );
            let ranged = matches!(entry.kind, ConfigKind::Range);
            schema.push(ProtocolSchema {
                key: entry.key.to_owned(),
                label: entry.label.to_owned(),
                kind: entry.kind.label().to_owned(),
                default: entry.default.to_value(),
                min: ranged.then_some(entry.minimum),
                max: ranged.then_some(entry.maximum),
                step: ranged.then_some(entry.step),
                choices: entry
                    .choices
                    .iter()
                    .map(|choice| (*choice).to_owned())
                    .collect(),
            });
        }
        ModuleSnapshot {
            name: info.name.to_owned(),
            description: info.description.to_owned(),
            category: info.category.label().to_owned(),
            keybind: info.keybind,
            enabled: self.runtime().enabled(),
            schema,
            values: Value::Object(values),
        }
    }

    pub fn on_enable(&self, context: &ModuleContext) {
        self.module.on_enable(context);
    }

    pub fn on_disable(&self, context: &ModuleContext) {
        self.module.on_disable(context);
    }

    pub fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        self.module.on_event(context, kind);
    }

    pub fn on_key(&self, context: &ModuleContext, key_code: u32) {
        self.module.on_key(context, key_code);
    }

    pub fn on_config(&self, context: &ModuleContext, key: &str) {
        self.module.on_config(context, key);
    }
}

pub struct ModuleRuntime {
    enabled: AtomicBool,
}

impl ModuleRuntime {
    pub const fn new() -> ModuleRuntime {
        ModuleRuntime {
            enabled: AtomicBool::new(false),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set(&self, enabled: bool) -> bool {
        self.enabled.swap(enabled, Ordering::AcqRel)
    }
}

pub struct ModuleState {
    module: RwLock<Weak<TargetModule>>,
}

impl ModuleState {
    pub fn new() -> ModuleState {
        ModuleState {
            module: RwLock::new(Weak::new()),
        }
    }

    pub fn with_module(module: Weak<TargetModule>) -> ModuleState {
        ModuleState {
            module: RwLock::new(module),
        }
    }

    pub fn clone_state(source: &RwLock<Arc<ModuleState>>) -> ModuleState {
        ModuleState {
            module: RwLock::new(source.read().module.read().clone()),
        }
    }

    pub fn upgrade(&self) -> Option<Arc<TargetModule>> {
        self.module.read().upgrade()
    }

    pub fn read_json(&self, key: &str, fallback: Value) -> Value {
        self.read(key, fallback)
    }

    fn ensure_loaded(&self, name: &str) -> Option<Arc<ConfigStore>> {
        let store = self
            .module
            .read()
            .upgrade()
            .and_then(|module| module.context().read().store())?;
        if !store.registers_module(name) {
            let _ = store.load_module(name);
        }
        Some(store)
    }

    pub fn read<T: DeserializeOwned>(&self, key: &str, fallback: T) -> T {
        let Some(module) = self.module.read().upgrade() else {
            return fallback;
        };
        let name = module.name();
        match self.ensure_loaded(name) {
            Some(store) => store.read(name, key, fallback),
            None => fallback,
        }
    }

    pub fn write<T: Serialize>(&self, key: &str, value: &T) -> bool {
        let Some(module) = self.module.read().upgrade() else {
            return false;
        };
        let name = module.name();
        let Some(store) = self.ensure_loaded(name) else {
            return false;
        };
        store.write(name, key, value).is_ok()
    }

    pub fn write_value(&self, key: &str, value: Value) -> bool {
        let Some(module) = self.module.read().upgrade() else {
            return false;
        };
        let name = module.name();
        let Some(store) = self.ensure_loaded(name) else {
            return false;
        };
        store.write_value(name, key, value).is_ok()
    }
}

pub fn event_flag(kind: EventKind) -> u32 {
    kind.flag()
}
