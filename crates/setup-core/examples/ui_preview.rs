//! Serves the app UI in a normal browser, backed by the real rules engine.
//! Handy for UI work: edit HTML/CSS/JS and just reload the page.
//!
//!   cargo run -p setup-core --example ui_preview        -> http://127.0.0.1:1420
//!
//! The UI calls `POST /invoke/<command>` here instead of Tauri's `invoke`.
//! Live telemetry only offers demo mode here (no game connection).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use setup_core::live::{LiveSession, NoSource};
use setup_core::{CarContext, History, RuleBook, Symptom};

fn main() {
    let ui_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/ui");
    let rules = std::sync::Arc::new(RuleBook::builtin());
    let live = LiveSession::start(Box::new(NoSource), None);
    let listener = TcpListener::bind("127.0.0.1:1420").expect("port 1420 is busy");
    println!("UI preview on http://127.0.0.1:1420");
    // One thread per connection: browsers keep idle connections open.
    for stream in listener.incoming().flatten() {
        let (ui_dir, rules, live) = (ui_dir.clone(), rules.clone(), live.clone());
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            if let Err(e) = handle(stream, &ui_dir, &rules, &live) {
                eprintln!("request failed: {e}");
            }
        });
    }
}

fn handle(mut stream: TcpStream, ui_dir: &Path, rules: &RuleBook, live: &LiveSession) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));

    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;

    let (status, mime, payload) = if method == "POST" && path.starts_with("/invoke/") {
        match invoke(&path["/invoke/".len()..], &body, rules, live) {
            Ok(v) => ("200 OK", "application/json", v.to_string().into_bytes()),
            Err(e) => ("400 Bad Request", "text/plain", e.into_bytes()),
        }
    } else {
        let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
        if rel.contains("..") {
            ("404 Not Found", "text/plain", b"not found".to_vec())
        } else {
            match std::fs::read(ui_dir.join(rel)) {
                Ok(bytes) => ("200 OK", mime_for(rel), bytes),
                Err(_) => ("404 Not Found", "text/plain", b"not found".to_vec()),
            }
        }
    };

    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(&payload)
}

fn invoke(cmd: &str, body: &[u8], rules: &RuleBook, live: &LiveSession) -> Result<Value, String> {
    let args: Value =
        if body.is_empty() { json!({}) } else { serde_json::from_slice(body).map_err(|e| e.to_string())? };
    let arg = |name: &str| args.get(name).cloned().ok_or(format!("missing argument '{name}'"));
    match cmd {
        "catalog" => Ok(json!({ "behaviors": rules.behaviors, "params": rules.params })),
        "symptom_label" => {
            let s: Symptom = serde_json::from_value(arg("symptom")?).map_err(|e| e.to_string())?;
            Ok(json!(rules.symptom_label(&s)))
        }
        "suggest" => {
            let car: CarContext = serde_json::from_value(arg("car")?).map_err(|e| e.to_string())?;
            let symptoms: Vec<Symptom> = serde_json::from_value(arg("symptoms")?).map_err(|e| e.to_string())?;
            let history: History = match args.get("history") {
                Some(h) if !h.is_null() => serde_json::from_value(h.clone()).map_err(|e| e.to_string())?,
                _ => History::default(),
            };
            Ok(json!(rules.suggest_with_history(&car, &symptoms, &history)))
        }
        "live_status" => Ok(json!(live.status(rules))),
        "live_next_run" => {
            match args.get("run").and_then(Value::as_u64) {
                Some(r) => live.start_run(r as u32),
                None => live.next_run(),
            }
            Ok(Value::Null)
        }
        "live_set_run" => {
            live.set_run(arg("run")?.as_u64().unwrap_or(1) as u32);
            Ok(Value::Null)
        }
        "live_new_session" => {
            live.new_session();
            Ok(Value::Null)
        }
        "recordings_dir" => Ok(Value::Null),
        "track_rename" => {
            let corner = arg("corner")?.as_u64().unwrap_or(0) as u32;
            let name = arg("name")?.as_str().unwrap_or("").to_string();
            live.rename_corner(corner, name);
            Ok(Value::Null)
        }
        "live_reset" => {
            live.reset();
            Ok(Value::Null)
        }
        "live_set_demo" => {
            live.set_demo(arg("on")?.as_bool().unwrap_or(false));
            Ok(Value::Null)
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}
