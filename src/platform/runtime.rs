use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

pub const MONO_MODULES: &[&str] = &[
    "mono-2.0-bdwgc.dll",
    "mono-2.0-sgen.dll",
    "mono-2.0.dll",
    "mono.dll",
];
const IL2CPP_MODULE: &str = "GameAssembly.dll";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRuntime {
    Il2Cpp,
    Mono,
    Unknown,
}

impl HostRuntime {
    pub const fn name(self) -> &'static str {
        match self {
            HostRuntime::Il2Cpp => "IL2CPP",
            HostRuntime::Mono => "Mono",
            HostRuntime::Unknown => "unknown",
        }
    }
}

pub fn host_runtime() -> HostRuntime {
    if module_handle(IL2CPP_MODULE).is_some() {
        return HostRuntime::Il2Cpp;
    }
    if MONO_MODULES.iter().any(|name| module_handle(name).is_some()) {
        return HostRuntime::Mono;
    }
    HostRuntime::Unknown
}

pub fn module_handle(name: &str) -> Option<HMODULE> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { GetModuleHandleW(wide.as_ptr()) };
    (!handle.is_null()).then_some(handle)
}
