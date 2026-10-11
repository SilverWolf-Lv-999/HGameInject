use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;

use parking_lot::RwLock;
use retour::RawDetour;

use crate::hooks::engine::{DetourClosure, DetourHook, HookBackend, HookContext, HookError};

struct ContextEntry {
    detour: DetourClosure,
    context: HookContext,
}

fn contexts() -> &'static RwLock<HashMap<usize, usize>> {
    static CONTEXTS: OnceLock<RwLock<HashMap<usize, usize>>> = OnceLock::new();
    CONTEXTS.get_or_init(|| RwLock::new(HashMap::new()))
}

pub struct RetourHook;

impl HookBackend for RetourHook {
    fn install(
        &self,
        target: usize,
        detour: DetourClosure,
        context: HookContext,
    ) -> Result<Box<dyn DetourHook>, HookError> {
        let entry = Box::leak(Box::new(ContextEntry { detour, context })) as *mut ContextEntry;
        let raw = unsafe {
            RawDetour::new(target as *const (), retour_thunk as *const ())
                .map_err(|error| HookError::Library(error.to_string()))?
        };
        let trampoline = raw.trampoline() as *const () as usize;
        contexts().write().insert(trampoline, entry as usize);
        unsafe {
            raw.enable()
                .map_err(|error| HookError::Library(error.to_string()))?;
        }
        Ok(Box::new(RetourHookHandle {
            raw,
            entry,
            trampoline,
            target,
            active: true,
        }))
    }

    fn install_raw(
        &self,
        target: usize,
        detour: *const (),
    ) -> Result<Box<dyn DetourHook>, HookError> {
        let raw = unsafe {
            RawDetour::new(target as *const (), detour)
                .map_err(|error| HookError::Library(error.to_string()))?
        };
        let trampoline = raw.trampoline() as *const () as usize;
        unsafe {
            raw.enable()
                .map_err(|error| HookError::Library(error.to_string()))?;
        }
        Ok(Box::new(RetourHookHandle {
            raw,
            entry: std::ptr::null_mut(),
            trampoline,
            target,
            active: true,
        }))
    }
}

unsafe extern "C" fn retour_thunk(original: *mut c_void) -> usize {
    let trampoline = original as usize;
    let entry = contexts()
        .read()
        .get(&trampoline)
        .copied()
        .unwrap_or(0);
    if entry == 0 {
        return 0;
    }
    let entry = unsafe { &*(entry as *const ContextEntry) };
    (entry.detour)(&entry.context, original)
}

pub struct RetourHookHandle {
    raw: RawDetour,
    entry: *mut ContextEntry,
    trampoline: usize,
    target: usize,
    active: bool,
}

unsafe impl Send for RetourHookHandle {}

unsafe impl Sync for RetourHookHandle {}

impl DetourHook for RetourHookHandle {
    fn uninstall(&self) {
        if !self.active {
            return;
        }
        unsafe {
            let _ = self.raw.disable();
        }
    }

    fn active(&self) -> bool {
        self.active
    }

    fn target(&self) -> usize {
        self.target
    }

    fn context(&self) -> *mut c_void {
        self.entry as *mut c_void
    }

    fn trampoline(&self) -> *mut c_void {
        self.trampoline as *mut c_void
    }
}

impl Drop for RetourHookHandle {
    fn drop(&mut self) {
        self.uninstall();
        if !self.entry.is_null() {
            contexts().write().remove(&self.trampoline);
            unsafe {
                drop(Box::from_raw(self.entry));
            }
        }
    }
}
