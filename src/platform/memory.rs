use std::ffi::c_void;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE, MEM_RELEASE, MEMORY_BASIC_INFORMATION,
    PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY,
    PAGE_READWRITE, PAGE_WRITECOPY, VirtualAlloc, VirtualFree, VirtualProtect,
    VirtualQuery,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::error::{InjectError, InjectResult, ListenAddress};

pub const PAGE_GUARD_FLAG: u32 = 0x100;
pub const PAGE_NOACCESS_FLAG: u32 = 0x01;

#[derive(Debug, Clone, Copy)]
pub struct Protection(pub u32);

impl Protection {
    pub const fn executable(self) -> bool {
        matches!(
            self.0 & 0xFF,
            PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
        )
    }

    pub const fn readable(self) -> bool {
        if self.0 & PAGE_NOACCESS_FLAG != 0 || self.0 & PAGE_GUARD_FLAG != 0 {
            return false;
        }
        !matches!(self.0 & 0xFF, 0)
    }

    pub const fn writable(self) -> bool {
        if self.0 & PAGE_NOACCESS_FLAG != 0 || self.0 & PAGE_GUARD_FLAG != 0 {
            return false;
        }
        matches!(
            self.0 & 0xFF,
            PAGE_READWRITE | PAGE_EXECUTE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_WRITECOPY
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: usize,
    pub size: usize,
    pub protection: Protection,
    pub state: u32,
}

pub struct MemoryUtility;

impl MemoryUtility {
    pub fn patch(address: usize, bytes: &[u8]) -> InjectResult<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let _guard = MemProtect::writable(address, patch_span(address, bytes.len()))?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
        }
        flush_instruction_cache(address, bytes.len());
        Ok(())
    }

    pub fn read(address: usize, out: &mut [u8]) -> InjectResult<()> {
        if out.is_empty() {
            return Ok(());
        }
        if !Self::region_containing(address)
            .map(|region| region.protection.readable())
            .unwrap_or(false)
        {
            return Err(InjectError::Memory(ListenAddress(address)));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(address as *const u8, out.as_mut_ptr(), out.len());
        }
        Ok(())
    }

    pub fn write(address: usize, bytes: &[u8]) -> InjectResult<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let _guard = MemProtect::writable(address, patch_span(address, bytes.len()))?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
        }
        Ok(())
    }

    pub fn read_value<T: Copy>(address: usize) -> InjectResult<T> {
        let mut value = std::mem::MaybeUninit::<T>::uninit();
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(value.as_mut_ptr() as *mut u8, std::mem::size_of::<T>())
        };
        Self::read(address, bytes)?;
        Ok(unsafe { value.assume_init() })
    }

    pub fn allocate_executable(size: usize) -> InjectResult<usize> {
        let rounded = (size + 0xFFF) & !0xFFF;
        let pointer = unsafe {
            VirtualAlloc(
                std::ptr::null(),
                rounded,
                MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        };
        if pointer.is_null() {
            return Err(InjectError::Memory(ListenAddress(0)));
        }
        Ok(pointer as usize)
    }

    pub fn release(address: usize) {
        if address == 0 {
            return;
        }
        unsafe {
            VirtualFree(address as *mut c_void, 0, MEM_RELEASE);
        }
    }

    pub fn region_containing(address: usize) -> Option<MemoryRegion> {
        let mut info = std::mem::MaybeUninit::<MEMORY_BASIC_INFORMATION>::zeroed();
        let written = unsafe {
            VirtualQuery(
                address as *const c_void,
                info.as_mut_ptr(),
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if written == 0 {
            return None;
        }
        let info = unsafe { info.assume_init() };
        Some(MemoryRegion {
            base: info.BaseAddress as usize,
            size: info.RegionSize,
            protection: Protection(info.Protect),
            state: info.State,
        })
    }

    pub fn committed_regions(limit: usize) -> Vec<MemoryRegion> {
        let mut regions = Vec::new();
        let mut address = 0x1_0000usize;
        while regions.len() < limit && address < 0x0000_7FFF_FFFF_FFFF {
            let mut info = std::mem::MaybeUninit::<MEMORY_BASIC_INFORMATION>::zeroed();
            let written = unsafe {
                VirtualQuery(
                    address as *const c_void,
                    info.as_mut_ptr(),
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if written == 0 {
                break;
            }
            let info = unsafe { info.assume_init() };
            let base = info.BaseAddress as usize;
            let size = info.RegionSize;
            if info.State == MEM_COMMIT
                && matches!(info.Type, MEM_IMAGE | MEM_MAPPED | MEM_PRIVATE)
                && size > 0
            {
                regions.push(MemoryRegion {
                    base,
                    size,
                    protection: Protection(info.Protect),
                    state: info.State,
                });
            }
            let next = base.checked_add(size);
            match next {
                Some(next) if next > address => address = next,
                _ => break,
            }
        }
        regions
    }
}

fn patch_span(address: usize, size: usize) -> usize {
    const PAGE_SIZE: usize = 0x1000;
    let offset = address & (PAGE_SIZE - 1);
    offset + size + PAGE_SIZE - 1
}

pub fn flush_instruction_cache(address: usize, size: usize) {
    unsafe {
        FlushInstructionCache(
            GetCurrentProcess(),
            address as *const c_void,
            size,
        );
    }
}

pub struct MemProtect {
    address: usize,
    size: usize,
    original: u32,
}

impl MemProtect {
    pub fn writable(address: usize, size: usize) -> InjectResult<MemProtect> {
        if let Some(region) = MemoryUtility::region_containing(address) {
            if region.protection.writable() {
                return Ok(MemProtect {
                    address: 0,
                    size: 0,
                    original: 0,
                });
            }
        }
        let mut original = 0u32;
        let changed = unsafe {
            VirtualProtect(
                address as *const c_void,
                size,
                PAGE_EXECUTE_READWRITE,
                &mut original,
            )
        };
        if changed == 0 {
            let code = unsafe { GetLastError() };
            return Err(InjectError::Protect(ListenAddress(address), code));
        }
        Ok(MemProtect {
            address,
            size,
            original,
        })
    }

    pub fn restore(&mut self) {
        if self.address == 0 {
            return;
        }
        unsafe {
            let mut ignored = 0u32;
            VirtualProtect(
                self.address as *const c_void,
                self.size,
                self.original,
                &mut ignored,
            );
        }
        self.address = 0;
    }
}

impl Drop for MemProtect {
    fn drop(&mut self) {
        self.restore();
    }
}
