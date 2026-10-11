pub mod engine;

#[cfg(feature = "retour")]
pub mod retour_backend;

pub use engine::{
    DetourClosure, DetourHook, HookBackend, HookContext, HookError, HookId, HookManager,
};
#[cfg(feature = "retour")]
pub use retour_backend::RetourHook;

#[cfg(feature = "retour")]
pub fn hook_backend() -> &'static dyn HookBackend {
    static BACKEND: RetourHook = RetourHook;
    &BACKEND
}

#[cfg(not(feature = "retour"))]
pub fn hook_backend() -> &'static dyn HookBackend {
    static BACKEND: UnavailableBackend = UnavailableBackend;
    &BACKEND
}

#[cfg(not(feature = "retour"))]
struct UnavailableBackend;

#[cfg(not(feature = "retour"))]
impl HookBackend for UnavailableBackend {
    fn install(
        &self,
        _target: usize,
        _detour: DetourClosure,
        _context: HookContext,
    ) -> Result<Box<dyn DetourHook>, HookError> {
        Err(HookError::Library(
            "no hook backend enabled, add the retour feature".to_owned(),
        ))
    }

    fn install_raw(
        &self,
        _target: usize,
        _detour: *const (),
    ) -> Result<Box<dyn DetourHook>, HookError> {
        Err(HookError::Library(
            "no hook backend enabled, add the retour feature".to_owned(),
        ))
    }
}
