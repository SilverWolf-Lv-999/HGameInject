use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
};
use windows_sys::Win32::System::Threading::{
    CreateRemoteThread, LPTHREAD_START_ROUTINE, OpenProcess, WaitForSingleObject,
};

use crate::utility::host::{HostResult, error_code};

const WAIT_MILLISECONDS: u32 = 8000;
const ACCESS_INJECT: u32 = 0x0002 | 0x0008 | 0x0010 | 0x0020 | 0x0400 | 0x0800;
const KERNEL32: &str = "kernel32.dll";

pub fn inject_library(process_id: u32, library: &str) -> HostResult<()> {
    let process = unsafe { OpenProcess(ACCESS_INJECT, 0, process_id) };
    if process.is_null() || process == -1isize as _ {
        return Err(error_code("OpenProcess"));
    }
    let path: Vec<u16> = library
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let bytes = path.len() * std::mem::size_of::<u16>();
    let remote = unsafe {
        VirtualAllocEx(
            process,
            std::ptr::null(),
            bytes,
            MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    if remote.is_null() {
        unsafe {
            CloseHandle(process);
        }
        return Err(error_code("VirtualAllocEx"));
    }
    let written = unsafe {
        WriteProcessMemory(
            process,
            remote,
            path.as_ptr() as *const c_void,
            bytes,
            std::ptr::null_mut(),
        )
    };
    if written == 0 {
        unsafe {
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
        }
        return Err(error_code("WriteProcessMemory"));
    }
    let Some(start) = load_library_address() else {
        unsafe {
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
        }
        return Err("LoadLibraryW address unavailable".to_owned());
    };
    let mut thread_id = 0u32;
    let thread = unsafe {
        CreateRemoteThread(
            process,
            std::ptr::null(),
            0,
            std::mem::transmute::<usize, LPTHREAD_START_ROUTINE>(start),
            remote,
            0,
            &mut thread_id as *mut u32,
        )
    };
    if thread.is_null() || thread == -1isize as _ {
        unsafe {
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
        }
        return Err(error_code("CreateRemoteThread"));
    }
    let waited = unsafe { WaitForSingleObject(thread, WAIT_MILLISECONDS) };
    unsafe {
        CloseHandle(thread);
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        CloseHandle(process);
    }
    if waited != WAIT_OBJECT_0 {
        return Err("remote load timed out".to_owned());
    }
    Ok(())
}

fn load_library_address() -> Option<usize> {
    let name: Vec<u16> = wide(KERNEL32);
    let module = unsafe { GetModuleHandleW(name.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let procedure = b"LoadLibraryW\0";
    let address = unsafe { GetProcAddress(module, procedure.as_ptr()) };
    address.map(|value| value as usize)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
