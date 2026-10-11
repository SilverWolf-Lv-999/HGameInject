use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, PeekNamedPipe, WaitNamedPipeW,
};

use crate::utility::host::{HostResult, error_code};

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const PIPE_BUFFER: u32 = 64 * 1024;
const DEFAULT_TIMEOUT_MILLISECONDS: u32 = 5000;
const LINE_LIMIT: usize = 1024 * 1024;

#[derive(Debug)]
pub struct PipeHandle(pub HANDLE);

unsafe impl Send for PipeHandle {}

unsafe impl Sync for PipeHandle {}

impl PipeHandle {
    pub fn is_valid(&self) -> bool {
        !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE
    }

    pub fn write_all(&self, bytes: &[u8]) -> HostResult<()> {
        let mut offset = 0usize;
        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            let mut written = 0u32;
            let ok = unsafe {
                WriteFile(
                    self.0,
                    remaining.as_ptr(),
                    remaining.len().min(u32::MAX as usize) as u32,
                    &mut written as *mut u32,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || written == 0 {
                return Err(error_code("WriteFile(pipe)"));
            }
            offset += written as usize;
        }
        Ok(())
    }

    fn read_once(&self, timeout_milliseconds: u32) -> HostResult<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_milliseconds as u64);
        while Instant::now() < deadline {
            let mut available = 0u32;
            let ok = unsafe {
                PeekNamedPipe(
                    self.0,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available as *mut u32,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_BROKEN_PIPE as u32 {
                    return Err("pipe closed".to_owned());
                }
                return Err(error_code("PeekNamedPipe"));
            }
            if available > 0 {
                let mut buffer = vec![0u8; available as usize];
                let mut taken = 0u32;
                let ok = unsafe {
                    ReadFile(
                        self.0,
                        buffer.as_mut_ptr(),
                        available,
                        &mut taken as *mut u32,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 || taken == 0 {
                    return Err(error_code("ReadFile(pipe)"));
                }
                buffer.truncate(taken as usize);
                return Ok(buffer);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err("pipe read timed out".to_owned())
    }

    pub fn read_line(&self, timeout_milliseconds: u32) -> HostResult<String> {
        let deadline = Instant::now() + Duration::from_millis(timeout_milliseconds as u64);
        let mut collected = Vec::new();
        while Instant::now() < deadline && collected.len() < LINE_LIMIT {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let chunk = self.read_once(remaining.as_millis().max(1) as u32)?;
            collected.extend_from_slice(&chunk);
            if collected.contains(&b'\n') {
                break;
            }
        }
        if collected.is_empty() {
            return Err("pipe returned no data".to_owned());
        }
        Ok(String::from_utf8_lossy(&collected).trim().to_owned())
    }

    pub fn write_line(&self, line: &str) -> HostResult<()> {
        let mut message = Vec::with_capacity(line.len() + 1);
        message.extend_from_slice(line.as_bytes());
        message.push(b'\n');
        self.write_all(&message)
    }

    pub fn drain(&self) {
        let _ = self.read_once(50);
    }
}

impl Drop for PipeHandle {
    fn drop(&mut self) {
        if self.is_valid() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub fn create_server(pipe_name: &str) -> HostResult<PipeHandle> {
    let name = wide(pipe_name);
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            PIPE_BUFFER,
            PIPE_BUFFER,
            DEFAULT_TIMEOUT_MILLISECONDS,
            std::ptr::null(),
        )
    };
    let handle = PipeHandle(handle);
    if !handle.is_valid() {
        return Err(error_code("CreateNamedPipeW"));
    }
    Ok(handle)
}

pub fn wait_for_client(handle: &PipeHandle) -> HostResult<bool> {
    let connected = unsafe { ConnectNamedPipe(handle.0, std::ptr::null_mut()) };
    if connected != 0 {
        return Ok(true);
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_PIPE_CONNECTED as u32 {
        return Ok(true);
    }
    Ok(false)
}

pub fn open_client(pipe_name: &str, timeout_milliseconds: u32) -> HostResult<PipeHandle> {
    let name = wide(pipe_name);
    let deadline = Instant::now() + Duration::from_millis(timeout_milliseconds as u64);
    loop {
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_NONE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        let handle = PipeHandle(handle);
        if handle.is_valid() {
            return Ok(handle);
        }
        if Instant::now() >= deadline {
            return Err(error_code("CreateFileW(pipe)"));
        }
        unsafe {
            WaitNamedPipeW(name.as_ptr(), 200);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
