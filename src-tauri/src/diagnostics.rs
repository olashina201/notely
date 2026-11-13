//! Bounded local JSONL diagnostics. Never pass prompts, response bodies, or keys here.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Instant,
};
use tauri::{AppHandle, Emitter, Manager};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
static JOURNAL: OnceLock<Mutex<Journal>> = OnceLock::new();

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub timestamp: String,
    pub level: String,
    pub stage: String,
    pub request_id: Option<String>,
    pub fields: Value,
}

struct Journal {
    path: PathBuf,
    write_error: Option<String>,
    max_bytes: u64,
}
impl Journal {
    fn append(&mut self, entry: &Entry) -> Result<(), String> {
        if fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0) >= self.max_bytes {
            let previous = self.path.with_extension("previous.jsonl");
            if previous.exists() {
                fs::remove_file(&previous).map_err(|e| e.to_string())?;
            }
            fs::rename(&self.path, previous).map_err(|e| e.to_string())?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path).map_err(|e| e.to_string())?;
        let mut bytes = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())
    }

    fn entries(&self, limit: usize) -> Vec<Entry> {
        let mut entries = Vec::new();
        for path in [
            self.path.with_extension("previous.jsonl"),
            self.path.clone(),
        ] {
            if let Ok(file) = fs::File::open(path) {
                for line in BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(entry) = serde_json::from_str::<Entry>(&line) {
                        entries.push(entry);
                    }
                }
            }
        }
        entries.into_iter().rev().take(limit.min(500)).collect()
    }
}

pub fn init(app: &AppHandle) -> Result<(), String> {
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = JOURNAL.set(Mutex::new(Journal {
        path: dir.join("notely.jsonl"),
        write_error: None,
        max_bytes: MAX_BYTES,
    }));
    record(
        "info",
        "app.started",
        None,
        json!({"version": env!("CARGO_PKG_VERSION")}),
    );
    Ok(())
}

pub fn record(level: &str, stage: &str, request_id: Option<&str>, fields: Value) {
    let entry = Entry {
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        level: level.into(),
        stage: safe_label(stage),
        request_id: request_id.map(safe_label),
        fields: sanitize_fields(fields),
    };
    if let Some(journal) = JOURNAL.get() {
        if let Ok(mut journal) = journal.lock() {
            if let Err(error) = journal.append(&entry) {
                eprintln!("[notely] Diagnostics write failed: {error}");
                journal.write_error = Some(error);
            }
        }
    }
    // Metadata only. This also makes `npm run tauri ...` useful for troubleshooting.
    eprintln!(
        "[notely] {} {} {} {}",
        entry.level,
        entry.stage,
        entry.request_id.as_deref().unwrap_or("-"),
        entry.fields
    );
}

fn safe_label(value: &str) -> String {
    if value.starts_with("sk-")
        || value.starts_with("AIza")
        || value.len() > 160
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._:/".contains(c))
    {
        "[redacted]".into()
    } else {
        value.into()
    }
}

fn sanitize_fields(fields: Value) -> Value {
    let allowed = [
        "version",
        "slot",
        "includeScreenshot",
        "provider",
        "model",
        "elapsedMs",
        "status",
        "providerRequestId",
        "stream",
        "jsonMode",
        "available",
        "images",
        "imageBytes",
        "imageBytesBefore",
        "payloadBytes",
        "responseWaitSeconds",
        "chunks",
        "characters",
        "success",
        "failureKind",
        "kind",
        "code",
        "timeout",
        "connect",
        "decode",
        "host",
        "count",
        "event",
        "hidden",
        "enabled",
        "granted",
        "width",
        "height",
        "lines",
    ];
    let mut safe = serde_json::Map::new();
    if let Some(fields) = fields.as_object() {
        for (key, value) in fields {
            if !allowed.contains(&key.as_str()) || value.is_object() || value.is_array() {
                continue;
            }
            safe.insert(
                key.clone(),
                if let Some(text) = value.as_str() {
                    json!(safe_label(text))
                } else {
                    value.clone()
                },
            );
        }
    }
    Value::Object(safe)
}

pub struct Trace {
    pub id: String,
    pub provider: String,
    pub model: String,
    started: Instant,
}

pub fn failure_kind(error: &str) -> &'static str {
    let message = error.to_lowercase();
    if message.contains("screencapturekit") && message.contains("declined") { "screen_permission_identity" }
    else if message.contains("keychain") {
        "keychain"
    } else if message.contains("api key") {
        "credentials"
    } else if message.contains("seconds")
        || message.contains("timed out")
        || message.contains("deadline")
    {
        "timeout"
    } else if message.contains("reach") || message.contains("connection") {
        "network"
    } else if message.contains("(4") || message.contains("(5") {
        "provider_rejected"
    } else if message.contains("stream") || message.contains("json") {
        "stream_protocol"
    } else {
        "request_error"
    }
}
impl Trace {
    pub fn new(id: &str, provider: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            provider: provider.into(),
            model: model.into(),
            started: Instant::now(),
        }
    }
    pub fn log(&self, level: &str, stage: &str, mut fields: Value) {
        fields["provider"] = json!(self.provider);
        fields["model"] = json!(self.model);
        fields["elapsedMs"] = json!(self.started.elapsed().as_millis() as u64);
        record(level, stage, Some(&self.id), fields);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub path: String,
    pub entries: Vec<Entry>,
    pub write_error: Option<String>,
}

#[tauri::command]
pub fn diagnostics_recent() -> Result<Snapshot, String> {
    let journal = JOURNAL
        .get()
        .ok_or("Diagnostics is not initialized")?
        .lock()
        .map_err(|_| "Diagnostics lock failed")?;
    Ok(Snapshot {
        path: journal.path.display().to_string(),
        entries: journal.entries(300),
        write_error: journal.write_error.clone(),
    })
}

#[tauri::command]
pub fn diagnostics_clear() -> Result<(), String> {
    let mut journal = JOURNAL
        .get()
        .ok_or("Diagnostics is not initialized")?
        .lock()
        .map_err(|_| "Diagnostics lock failed")?;
    for path in [
        &journal.path,
        &journal.path.with_extension("previous.jsonl"),
    ] {
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    journal.write_error = None;
    Ok(())
}

#[tauri::command]
pub fn diagnostics_probe(app: AppHandle, request_id: String) -> Result<(), String> {
    uuid::Uuid::parse_str(&request_id).map_err(|_| "Invalid diagnostic probe ID")?;
    app.emit("diagnostics:probe", json!({"requestId": request_id}))
        .map_err(|e| e.to_string())?;
    record("info", "events.probe_sent", Some(&request_id), json!({}));
    Ok(())
}

#[tauri::command]
pub fn diagnostics_frontend(
    stage: String,
    request_id: Option<String>,
    event: Option<String>,
    elapsed_ms: Option<u64>,
) -> Result<(), String> {
    let allowed = [
        "request_started",
        "request_accepted",
        "first_text",
        "request_finished",
        "request_cancelled",
        "request_failed",
        "listener_failed",
        "watchdog_timeout",
        "javascript_error",
        "unhandled_rejection",
        "listeners_ready",
    ];
    if !allowed.contains(&stage.as_str()) {
        return Err("Unknown diagnostic stage".into());
    }
    let id = request_id
        .filter(|s| s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    // Only known event names may be recorded; arbitrary error strings might contain user content.
    let allowed_events: Value =
        serde_json::from_str(include_str!("../../src/events.json")).map_err(|e| e.to_string())?;
    let event = event.filter(|name| {
        allowed_events
            .as_object()
            .unwrap()
            .values()
            .any(|v| v.as_str() == Some(name.as_str()))
    });
    let level = if [
        "listener_failed",
        "request_failed",
        "watchdog_timeout",
        "javascript_error",
        "unhandled_rejection",
    ]
    .contains(&stage.as_str())
    {
        "error"
    } else {
        "info"
    };
    record(
        level,
        &format!("ui.{stage}"),
        id.as_deref(),
        json!({"event": event, "elapsedMs": elapsed_ms}),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_persist_rotate_and_recover_from_incomplete_lines() {
        let dir = std::env::temp_dir().join(format!("notely-log-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let mut journal = Journal {
            path: dir.join("test.jsonl"),
            write_error: None,
            max_bytes: 1,
        };
        let entry = |stage: &str| Entry {
            timestamp: "2026-11-13T00:00:00Z".into(),
            level: "info".into(),
            stage: stage.into(),
            request_id: Some("test-request".into()),
            fields: json!({"status": 200}),
        };
        journal.append(&entry("request.started")).unwrap();
        journal.append(&entry("request.finished")).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&journal.path)
            .unwrap()
            .write_all(b"{incomplete\n")
            .unwrap();
        let entries = journal.entries(10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].stage, "request.finished");
        assert!(journal.path.with_extension("previous.jsonl").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&journal.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn secret_and_content_fields_cannot_enter_logs() {
        let safe = sanitize_fields(
            json!({"apiKey": "secret", "Authorization": "Bearer secret", "prompt": "private question", "text": "private answer", "screenshot": "base64", "model": "sk-private-key", "status": 401, "provider": "openai", "nested": {"key": "secret"}}),
        );
        assert_eq!(
            safe,
            json!({"model": "[redacted]", "status": 401, "provider": "openai"})
        );
        assert_eq!(safe_label("AIza-fake"), "[redacted]");
    }
}
