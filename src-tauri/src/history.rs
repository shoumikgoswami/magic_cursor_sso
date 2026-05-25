use crate::config::config_dir;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditEntry {
    pub tool: String,
    pub args: serde_json::Value,
    pub result: String,
    pub duration_ms: u64,
    pub approved_by: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionEntry {
    pub id: String,
    pub timestamp: String,
    pub content_type: String,
    pub context_preview: String,
    pub query: String,
    pub response: String,
    pub tool_calls: Vec<AuditEntry>,
}

fn history_path() -> std::path::PathBuf {
    config_dir().join("history.json")
}

pub fn load_history() -> Vec<SessionEntry> {
    let path = history_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_session(entry: SessionEntry, max_entries: usize) {
    let mut history = load_history();
    history.insert(0, entry);
    history.truncate(max_entries);
    if let Ok(json) = serde_json::to_string_pretty(&history) {
        let path = history_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, json);
    }
}

pub fn clear_history() {
    let _ = fs::write(history_path(), "[]");
}

pub fn delete_session(id: &str) {
    let mut history = load_history();
    history.retain(|e| e.id != id);
    if let Ok(json) = serde_json::to_string_pretty(&history) {
        let _ = fs::write(history_path(), json);
    }
}

pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn append_audit_log(entry: &AuditEntry) {
    use std::io::Write;
    let path = config_dir().join("audit.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let ts = now_iso();
        let line = serde_json::json!({
            "ts": ts,
            "tool": entry.tool,
            "args": entry.args,
            "result": entry.result,
            "duration_ms": entry.duration_ms,
            "approved_by": entry.approved_by,
        });
        let _ = writeln!(file, "{}", line);
    }
}
