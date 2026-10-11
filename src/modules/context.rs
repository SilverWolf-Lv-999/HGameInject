use std::sync::{Arc, Weak};

use crate::config::ConfigStore;
use crate::inject_log::InjectLogger;
use crate::modules::runtime::ModuleState;
use crate::modules::info::ModuleInfo;
use crate::modules::manager::ModuleManager;

pub trait ModuleHost: Send + Sync {
    fn module_manager(&self) -> Arc<ModuleManager>;

    fn config_store(&self) -> Arc<ConfigStore>;

    fn logger(&self) -> InjectLogger;
}

#[derive(Clone)]
pub struct ModuleContext {
    host: Weak<dyn ModuleHost>,
    info: ModuleInfo,
    command: String,
}

impl ModuleContext {
    pub fn new(host: Weak<dyn ModuleHost>, info: ModuleInfo, command: String) -> ModuleContext {
        ModuleContext {
            host,
            info,
            command,
        }
    }

    pub fn name(&self) -> &'static str {
        self.info.name
    }

    pub fn description(&self) -> &'static str {
        self.info.description
    }

    pub fn info(&self) -> ModuleInfo {
        self.info
    }

    pub fn info_mut(&mut self) -> &mut ModuleInfo {
        &mut self.info
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn logger(&self) -> Option<InjectLogger> {
        self.host.upgrade().map(|host| host.logger())
    }

    pub fn store(&self) -> Option<Arc<ConfigStore>> {
        self.host.upgrade().map(|host| host.config_store())
    }

    pub fn manager(&self) -> Option<Arc<ModuleManager>> {
        self.host.upgrade().map(|host| host.module_manager())
    }

    pub fn state(&self) -> Option<ModuleState> {
        self.manager()
            .and_then(|manager| manager.find(&self.command))
            .map(|module| module.state())
    }

    pub fn toggle(&self) {
        if let Some(manager) = self.manager() {
            manager.toggle(&self.command);
        }
    }

    pub fn open(&self) {
        if let Some(manager) = self.manager() {
            manager.open(&self.command);
        }
    }

    pub fn close(&self) {
        if let Some(manager) = self.manager() {
            manager.close(&self.command);
        }
    }
}
