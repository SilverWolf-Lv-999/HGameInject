use std::ffi::c_void;

use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::platform::{MemoryUtility, Win32Module};

pub const READ_CHUNK: usize = 0x40000;

#[derive(Debug, Clone)]
pub enum PatternError {
    Empty,
    InvalidToken(String),
    OddToken(String),
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternError::Empty => write!(formatter, "pattern is empty"),
            PatternError::InvalidToken(token) => write!(formatter, "invalid pattern token: {token}"),
            PatternError::OddToken(token) => write!(formatter, "truncated pattern token: {token}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    bytes: [u8; 64],
    mask: u64,
    length: usize,
}

impl Pattern {
    pub fn compile(signature: &str) -> Result<Pattern, PatternError> {
        let mut bytes = [0u8; 64];
        let mut mask = 0u64;
        let mut length = 0usize;
        for token in signature.split_whitespace() {
            if length >= 64 {
                return Err(PatternError::InvalidToken(token.to_owned()));
            }
            if token.len() != 2 {
                return Err(PatternError::OddToken(token.to_owned()));
            }
            if token == "??" {
                length += 1;
                continue;
            }
            match u8::from_str_radix(token, 16) {
                Ok(value) => {
                    bytes[length] = value;
                    mask |= 1u64 << length;
                    length += 1;
                }
                Err(_) => return Err(PatternError::InvalidToken(token.to_owned())),
            }
        }
        if length == 0 {
            return Err(PatternError::Empty);
        }
        Ok(Pattern {
            bytes,
            mask,
            length,
        })
    }

    pub const fn length(&self) -> usize {
        self.length
    }

    pub fn matches(&self, data: &[u8]) -> bool {
        if data.len() < self.length {
            return false;
        }
        let mut index = 0usize;
        while index < self.length {
            if self.mask & (1u64 << index) != 0 && data[index] != self.bytes[index] {
                return false;
            }
            index += 1;
        }
        true
    }
}

pub struct PatternScanner;

impl PatternScanner {
    pub fn find_in_module(module: &Win32Module, signature: &str) -> Option<usize> {
        let pattern = Pattern::compile(signature).ok()?;
        let end = module.end().ok()?;
        Self::find(module.base(), end, &pattern)
    }

    pub fn find_many_in_module(module: &Win32Module, signature: &str, limit: usize) -> Vec<usize> {
        let pattern = match Pattern::compile(signature) {
            Ok(pattern) => pattern,
            Err(_) => return Vec::new(),
        };
        let end = match module.end() {
            Ok(end) => end,
            Err(_) => return Vec::new(),
        };
        Self::find_many(module.base(), end, &pattern, limit)
    }

    pub fn find(start: usize, end: usize, pattern: &Pattern) -> Option<usize> {
        Self::find_many(start, end, pattern, 1).into_iter().next()
    }

    pub fn find_many(start: usize, end: usize, pattern: &Pattern, limit: usize) -> Vec<usize> {
        let mut results = Vec::new();
        if pattern.length == 0 || start >= end || limit == 0 {
            return results;
        }
        let regions = MemoryUtility::committed_regions(4096);
        for region in regions {
            let from = region.base.max(start);
            let to = region.base.saturating_add(region.size).min(end);
            if from >= to || !region.protection.readable() {
                continue;
            }
            if Self::scan_range(from, to, pattern, &mut results, limit) {
                return results;
            }
        }
        results
    }

    fn scan_range(
        from: usize,
        to: usize,
        pattern: &Pattern,
        results: &mut Vec<usize>,
        limit: usize,
    ) -> bool {
        let mut cursor = from;
        while cursor < to {
            let size = (to - cursor).min(READ_CHUNK);
            let mut buffer = vec![0u8; size];
            if read_memory(cursor, &mut buffer) {
                let mut index = 0usize;
                while index + pattern.length <= size {
                    if pattern.matches(&buffer[index..]) {
                        results.push(cursor + index);
                        if results.len() >= limit {
                            return true;
                        }
                    }
                    index += 1;
                }
            }
            let advance = if size > pattern.length - 1 {
                size - (pattern.length - 1)
            } else {
                size
            };
            cursor = cursor.saturating_add(advance.max(1));
        }
        false
    }
}

fn read_memory(address: usize, buffer: &mut [u8]) -> bool {
    let mut read = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const c_void,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len(),
            &mut read,
        )
    };
    ok != 0 && read == buffer.len()
}
