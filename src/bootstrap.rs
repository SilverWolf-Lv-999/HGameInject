use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::client::ClientManager;
use crate::config::client::config_directory;
use crate::config::{ClientConfig, CommonConfig, ConfigStore};
use crate::error::InjectResult;
use crate::hooks::HookManager;
use crate::inject_log::{InjectLogger, Level, write_line};
use crate::bridge::BridgeModule;
use crate::modules::bus::EventKind;
use crate::modules::context::ModuleHost;
use crate::modules::builtin::harnesses;
use crate::modules::harness::register_harnesses;
use crate::modules::manager::ModuleManager;
use crate::modules::runtime::TargetModule;
use crate::utility::language::LanguageUtility;

static HOST: OnceLock<Arc<Host>> = OnceLock::new();

pub struct Host {
    client: Arc<ClientManager>,
    config: RwLock<ClientConfig>,
    common: RwLock<CommonConfig>,
    store: Arc<ConfigStore>,
    logger: InjectLogger,
    manager: Arc<ModuleManager>,
    hooks: RwLock<HookManager>,
    directory: PathBuf,
    bridge: OnceLock<Arc<BridgeModule>>,
}

impl Host {
    pub fn create(directory: PathBuf) -> InjectResult<Arc<Host>> {
        let client = ClientManager::create()?;
        ClientManager::register(Arc::clone(&client));
        let common = CommonConfig::load(&directory);
        let store = Arc::new(ConfigStore::new(directory.join("modules")));
        for (name, patch) in &common.modulePatch {
            store.seed(name, patch.clone());
        }
        let config = ClientConfig::load(&directory);
        let logger = InjectLogger::from_client_config(&config);
        logger.clone().register();
        let host = Arc::new(Host {
            client,
            config: RwLock::new(config),
            common: RwLock::new(common),
            store,
            logger,
            manager: Arc::new(ModuleManager::with_capacity(16)),
            hooks: RwLock::new(HookManager::new()),
            directory,
            bridge: OnceLock::new(),
        });
        let weak: Weak<dyn ModuleHost> = Arc::downgrade(&host) as Weak<dyn ModuleHost>;
        register_harnesses(&host.manager, &weak, &harnesses());
        Ok(host)
    }

    pub fn restore(&self) -> InjectResult<()> {
        let common = self.common.read().clone();
        for name in &common.enabledModule {
            self.manager.open(name);
        }
        for (key_code, name) in &common.keyBindings {
            self.manager.bindings().bind(*key_code, name);
        }
        Ok(())
    }

    pub fn client(&self) -> Arc<ClientManager> {
        Arc::clone(&self.client)
    }

    pub fn manager(&self) -> Arc<ModuleManager> {
        Arc::clone(&self.manager)
    }

    pub fn store(&self) -> Arc<ConfigStore> {
        Arc::clone(&self.store)
    }

    pub fn hooks(&self) -> &RwLock<HookManager> {
        &self.hooks
    }

    pub fn logger(&self) -> InjectLogger {
        self.logger.clone()
    }

    pub fn bridge(&self, module: Arc<BridgeModule>) {
        let _ = self.bridge.set(module);
    }

    pub fn modules(&self) -> Vec<Arc<TargetModule>> {
        self.manager
            .states()
            .into_iter()
            .filter_map(|state| state.upgrade())
            .collect()
    }

    pub fn sync_module_state(&self) {
        let mut common = self.common.write();
        for module in self.modules() {
            let name = module.name();
            common.set_enabled(name, module.runtime().enabled());
            let context = module.context().read();
            if context.info().keybind != 0 {
                common.set_key(context.info().keybind, name);
            }
            if let Some(store) = context.store() {
                let patch = store.patch(name);
                if !patch.is_empty() {
                    common.apply_patch(name, patch);
                }
            }
        }
    }

    pub fn persist(&self) -> InjectResult<()> {
        self.sync_module_state();
        for module in self.modules() {
            let name = module.name();
            if self.store.patch(name).is_empty() {
                continue;
            }
            self.store.commit(name)?;
        }
        if self.common.read().is_dirty() {
            self.common.read().save(&self.directory)?;
        }
        if self.config.read().is_dirty() {
            self.config.read().save(&self.directory)?;
        }
        Ok(())
    }

    pub fn tick(&self) {
        self.manager.update(EventKind::Update);
    }

    pub fn key(&self, key_code: u32) {
        self.manager.handle_key(key_code);
    }

    pub fn shut_down(&self) {
        if let Some(bridge) = self.bridge.get() {
            bridge.stop();
        }
        self.hooks.write().uninstall_all();
        if let Err(error) = self.persist() {
            write_line(Level::Error, &format!("persist failed: {error}"));
        }
    }
}

impl ModuleHost for Host {
    fn module_manager(&self) -> Arc<ModuleManager> {
        self.manager()
    }

    fn config_store(&self) -> Arc<ConfigStore> {
        self.store()
    }

    fn logger(&self) -> InjectLogger {
        self.logger.clone()
    }
}

pub fn initialize() -> InjectResult<()> {
    let directory = writable_config_directory();
    LanguageUtility::initialize(&directory.join("Language.toml"));
    let host = match Host::create(directory) {
        Ok(host) => host,
        Err(error) => {
            write_line(Level::Error, &format!("host creation failed: {error}"));
            return Err(error);
        }
    };
    let _ = HOST.set(Arc::clone(&host));
    host.bridge(BridgeModule::start(&host));
    if let Err(error) = host.restore() {
        write_line(Level::Warn, &format!("restore skipped: {error}"));
    }
    write_line(
        Level::Info,
        &format!("HGameInject attached, {} modules", host.manager().count()),
    );
    Ok(())
}

fn writable_config_directory() -> PathBuf {
    let preferred = config_directory();
    if std::fs::create_dir_all(&preferred).is_ok() {
        return preferred;
    }
    let fallback = crate::inject_log::log_directory().join("config");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

pub fn host() -> Option<Arc<Host>> {
    HOST.get().cloned()
}

pub fn run_loop(host: &Arc<Host>) {
    let mut next_tick = Instant::now();
    let mut next_save = Instant::now() + Duration::from_secs(10);
    while !crate::is_shutting_down() {
        let now = Instant::now();
        if now >= next_tick {
            next_tick += Duration::from_millis(16);
            host.tick();
        }
        if now >= next_save {
            next_save += Duration::from_secs(10);
            if let Err(error) = host.persist() {
                write_line(Level::Error, &format!("save failed: {error}"));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    host.shut_down();
    write_line(Level::Info, "HGameInject detached");
}
