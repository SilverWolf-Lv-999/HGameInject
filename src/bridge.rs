use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use serde_json::Value;

use crate::bootstrap::Host;
use crate::inject_log::{Level, write_line};
use crate::modules::bus::EventKind;
use crate::protocol::{
    ModuleSnapshot, Request, Response, SnapshotPayload, decode_request, encode_response, pipe_name,
};
use crate::utility::pipe;

pub struct BridgeModule {
    host: Weak<Host>,
    shutdown: AtomicBool,
}

impl BridgeModule {
    pub fn start(host: &std::sync::Arc<Host>) -> std::sync::Arc<BridgeModule> {
        let module = std::sync::Arc::new(BridgeModule {
            host: std::sync::Arc::downgrade(host),
            shutdown: AtomicBool::new(false),
        });
        let worker = std::sync::Arc::clone(&module);
        let spawned = thread::Builder::new()
            .name("HGameInject-Bridge".to_owned())
            .spawn(move || worker.run());
        if let Err(error) = spawned {
            write_line(
                Level::Error,
                &format!("bridge thread spawn failed: {error}"),
            );
        }
        module
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    fn run(&self) {
        let name = pipe_name(crate::platform::current_process_id());
        while !self.shutdown.load(Ordering::Acquire) {
            let server = match pipe::create_server(&name) {
                Ok(server) => server,
                Err(error) => {
                    write_line(Level::Error, &format!("bridge pipe failed: {error}"));
                    thread::sleep(std::time::Duration::from_millis(250));
                    continue;
                }
            };
            if !matches!(pipe::wait_for_client(&server), Ok(true)) {
                continue;
            }
            if let Ok(line) = server.read_line(5000) {
                let response = respond(&self.host, &line);
                let _ = server.write_line(&response);
            }
        }
    }
}

fn respond(host: &Weak<Host>, line: &str) -> String {
    let Some(request) = decode_request(line) else {
        return encode_response(&Response::Error {
            message: "invalid request".to_owned(),
        });
    };
    let Some(host) = host.upgrade() else {
        return encode_response(&Response::Error {
            message: "host unavailable".to_owned(),
        });
    };
    let manager = host.manager();
    let response = match request {
        Request::Ping => Response::Pong {
            processId: host.client().process_id(),
        },
        Request::Pulse => {
            manager.update(EventKind::Update);
            Response::Ok
        }
        Request::Toggle { name, enabled } => {
            let changed = if enabled {
                manager.open(&name)
            } else {
                manager.close(&name)
            };
            if changed {
                host.sync_module_state();
                Response::Ok
            } else {
                Response::Error {
                    message: format!("module not found: {name}"),
                }
            }
        }
        Request::SetConfig { name, key, value } => change_config(&host, &name, &key, value),
        Request::Snapshot => Response::Snapshot(snapshot(&host)),
    };
    encode_response(&response)
}

fn change_config(host: &Host, name: &str, key: &str, value: Value) -> Response {
    let manager = host.manager();
    if key == "keybind" {
        if !value.is_u64() {
            return Response::Error {
                message: "keybind must be an unsigned number".to_owned(),
            };
        }
        let key_code = value.as_u64().unwrap_or(0) as u32;
        if !manager.set_binding(name, key_code) {
            return Response::Error {
                message: format!("module not found: {name}"),
            };
        }
        host.sync_module_state();
        return Response::Ok;
    }
    if let Value::Object(entries) = value {
        let mut applied = 0usize;
        for (entry_key, entry_value) in entries {
            if manager.set_config(name, &entry_key, entry_value) {
                applied += 1;
            }
        }
        if applied == 0 {
            return Response::Error {
                message: format!("no configuration applied for {name}"),
            };
        }
        host.sync_module_state();
        return Response::Ok;
    }
    if !manager.set_config(name, key, value) {
        return Response::Error {
            message: format!("configuration rejected: {name}.{key}"),
        };
    }
    host.sync_module_state();
    Response::Ok
}

fn snapshot(host: &Host) -> SnapshotPayload {
    let client = host.client();
    let modules: Vec<ModuleSnapshot> = host.manager().snapshots();
    SnapshotPayload {
        processId: client.process_id(),
        processName: client
            .module()
            .path()
            .ok()
            .map(|path| crate::utility::windows::process_name(&path))
            .unwrap_or_else(|| format!("pid-{}", client.process_id())),
        runtime: crate::platform::host_runtime().name().to_owned(),
        modules,
    }
}
