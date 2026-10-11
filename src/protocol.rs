use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PIPE_PREFIX: &str = "HGameInject";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Snapshot,
    Toggle {
        name: String,
        enabled: bool,
    },
    SetConfig {
        name: String,
        key: String,
        value: Value,
    },
    Pulse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Pong {
        processId: u32,
    },
    Snapshot(SnapshotPayload),
    Ok,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub processId: u32,
    pub processName: String,
    pub modules: Vec<ModuleSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSnapshot {
    pub name: String,
    pub description: String,
    pub category: String,
    pub keybind: u32,
    pub enabled: bool,
    pub schema: Vec<ConfigSchema>,
    pub values: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub choices: Vec<String>,
}

pub fn pipe_name(process_id: u32) -> String {
    format!(r"\\.\pipe\{PIPE_PREFIX}-{process_id}")
}

pub fn decode_request(line: &str) -> Option<Request> {
    serde_json::from_str(line.trim()).ok()
}

pub fn encode_response(response: &Response) -> String {
    serde_json::to_string(response).unwrap_or_else(|_| {
        r#"{"Error":{"message":"serialization failed"}}"#.to_owned()
    })
}

pub fn decode_response(line: &str) -> Option<Response> {
    serde_json::from_str(line.trim()).ok()
}
