use std::collections::HashMap;
use std::ffi::c_void;
use std::fmt::{Display, Formatter, Result as FmtResult};

use parking_lot::RwLock;

use crate::hooks::hook_backend;

pub type HookContext = Box<dyn Send + Sync>;

pub type DetourClosure = Box<dyn Fn(&HookContext, *mut c_void) -> usize + Send + Sync + 'static>;

#[derive(Debug)]
pub enum HookError {
    Library(String),
    NotFound(String),
}

impl Display for HookError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            HookError::Library(message) => write!(formatter, "hook library error: {message}"),
            HookError::NotFound(message) => write!(formatter, "hook not found: {message}"),
        }
    }
}

impl std::error::Error for HookError {}

pub trait DetourHook: Send + Sync {
    fn uninstall(&self);

    fn active(&self) -> bool;

    fn target(&self) -> usize;

    fn context(&self) -> *mut c_void;

    fn trampoline(&self) -> *mut c_void;
}

pub trait HookBackend: Send + Sync {
    fn install(
        &self,
        target: usize,
        detour: DetourClosure,
        context: HookContext,
    ) -> Result<Box<dyn DetourHook>, HookError>;

    fn install_raw(
        &self,
        target: usize,
        detour: *const (),
    ) -> Result<Box<dyn DetourHook>, HookError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HookId(pub u64);

pub struct HookManager {
    nextId: u64,
    hooks: RwLock<HashMap<u64, Box<dyn DetourHook>>>,
}

impl Default for HookManager {
    fn default() -> HookManager {
        HookManager::new()
    }
}

impl HookManager {
    pub fn new() -> HookManager {
        HookManager {
            nextId: 0,
            hooks: RwLock::new(HashMap::new()),
        }
    }

    pub fn install(
        &mut self,
        target: usize,
        detour: DetourClosure,
        context: HookContext,
    ) -> Result<HookId, HookError> {
        let hook = hook_backend().install(target, detour, context)?;
        Ok(self.track(hook))
    }

    pub fn install_raw(&mut self, target: usize, detour: *const ()) -> Result<HookId, HookError> {
        let hook = hook_backend().install_raw(target, detour)?;
        Ok(self.track(hook))
    }

    fn track(&mut self, hook: Box<dyn DetourHook>) -> HookId {
        let id = self.nextId;
        self.nextId = self.nextId.wrapping_add(1);
        self.hooks.write().insert(id, hook);
        HookId(id)
    }

    pub fn uninstall(&self, id: HookId) -> bool {
        let hook = self.hooks.write().remove(&id.0);
        match hook {
            Some(hook) => {
                hook.uninstall();
                true
            }
            None => false,
        }
    }

    pub fn contains(&self, id: HookId) -> bool {
        self.hooks.read().contains_key(&id.0)
    }

    pub fn context(&self, id: HookId) -> *mut c_void {
        self.hooks
            .read()
            .get(&id.0)
            .map(|hook| hook.context())
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn trampoline(&self, id: HookId) -> *mut c_void {
        self.hooks
            .read()
            .get(&id.0)
            .map(|hook| hook.trampoline())
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn ids(&self) -> Vec<HookId> {
        self.hooks.read().keys().map(|id| HookId(*id)).collect()
    }

    pub fn uninstall_all(&mut self) {
        for (_, hook) in self.hooks.write().drain() {
            hook.uninstall();
        }
    }
}

impl Drop for HookManager {
    fn drop(&mut self) {
        for (_, hook) in self.hooks.write().drain() {
            hook.uninstall();
        }
    }
}
