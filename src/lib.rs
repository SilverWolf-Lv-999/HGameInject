#![allow(non_snake_case)]
#![allow(function_casts_as_integer)]

pub mod bootstrap;
pub mod bridge;
pub mod client;
pub mod config;
pub mod error;
pub mod hooks;
pub mod inject_log;
pub mod modules;
pub mod platform;
pub mod protocol;
pub mod utility;
pub mod web;

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::System::LibraryLoader::DisableThreadLibraryCalls;

pub use bootstrap::Host;
pub use client::{ClientManager, IMinecraft, instance};
pub use config::{ClientConfig, CommonConfig, ConfigStore};
pub use error::InjectError;
pub use hooks::{DetourClosure, DetourHook, HookBackend, HookContext, HookError, HookId, HookManager};
pub use inject_log::InjectLogger;
pub use modules::{
    EventKind, KeyBindings, Module, ModuleCategory, ModuleContext, ModuleHost, ModuleInfo,
    ModuleManager, ModuleRuntime, ModuleState, TargetModule,
};
pub use platform::Win32Module;

static MODULE_INSTANCE: OnceLock<usize> = OnceLock::new();
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn module_instance() -> HINSTANCE {
    MODULE_INSTANCE
        .get()
        .map(|value| *value as HINSTANCE)
        .unwrap_or(std::ptr::null_mut())
}

pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

pub fn is_shutting_down() -> bool {
    SHUTDOWN.load(Ordering::Acquire)
}

const DLL_PROCESS_DETACH: u32 = 0;
const DLL_PROCESS_ATTACH: u32 = 1;

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    instance: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            let _ = MODULE_INSTANCE.set(instance as usize);
            unsafe {
                DisableThreadLibraryCalls(instance);
            }
            INITIALIZED.store(false, Ordering::Release);
            SHUTDOWN.store(false, Ordering::Release);
            let spawned = thread::Builder::new()
                .name("HGameInject-Bootstrap".to_owned())
                .spawn(|| match bootstrap::initialize() {
                    Ok(()) => {
                        if let Some(host) = bootstrap::host() {
                            bootstrap::run_loop(&host);
                        }
                    }
                    Err(error) => {
                        inject_log::write_line(
                            inject_log::Level::Error,
                            &format!("bootstrap failed: {error}"),
                        );
                    }
                });
            if spawned.is_err() {
                inject_log::write_line(inject_log::Level::Error, "bootstrap thread spawn failed");
            }
        }
        DLL_PROCESS_DETACH => {
            SHUTDOWN.store(true, Ordering::Release);
        }
        _ => {}
    }
    1
}
