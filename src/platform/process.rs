use std::ffi::c_void;

use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::error::{InjectError, InjectResult, ListenAddress};

pub const IMAGE_SIZEOF_NT_HEADERS64: usize = 4 + 20 + 240;

#[derive(Debug, Clone, Copy)]
pub struct Win32Module {
    base: usize,
}

impl Win32Module {
    pub fn by_name(name: &[u16]) -> InjectResult<Win32Module> {
        let handle = unsafe { GetModuleHandleW(name.as_ptr()) };
        if handle.is_null() {
            return Err(InjectError::InvalidState("module handle not found".to_owned()));
        }
        Ok(Win32Module {
            base: handle as usize,
        })
    }

    pub fn from_base(base: usize) -> Win32Module {
        Win32Module { base }
    }

    pub const fn base(&self) -> usize {
        self.base
    }

    pub fn size(&self) -> InjectResult<usize> {
        if self.base == 0 {
            return Err(InjectError::Memory(ListenAddress(0)));
        }
        let lfanew = unsafe { *((self.base + 0x3C) as *const u32) } as usize;
        let optional_magic = unsafe { *((self.base + lfanew + 0x18) as *const u16) };
        if optional_magic != 0x20B {
            return Err(InjectError::InvalidState("module is not pe32+".to_owned()));
        }
        let size_of_image = unsafe { *((self.base + lfanew + 0x18 + 56) as *const u32) };
        Ok(size_of_image as usize)
    }

    pub fn end(&self) -> InjectResult<usize> {
        Ok(self.base + self.size()?)
    }

    pub fn contains(&self, address: usize) -> bool {
        self.size()
            .map(|size| address >= self.base && address < self.base + size)
            .unwrap_or(false)
    }

    pub fn path(&self) -> InjectResult<String> {
        let mut buffer = vec![0u16; 512];
        let length = unsafe {
            GetModuleFileNameW(
                self.base as *mut c_void,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        if length == 0 {
            return Err(InjectError::InvalidState("module path unavailable".to_owned()));
        }
        Ok(String::from_utf16_lossy(&buffer[..length as usize]))
    }
}

pub fn current_process_id() -> u32 {
    unsafe { GetCurrentProcessId() }
}

pub fn main_module() -> InjectResult<Win32Module> {
    let handle = unsafe { GetModuleHandleW(std::ptr::null()) };
    if handle.is_null() {
        return Err(InjectError::InvalidState(
            "main module handle not found".to_owned(),
        ));
    }
    Ok(Win32Module {
        base: handle as usize,
    })
}
