use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum InjectError {
    Memory(ListenAddress),
    Protect(ListenAddress, u32),
    PatternNotFound(String),
    InvalidState(String),
    Config(String),
    Json(serde_json::Error),
    Io(std::io::Error),
    Thread(String),
    Host(String),
}

#[derive(Debug, Clone, Copy)]
pub struct ListenAddress(pub usize);

impl Display for ListenAddress {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "{:#x}", self.0)
    }
}

impl Display for InjectError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            InjectError::Memory(address) => write!(formatter, "memory access failed at {address}"),
            InjectError::Protect(address, code) => {
                write!(formatter, "memory protect failed at {address} with code {code}")
            }
            InjectError::PatternNotFound(name) => write!(formatter, "pattern not found: {name}"),
            InjectError::InvalidState(message) => write!(formatter, "invalid state: {message}"),
            InjectError::Config(message) => write!(formatter, "config error: {message}"),
            InjectError::Json(error) => write!(formatter, "json error: {error}"),
            InjectError::Io(error) => write!(formatter, "io error: {error}"),
            InjectError::Thread(message) => write!(formatter, "thread error: {message}"),
            InjectError::Host(message) => write!(formatter, "host error: {message}"),
        }
    }
}

impl std::error::Error for InjectError {}

impl From<std::io::Error> for InjectError {
    fn from(error: std::io::Error) -> Self {
        InjectError::Io(error)
    }
}

impl From<serde_json::Error> for InjectError {
    fn from(error: serde_json::Error) -> Self {
        InjectError::Json(error)
    }
}

pub type InjectResult<T> = Result<T, InjectError>;
