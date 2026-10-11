use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use windows_sys::Win32::System::Console::AttachConsole;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;

use crate::config::ClientConfig;

pub const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const fn sort_order(self) -> u8 {
        match self {
            Level::Debug => 0,
            Level::Info => 1,
            Level::Warn => 2,
            Level::Error => 3,
        }
    }

    pub fn from_name(name: &str) -> Level {
        match name.to_ascii_lowercase().as_str() {
            "debug" => Level::Debug,
            "warn" | "warning" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InjectLogger {
    threshold: Level,
    debug_output: bool,
    console: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LoggerOptions {
    pub threshold: Level,
    pub debug_output: bool,
    pub console: bool,
}

static GLOBAL_LOGGER: OnceLock<InjectLogger> = OnceLock::new();
static CONSOLE_ATTACHED: AtomicBool = AtomicBool::new(false);

pub fn log_directory() -> PathBuf {
    let root = std::env::var("TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    root.join("HGameInject")
}

pub fn log_file_path() -> PathBuf {
    log_directory().join(format!("pid-{}.log", crate::platform::current_process_id()))
}

pub fn write_file_line(message: &str) {
    let directory = log_directory();
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let path = log_file_path();
    truncate_log(&path);
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = file.write_all(message.as_bytes());
    let _ = file.write_all(b"\n");
}

fn truncate_log(path: &PathBuf) {
    let Ok(mut file) = std::fs::OpenOptions::new().read(true).open(path) else {
        return;
    };
    let length = file.seek(SeekFrom::End(0)).unwrap_or(0);
    if length <= 256 * 1024 {
        return;
    }
    let mut previous = Vec::new();
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_end(&mut previous).is_err() {
        return;
    }
    let keep = previous.len().saturating_sub(64 * 1024);
    let _ = std::fs::write(path, &previous[keep..]);
}

impl InjectLogger {
    pub fn new(options: LoggerOptions) -> InjectLogger {
        InjectLogger {
            threshold: options.threshold,
            debug_output: options.debug_output,
            console: options.console,
        }
    }

    pub fn from_client_config(config: &ClientConfig) -> InjectLogger {
        InjectLogger {
            threshold: Level::from_name(&config.logLevel),
            debug_output: config.debugOutput,
            console: config.attachConsole,
        }
    }

    pub fn register(self) {
        let _ = GLOBAL_LOGGER.set(self);
    }

    pub fn enabled(&self, level: Level) -> bool {
        level.sort_order() >= self.threshold.sort_order()
    }

    pub fn debug(&self, message: &str) {
        self.emit(Level::Debug, message);
    }

    pub fn info(&self, message: &str) {
        self.emit(Level::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.emit(Level::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.emit(Level::Error, message);
    }

    pub fn emit(&self, level: Level, message: &str) {
        if !self.enabled(level) {
            return;
        }
        let line = format!("[HGameInject][{}] {}", level.prefix(), message);
        if self.console {
            write_console(&line);
        }
        if self.debug_output {
            write_debug_line(&line);
        }
        write_file_line(&line);
    }
}

pub fn global_logger() -> InjectLogger {
    GLOBAL_LOGGER
        .get()
        .cloned()
        .unwrap_or_else(|| {
            InjectLogger::new(LoggerOptions {
                threshold: Level::Info,
                debug_output: true,
                console: false,
            })
        })
}

pub fn write_line(level: Level, message: &str) {
    global_logger().emit(level, message);
}

pub fn write_console(message: &str) {
    let attached = CONSOLE_ATTACHED.load(Ordering::Acquire);
    if !attached {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        CONSOLE_ATTACHED.store(true, Ordering::Release);
    }
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(message.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

pub fn write_debug_line(message: &str) {
    let mut buffer: Vec<u16> = message.encode_utf16().collect();
    buffer.push(b'\n' as u16);
    buffer.push(0);
    unsafe {
        OutputDebugStringW(buffer.as_ptr());
    }
}
