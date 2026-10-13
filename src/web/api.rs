use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;

use crate::config::ClientConfig;
use crate::protocol::{Request, Response, SnapshotPayload, decode_response, pipe_name};
use crate::utility::host::HostResult;
use crate::utility::inject::inject_library;
use crate::utility::pipe;
use crate::utility::windows::{ProcessEntry, query_path, scan_processes};

#[derive(Debug, Clone, Serialize)]
pub struct ProcessRecord {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub title: String,
}

impl From<ProcessEntry> for ProcessRecord {
    fn from(entry: ProcessEntry) -> ProcessRecord {
        ProcessRecord {
            pid: entry.pid,
            name: entry.name,
            path: entry.path,
            title: entry.title,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub attached: bool,
    pub attaching: bool,
    pub processId: u32,
    pub processName: String,
    pub runtime: String,
    pub processPath: String,
    pub message: String,
    pub modules: Vec<crate::protocol::ModuleSnapshot>,
}

pub struct Service {
    pub pipe_name: String,
}

impl Service {
    pub fn new(process_id: u32) -> Service {
        Service {
            pipe_name: pipe_name(process_id),
        }
    }

    pub fn call(&self, request: &Request) -> HostResult<Response> {
        let connection = pipe::open_client(&self.pipe_name, 2500)?;
        let payload = serde_json::to_string(request).unwrap_or_else(|_| "{}".to_owned());
        let result = connection
            .write_line(&payload)
            .and_then(|_| connection.read_line(5000));
        match result {
            Ok(line) => match decode_response(&line) {
                Some(response) => Ok(response),
                None => Err("malformed response".to_owned()),
            },
            Err(error) => Err(error),
        }
    }
}

impl Service {
    pub fn toggle(&self, name: &str, enabled: bool) -> HostResult<Response> {
        self.call(&Request::Toggle {
            name: name.to_owned(),
            enabled,
        })
    }

    pub fn set_config(
        &self,
        name: &str,
        key: &str,
        value: serde_json::Value,
    ) -> HostResult<Response> {
        self.call(&Request::SetConfig {
            name: name.to_owned(),
            key: key.to_owned(),
            value,
        })
    }

    pub fn snapshot(&self) -> HostResult<Response> {
        self.call(&Request::Snapshot)
    }

    pub fn pulse(&self) -> HostResult<Response> {
        self.call(&Request::Pulse)
    }
}

pub struct Session {
    client: ClientConfig,
    state: Mutex<SessionState>,
}

struct SessionState {
    attached: bool,
    attaching: bool,
    processId: u32,
    processName: String,
    runtime: String,
    processPath: String,
    message: String,
    snapshot: Option<SnapshotPayload>,
    service: Option<Arc<Service>>,
}

impl Session {
    pub fn new(client: ClientConfig) -> Arc<Session> {
        Arc::new(Session {
            client,
            state: Mutex::new(SessionState {
                attached: false,
                attaching: false,
                processId: 0,
                processName: String::new(),
                runtime: String::new(),
                processPath: String::new(),
                message: "select a target process".to_owned(),
                snapshot: None,
                service: None,
            }),
        })
    }

    pub fn client(&self) -> ClientConfig {
        self.client.clone()
    }

    pub fn client_mut(&self) -> &ClientConfig {
        &self.client
    }

    pub fn list_processes(&self) -> Vec<ProcessRecord> {
        scan_processes(
            &self.client.targetDirectories,
            &self.client.windowTitleFilter,
        )
        .into_iter()
        .map(ProcessRecord::from)
        .collect()
    }

    pub fn view(&self) -> SessionView {
        let state = self.state.lock();
        let modules = state
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.modules.clone())
            .unwrap_or_default();
        SessionView {
            attached: state.attached,
            attaching: state.attaching,
            processId: state.processId,
            processName: state.processName.clone(),
            runtime: state.runtime.clone(),
            processPath: state.processPath.clone(),
            message: state.message.clone(),
            modules,
        }
    }

    pub fn attach(self: &Arc<Session>, process_id: u32) -> HostResult<()> {
        {
            let mut state = self.state.lock();
            if state.attaching {
                return Err("attach already running".to_owned());
            }
            state.attaching = true;
            state.message = format!("attaching to pid {process_id}");
        }
        let session = Arc::clone(self);
        let spawned = thread::Builder::new()
            .name("HGameInject-Attach".to_owned())
            .spawn(move || session.run_attach(process_id));
        if let Err(error) = spawned {
            let mut state = self.state.lock();
            state.attaching = false;
            state.message = format!("attach thread failed: {error}");
            return Err(error.to_string());
        }
        Ok(())
    }

    fn run_attach(&self, process_id: u32) {
        let result = self.perform_attach(process_id);
        let mut state = self.state.lock();
        state.attaching = false;
        match result {
            Ok(snapshot) => {
                state.attached = true;
                state.processId = snapshot.processId;
                state.processName = snapshot.processName.clone();
                state.runtime = snapshot.runtime.clone();
                state.processPath = query_path(process_id).unwrap_or_default();
                state.message = format!(
                    "attached to {} ({})",
                    snapshot.processName, snapshot.processId
                );
                state.snapshot = Some(snapshot);
            }
            Err(error) => {
                state.attached = false;
                state.message = error;
                state.runtime.clear();
                state.snapshot = None;
                state.service = None;
            }
        }
    }

    fn perform_attach(&self, process_id: u32) -> HostResult<SnapshotPayload> {
        let service = Arc::new(Service::new(process_id));
        let mut injected = false;
        if !service_reachable(&service) {
            let library = self.resolve_library()?;
            let library = library.to_string_lossy().into_owned();
            inject_library(process_id, &library)?;
            injected = true;
        }
        let mut last_error = String::new();
        for _ in 0..40 {
            match service.snapshot() {
                Ok(Response::Snapshot(snapshot)) => {
                    let mut state = self.state.lock();
                    state.service = Some(Arc::clone(&service));
                    return Ok(snapshot);
                }
                Ok(Response::Error { message }) => last_error = message,
                Ok(_) => last_error = "unexpected response".to_owned(),
                Err(error) => last_error = error,
            }
            thread::sleep(Duration::from_millis(250));
        }
        if injected {
            return Err(format!(
                "library loaded but bridge unreachable: {last_error}"
            ));
        }
        Err(format!("bridge unreachable: {last_error}"))
    }

    fn resolve_library(&self) -> HostResult<PathBuf> {
        if !self.client.dllPath.trim().is_empty() {
            let configured = PathBuf::from(self.client.dllPath.trim());
            if configured.is_file() {
                return Ok(configured);
            }
            return Err(format!("dllPath not found: {}", configured.display()));
        }
        let directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        crate::utility::windows::resolve_library(&directory)
    }

    pub fn toggle(&self, name: &str, enabled: bool) -> HostResult<()> {
        let service = self.service()?;
        let response = service.toggle(name, enabled)?;
        if let Response::Error { message } = response {
            return Err(message);
        }
        self.refresh();
        Ok(())
    }

    pub fn set_config(
        &self,
        name: &str,
        key: &str,
        value: serde_json::Value,
    ) -> HostResult<()> {
        let service = self.service()?;
        let response = service.set_config(name, key, value)?;
        if let Response::Error { message } = response {
            return Err(message);
        }
        self.refresh();
        Ok(())
    }

    pub fn refresh(&self) {
        let service = match self.state.lock().service.clone() {
            Some(service) => service,
            None => return,
        };
        match service.snapshot() {
            Ok(Response::Snapshot(snapshot)) => {
                let mut state = self.state.lock();
                state.attached = true;
                state.processId = snapshot.processId;
                state.runtime = snapshot.runtime.clone();
                state.snapshot = Some(snapshot);
                state.message.clear();
            }
            Ok(Response::Error { message }) => {
                let mut state = self.state.lock();
                state.message = message;
            }
            Ok(_) => {}
            Err(error) => {
                let mut state = self.state.lock();
                state.attached = false;
                state.message = format!("connection lost: {error}");
                state.snapshot = None;
                state.service = None;
            }
        }
    }

    pub fn detach(&self) {
        let mut state = self.state.lock();
        state.attached = false;
        state.attaching = false;
        state.processId = 0;
        state.processName.clear();
        state.runtime.clear();
        state.processPath.clear();
        state.snapshot = None;
        state.service = None;
        state.message = "select a target process".to_owned();
    }

    fn service(&self) -> HostResult<Arc<Service>> {
        self.state
            .lock()
            .service
            .clone()
            .ok_or_else(|| "no attached process".to_owned())
    }
}

fn service_reachable(service: &Service) -> bool {
    matches!(service.call(&Request::Ping), Ok(Response::Pong { .. }))
}

