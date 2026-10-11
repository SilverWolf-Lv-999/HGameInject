use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tiny_http::{Header, Method, Request, Response, Server};

use serde_json::json;

use crate::web::api::{Session, SessionView};

const INDEX_HTML: &str = include_str!("static/index.html");
const STYLE_CSS: &str = include_str!("static/style.css");
const APP_JS: &str = include_str!("static/app.js");
const REFRESH_COOLDOWN: Duration = Duration::from_millis(750);

static LAST_SCAN: std::sync::OnceLock<parking_lot::Mutex<Instant>> = std::sync::OnceLock::new();
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn serve(session: Arc<Session>) -> std::io::Result<()> {
    let port = session.client().webPort;
    let server = match Server::http(("127.0.0.1", port)) {
        Ok(bound) => bound,
        Err(_) => Server::http("127.0.0.1:0").map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::AddrInUse, error.to_string())
        })?,
    };
    let address = server.server_addr();
    let url = format!("http://{address}/");
    println!("HGameInject dashboard: {url}");
    println!("Press Ctrl+C to stop.");
    if session.client().autoOpenBrowser {
        crate::utility::shell::open_url(&url);
    }
    for request in server.incoming_requests() {
        if SHUTDOWN.load(Ordering::Acquire) {
            break;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle(&session, request)
        }));
        if result.is_err() {
            eprintln!("request handler panicked");
        }
    }
    Ok(())
}

pub fn stop() {
    SHUTDOWN.store(true, Ordering::Release);
}

fn handle(session: &Arc<Session>, mut request: Request) {
    let is_get = *request.method() == Method::Get;
    let is_post = *request.method() == Method::Post;
    let url = request.url().to_owned();
    let path = url.split('?').next().unwrap_or("/").to_owned();
    match path.as_str() {
        "/" | "/index.html" if is_get => {
            respond_html(request, INDEX_HTML);
        }
        "/style.css" if is_get => {
            respond_text(request, STYLE_CSS, "text/css; charset=utf-8");
        }
        "/app.js" if is_get => {
            respond_text(request, APP_JS, "text/javascript; charset=utf-8");
        }
        "/api/health" if is_get => {
            respond_json(request, json!({ "ok": true }));
        }
        "/api/processes" if is_get => {
            let entries = session.list_processes();
            respond_json(request, json!({ "processes": entries }));
        }
        "/api/state" if is_get => {
            session.refresh();
            let view: SessionView = session.view();
            respond_json(request, view);
        }
        "/api/attach" if is_post => {
            let payload = read_payload(&mut request);
            let process_id = payload
                .as_ref()
                .and_then(|value| value.get("pid"))
                .and_then(|value| value.as_u64())
                .map(|pid| pid as u32);
            match process_id {
                Some(process_id) => match session.attach(process_id) {
                    Ok(()) => respond_json(request, json!({ "started": true })),
                    Err(error) => respond_error(request, &error),
                },
                None => respond_error(request, "pid required"),
            }
        }
        "/api/detach" if is_post => {
            session.detach();
            respond_json(request, json!({ "ok": true }));
        }
        "/api/refresh" if is_post => {
            session.refresh();
            respond_json(request, json!({ "ok": true }));
        }
        "/api/processes" if is_post => {
            if !scan_allowed() {
                respond_error(request, "scan throttled");
                return;
            }
            let entries = session.list_processes();
            respond_json(request, json!({ "processes": entries }));
        }
        _ => {
            if let Some((name, action)) = route_module(&path) {
                if is_post {
                    handle_module(session, request, &name, &action);
                    return;
                }
            }
            respond_error(request, "not found");
        }
    }
}

fn handle_module(session: &Arc<Session>, mut request: Request, name: &str, action: &str) {
    let payload = read_payload(&mut request);
    match action {
        "toggle" => {
            let enabled = payload
                .as_ref()
                .and_then(|value| value.get("enabled"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            match session.toggle(name, enabled) {
                Ok(()) => respond_json(request, json!({ "ok": true })),
                Err(error) => respond_error(request, &error),
            }
        }
        "config" => {
            let entries = payload
                .as_ref()
                .and_then(|value| value.as_object())
                .cloned()
                .unwrap_or_default();
            let mut applied = 0usize;
            let mut last_error = String::new();
            for (key, value) in entries {
                match session.set_config(name, &key, value) {
                    Ok(()) => applied += 1,
                    Err(error) => last_error = error,
                }
            }
            if applied == 0 && !last_error.is_empty() {
                respond_error(request, &last_error);
                return;
            }
            respond_json(request, json!({ "applied": applied }));
        }
        _ => respond_error(request, "unknown module action"),
    }
}

fn route_module(path: &str) -> Option<(String, String)> {
    let mut parts = path.trim_start_matches('/').split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("api"), Some("module"), Some(name), Some(action)) => {
            Some((name.to_owned(), action.to_owned()))
        }
        _ => None,
    }
}

fn read_payload(request: &mut Request) -> Option<serde_json::Value> {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return None;
    }
    if body.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&body).ok()
}

fn respond_json<T: serde::Serialize>(request: Request, value: T) {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned());
    let header = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .expect("valid header");
    let _ = request.respond(Response::from_string(body).with_header(header));
}

fn respond_html(request: Request, body: &str) {
    let header = Header::from_bytes("Content-Type", "text/html; charset=utf-8")
        .expect("valid header");
    let _ = request.respond(Response::from_string(body).with_header(header));
}

fn respond_text(request: Request, body: &str, content_type: &str) {
    let header = Header::from_bytes("Content-Type", content_type).expect("valid header");
    let _ = request.respond(Response::from_string(body).with_header(header));
}

fn respond_error(request: Request, message: &str) {
    let header = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .expect("valid header");
    let body = json!({ "error": message }).to_string();
    let response = Response::from_string(body)
        .with_header(header)
        .with_status_code(400);
    let _ = request.respond(response);
}

fn scan_allowed() -> bool {
    let lock = LAST_SCAN.get_or_init(|| parking_lot::Mutex::new(Instant::now() - REFRESH_COOLDOWN));
    let mut last = lock.lock();
    if Instant::now().duration_since(*last) < REFRESH_COOLDOWN {
        return false;
    }
    *last = Instant::now();
    true
}
