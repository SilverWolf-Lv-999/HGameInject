use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, MAX_PATH};
use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::utility::host::{HostResult, error_code};

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const PROCESS_ENTRY_LIMIT: usize = 4096;
const WINDOW_ENTRY_LIMIT: usize = 8192;
const SCAN_BUDGET: Duration = Duration::from_millis(2500);
const WINDOW_TITLE_MINIMUM: i32 = 2;

#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub title: String,
}

impl ProcessEntry {
    pub fn matches_directory(&self, directory: &str) -> bool {
        let directory = normalize(directory);
        if directory.is_empty() {
            return false;
        }
        let path = normalize(&self.path);
        !path.is_empty() && path.starts_with(&directory)
    }
}

pub fn process_entry(process_id: u32) -> Option<ProcessEntry> {
    let path = query_path(process_id).unwrap_or_default();
    let title = window_titles(SCAN_BUDGET)
        .get(&process_id)
        .cloned()
        .unwrap_or_default();
    Some(ProcessEntry {
        pid: process_id,
        name: process_name(&path),
        path,
        title,
    })
}

pub fn scan_processes(target_directories: &[String], title_filter: &str) -> Vec<ProcessEntry> {
    let titles = window_titles(SCAN_BUDGET);
    let filter = title_filter.trim().to_lowercase();
    let mut entries = Vec::new();
    for process_id in process_ids() {
        let Some(title) = titles.get(&process_id) else {
            continue;
        };
        let path = query_path(process_id).unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let normalized_path = normalize(&path);
        let in_target_directory = target_directories.is_empty()
            || target_directories.iter().any(|directory| {
                let directory = normalize(directory);
                !directory.is_empty() && normalized_path.starts_with(&directory)
            });
        if !in_target_directory {
            continue;
        }
        if !filter.is_empty() && !title.to_lowercase().contains(&filter) {
            continue;
        }
        entries.push(ProcessEntry {
            pid: process_id,
            name: process_name(&path),
            path,
            title: title.clone(),
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

pub fn process_ids() -> Vec<u32> {
    let mut buffer = vec![0u32; PROCESS_ENTRY_LIMIT];
    let mut written = 0u32;
    let bytes = (buffer.len() * std::mem::size_of::<u32>()) as u32;
    let ok = unsafe { EnumProcesses(buffer.as_mut_ptr(), bytes, &mut written as *mut u32) };
    if ok == 0 {
        return Vec::new();
    }
    let count = written as usize / std::mem::size_of::<u32>();
    buffer.truncate(count);
    buffer.retain(|process_id| *process_id != 0);
    buffer
}

struct WindowCollector {
    entries: Vec<(u32, String)>,
    deadline: Instant,
}

pub fn window_titles(budget: Duration) -> HashMap<u32, String> {
    let mut collector = WindowCollector {
        entries: Vec::new(),
        deadline: Instant::now() + budget,
    };
    unsafe {
        EnumWindows(
            Some(collect_window),
            &mut collector as *mut WindowCollector as LPARAM,
        );
    }
    let mut titles = HashMap::new();
    for (process_id, title) in collector.entries {
        titles.entry(process_id).or_insert(title);
    }
    titles
}

unsafe extern "system" fn collect_window(window: HWND, param: LPARAM) -> i32 {
    let collector = unsafe { &mut *(param as *mut WindowCollector) };
    if collector.entries.len() >= WINDOW_ENTRY_LIMIT || Instant::now() >= collector.deadline {
        return 0;
    }
    if unsafe { IsWindowVisible(window) } == 0 {
        return 1;
    }
    if unsafe { GetWindowTextLengthW(window) } < WINDOW_TITLE_MINIMUM {
        return 1;
    }
    let mut buffer = vec![0u16; 512];
    let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 0 {
        return 1;
    }
    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(window, &mut process_id as *mut u32);
    }
    if process_id == 0 {
        return 1;
    }
    let title = String::from_utf16_lossy(&buffer[..length as usize]);
    collector.entries.push((process_id, title));
    1
}

pub fn query_path(process_id: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut buffer = vec![0u16; MAX_PATH as usize * 2];
    let mut size = buffer.len() as u32;
    let ok =
        unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size as *mut u32) };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..size as usize]))
}

pub fn process_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

pub fn normalize(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

pub fn find_library(directory: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(directory).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_library = path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("dll"))
            .unwrap_or(false);
        if !is_library {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if name.contains("hgameinject") {
            return Some(path);
        }
    }
    None
}

pub fn resolve_library(directory: &Path) -> HostResult<PathBuf> {
    match find_library(directory) {
        Some(found) => Ok(found),
        None => Err(error_code("locate HGameInject.dll")),
    }
}

pub fn terminate(process_id: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, process_id) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return false;
    }
    let result = unsafe { TerminateProcess(handle, 1) };
    unsafe {
        CloseHandle(handle);
    }
    result != 0
}
