use std::sync::{Arc, Weak};

use parking_lot::RwLock;
use serde_json::Value;

use crate::modules::bus::EventKind;
use crate::modules::context::{ModuleContext, ModuleHost};
use crate::modules::info::ModuleInfo;
use crate::modules::keybind::KeyBindings;
use crate::modules::runtime::{ModuleState, TargetModule};
use crate::protocol::ModuleSnapshot;
use crate::utility::language::LanguageUtility;

pub struct ModuleManager {
    modules: RwLock<Vec<ModuleEntry>>,
    states: RwLock<Vec<Arc<ModuleState>>>,
    bindings: KeyBindings,
}

pub struct ModuleEntry {
    module: Arc<TargetModule>,
}

impl Default for ModuleManager {
    fn default() -> ModuleManager {
        ModuleManager::new()
    }
}

impl ModuleManager {
    pub fn new() -> ModuleManager {
        ModuleManager {
            modules: RwLock::new(Vec::new()),
            states: RwLock::new(Vec::new()),
            bindings: KeyBindings::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> ModuleManager {
        ModuleManager {
            modules: RwLock::new(Vec::with_capacity(capacity)),
            states: RwLock::new(Vec::with_capacity(capacity)),
            bindings: KeyBindings::new(),
        }
    }

    pub fn bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    pub fn register(
        self: &Arc<Self>,
        host: &Weak<dyn ModuleHost>,
        info: ModuleInfo,
        module: Box<dyn crate::modules::runtime::Module>,
    ) -> Arc<TargetModule> {
        let command = info.name.to_owned();
        let target = Arc::new(TargetModule::new(module));
        let state = Arc::new(ModuleState::with_module(Arc::downgrade(&target)));
        *target.context().write() = ModuleContext::new(host.clone(), info, command.clone());
        self.modules.write().push(ModuleEntry {
            module: Arc::clone(&target),
        });
        target.bind_state(&state);
        self.states.write().push(Arc::clone(&state));
        target.module().attach(Arc::clone(&state));
        if info.keybind != 0 {
            self.bindings.bind(info.keybind, &command);
        }
        target
    }

    pub fn snapshots(&self) -> Vec<ModuleSnapshot> {
        self.modules
            .read()
            .iter()
            .map(|entry| entry.module.snapshot())
            .collect()
    }

    pub fn set_config(&self, name: &str, key: &str, value: Value) -> bool {
        let Some(module) = self.find(name) else {
            return false;
        };
        if !module
            .schema()
            .iter()
            .any(|entry| entry.key == key && entry.matches(&value))
        {
            return false;
        }
        if !module.state().write_value(key, value) {
            return false;
        }
        {
            let context = module.context().read();
            module.on_config(&context, key);
        }
        if module.event_flags() & EventKind::Config.flag() != 0 {
            let mut context = module.context().write();
            module.on_event(&mut context, EventKind::Config);
        }
        true
    }

    pub fn set_binding(&self, name: &str, key_code: u32) -> bool {
        let Some(module) = self.find(name) else {
            return false;
        };
        self.bindings.bind(key_code, name);
        module.context().write().info_mut().keybind = key_code;
        true
    }

    pub fn count(&self) -> usize {
        self.modules.read().len()
    }

    pub fn states(&self) -> Vec<Arc<ModuleState>> {
        self.states.read().clone()
    }

    pub fn names(&self) -> Vec<String> {
        self.modules
            .read()
            .iter()
            .map(|entry| entry.module.name().to_owned())
            .collect()
    }

    pub fn find(&self, name: &str) -> Option<Arc<TargetModule>> {
        self.modules
            .read()
            .iter()
            .map(|entry| Arc::clone(&entry.module))
            .find(|module| module.name().eq_ignore_ascii_case(name))
    }

    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn find_by_display(&self, display: &str) -> Option<Arc<TargetModule>> {
        self.modules
            .read()
            .iter()
            .map(|entry| Arc::clone(&entry.module))
            .find(|module| {
                let context = module.context().read();
                LanguageUtility::translate(context.name()).eq_ignore_ascii_case(display)
            })
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.find(name)
            .map(|module| module.runtime().enabled())
            .unwrap_or(false)
    }

    pub fn enabled(&self) -> Vec<String> {
        self.modules
            .read()
            .iter()
            .map(|entry| Arc::clone(&entry.module))
            .filter(|module| module.runtime().enabled())
            .map(|module| module.name().to_owned())
            .collect()
    }

    pub fn toggle(&self, name: &str) -> bool {
        match self.find(name) {
            Some(module) => {
                if module.runtime().enabled() {
                    self.close(name);
                } else {
                    self.open(name);
                }
                true
            }
            None => false,
        }
    }

    pub fn open(&self, name: &str) -> bool {
        let Some(module) = self.find(name) else {
            return false;
        };
        if module.runtime().enabled() {
            return true;
        }
        module.runtime().set(true);
        let flags = module.event_flags();
        {
            let context = module.context().read();
            module.on_enable(&context);
            if let Some(logger) = context.logger() {
                logger.info(&format!("enabled {}", module.name()));
            }
        }
        if flags & EventKind::Open.flag() != 0 {
            let mut context = module.context().write();
            module.on_event(&mut context, EventKind::Open);
        }
        true
    }

    pub fn close(&self, name: &str) -> bool {
        let Some(module) = self.find(name) else {
            return false;
        };
        if !module.runtime().enabled() {
            return true;
        }
        module.runtime().set(false);
        let flags = module.event_flags();
        {
            let context = module.context().read();
            module.on_disable(&context);
            if let Some(logger) = context.logger() {
                logger.info(&format!("disabled {}", module.name()));
            }
        }
        if flags & EventKind::Close.flag() != 0 {
            let mut context = module.context().write();
            module.on_event(&mut context, EventKind::Close);
        }
        true
    }

    pub fn update(&self, kind: EventKind) {
        let modules: Vec<Arc<TargetModule>> = self
            .modules
            .read()
            .iter()
            .map(|entry| Arc::clone(&entry.module))
            .collect();
        for module in modules {
            if !module.runtime().enabled() {
                continue;
            }
            if module.event_flags() & kind.flag() == 0 {
                continue;
            }
            let mut context = module.context().write();
            module.on_event(&mut context, kind);
        }
    }

    pub fn handle_key(&self, key_code: u32) -> bool {
        let Some(command) = self.bindings.module_for(key_code) else {
            return false;
        };
        match self.find(&command) {
            Some(module) => {
                let context = module.context().read();
                module.on_key(&context, key_code);
                true
            }
            None => false,
        }
    }
}
