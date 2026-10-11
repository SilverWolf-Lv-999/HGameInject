use std::sync::{Arc, OnceLock};

use crate::error::{InjectError, InjectResult};
use crate::platform::process::{Win32Module, current_process_id, main_module};
use crate::utility::pattern::PatternScanner;

pub struct ClientManager {
    process_id: u32,
    module: Win32Module,
}

static INSTANCE: OnceLock<Arc<ClientManager>> = OnceLock::new();

impl ClientManager {
    pub fn create() -> InjectResult<Arc<ClientManager>> {
        let module = main_module()?;
        Ok(Arc::new(ClientManager {
            process_id: current_process_id(),
            module,
        }))
    }

    pub fn register(instance: Arc<ClientManager>) {
        let _ = INSTANCE.set(instance);
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn module(&self) -> Win32Module {
        self.module
    }

    pub fn module_base(&self) -> usize {
        self.module.base()
    }

    pub fn module_base_of(&self, module_name: &str) -> InjectResult<usize> {
        Ok(self.win32_module(module_name)?.base())
    }

    pub fn win32_module(&self, module_name: &str) -> InjectResult<Win32Module> {
        let encoded: Vec<u16> = module_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        Win32Module::by_name(&encoded)
    }

    pub fn read_memory(&self, address: usize, out: &mut [u8]) -> InjectResult<()> {
        crate::platform::MemoryUtility::read(address, out)
    }

    pub fn write_memory(&self, address: usize, bytes: &[u8]) -> InjectResult<()> {
        crate::platform::MemoryUtility::patch(address, bytes)
    }

    pub fn read_value<T: Copy>(&self, address: usize) -> InjectResult<T> {
        crate::platform::MemoryUtility::read_value(address)
    }

    pub fn find_pattern(&self, signature: &str) -> Option<usize> {
        PatternScanner::find_in_module(&self.module, signature)
    }

    pub fn find_patterns(&self, signature: &str, limit: usize) -> Vec<usize> {
        PatternScanner::find_many_in_module(&self.module, signature, limit)
    }

    pub fn find_pattern_in(&self, module_name: &str, signature: &str) -> InjectResult<usize> {
        let module = self.win32_module(module_name)?;
        PatternScanner::find_in_module(&module, signature)
            .ok_or_else(|| InjectError::PatternNotFound(signature.to_owned()))
    }
}

pub fn instance() -> Arc<ClientManager> {
    INSTANCE
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(ClientManager {
            process_id: current_process_id(),
            module: Win32Module::from_base(0),
        }))
}

pub fn instance_optional() -> Option<Arc<ClientManager>> {
    INSTANCE.get().cloned()
}

pub trait IMinecraft: Send + Sync {
    fn process_id(&self) -> u32;

    fn module_base(&self) -> usize;
}

impl IMinecraft for ClientManager {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn module_base(&self) -> usize {
        self.module.base()
    }
}
