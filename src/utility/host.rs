pub type HostResult<T> = Result<T, String>;

pub fn last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

pub fn error_with(operation: &str, code: u32) -> String {
    format!("{operation} failed with code {code}")
}

pub fn error_code(operation: &str) -> String {
    error_with(operation, last_error())
}
