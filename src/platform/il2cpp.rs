use std::ffi::{CStr, CString, c_char};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

use crate::platform::runtime::module_handle;

type Il2CppDomainGet = unsafe extern "system" fn() -> usize;
type Il2CppThreadAttach = unsafe extern "system" fn(usize) -> usize;
type Il2CppDomainGetAssemblies = unsafe extern "system" fn(usize, *mut usize) -> *const usize;
type Il2CppAssemblyGetImage = unsafe extern "system" fn(usize) -> usize;
type Il2CppImageGetName = unsafe extern "system" fn(usize) -> *const c_char;
type Il2CppClassFromName = unsafe extern "system" fn(usize, *const c_char, *const c_char) -> usize;
type Il2CppClassGetMethodFromName =
    unsafe extern "system" fn(usize, *const c_char, i32) -> usize;
type Il2CppClassGetFieldFromName = unsafe extern "system" fn(usize, *const c_char) -> usize;
type Il2CppFieldGetOffset = unsafe extern "system" fn(usize) -> usize;

pub const IL2CPP_MODULE: &str = "GameAssembly.dll";

#[derive(Clone, Copy)]
pub struct Il2CppField {
    pub handle: usize,
    pub offset: usize,
}

#[derive(Clone, Copy)]
pub struct Il2CppMethod {
    pub handle: usize,
    pub address: usize,
}

pub struct Il2CppHandler {
    domain: usize,
    threadAttach: Il2CppThreadAttach,
    domainGetAssemblies: Il2CppDomainGetAssemblies,
    assemblyGetImage: Il2CppAssemblyGetImage,
    imageGetName: Il2CppImageGetName,
    classFromName: Il2CppClassFromName,
    classGetMethodFromName: Il2CppClassGetMethodFromName,
    classGetFieldFromName: Il2CppClassGetFieldFromName,
    fieldGetOffset: Il2CppFieldGetOffset,
}

impl Il2CppHandler {
    pub fn create() -> Result<Il2CppHandler, String> {
        let module = module_handle(IL2CPP_MODULE)
            .ok_or_else(|| "il2cpp runtime not loaded".to_owned())?;
        let getDomain: Il2CppDomainGet =
            unsafe { std::mem::transmute(resolve(module, "il2cpp_domain_get")?) };
        let domain = unsafe { getDomain() };
        if domain == 0 {
            return Err("il2cpp domain unavailable".to_owned());
        }
        Ok(Il2CppHandler {
            domain,
            threadAttach: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_thread_attach")?)
            },
            domainGetAssemblies: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_domain_get_assemblies")?)
            },
            assemblyGetImage: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_assembly_get_image")?)
            },
            imageGetName: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_image_get_name")?)
            },
            classFromName: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_class_from_name")?)
            },
            classGetMethodFromName: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_class_get_method_from_name")?)
            },
            classGetFieldFromName: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_class_get_field_from_name")?)
            },
            fieldGetOffset: unsafe {
                std::mem::transmute(resolve(module, "il2cpp_field_get_offset")?)
            },
        })
    }

    pub fn attach(&self) {
        unsafe {
            (self.threadAttach)(self.domain);
        }
    }

    pub fn image(&self, name: &str) -> Option<usize> {
        let mut count = 0usize;
        let assemblies = unsafe { (self.domainGetAssemblies)(self.domain, &mut count as *mut usize) };
        if assemblies.is_null() {
            return None;
        }
        let wanted = name.trim_end_matches(".dll");
        for index in 0..count {
            let assembly = unsafe { *assemblies.add(index) };
            if assembly == 0 {
                continue;
            }
            let image = unsafe { (self.assemblyGetImage)(assembly) };
            if image == 0 {
                continue;
            }
            let pointer = unsafe { (self.imageGetName)(image) };
            if pointer.is_null() {
                continue;
            }
            let text = unsafe { CStr::from_ptr(pointer) }.to_string_lossy();
            if text.trim_end_matches(".dll") == wanted {
                return Some(image);
            }
        }
        None
    }

    pub fn class(&self, image: usize, namespace: &str, name: &str) -> Option<usize> {
        let namespace = CString::new(namespace).ok()?;
        let name = CString::new(name).ok()?;
        let class = unsafe { (self.classFromName)(image, namespace.as_ptr(), name.as_ptr()) };
        (class != 0).then_some(class)
    }

    pub fn field(&self, class: usize, name: &str) -> Option<Il2CppField> {
        let name = CString::new(name).ok()?;
        let handle = unsafe { (self.classGetFieldFromName)(class, name.as_ptr()) };
        if handle == 0 {
            return None;
        }
        let offset = unsafe { (self.fieldGetOffset)(handle) };
        Some(Il2CppField { handle, offset })
    }

    pub fn method(&self, class: usize, name: &str, parameters: i32) -> Option<Il2CppMethod> {
        let name = CString::new(name).ok()?;
        let handle = unsafe { (self.classGetMethodFromName)(class, name.as_ptr(), parameters) };
        if handle == 0 {
            return None;
        }
        let address = unsafe { *(handle as *const usize) };
        (address != 0).then_some(Il2CppMethod { handle, address })
    }
}

fn resolve(module: HMODULE, name: &str) -> Result<usize, String> {
    let symbol = CString::new(name).map_err(|_| format!("{name} invalid"))?;
    let address = unsafe { GetProcAddress(module, symbol.as_ptr() as *const u8) };
    match address {
        Some(function) => Ok(function as usize),
        None => Err(format!("{name} missing")),
    }
}
