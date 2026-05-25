use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    // provider selection
    #[serde(default = "default_provider")]
    pub provider: String,          // "ollama" | "openai" | "groq" | "anthropic"
    #[serde(default = "default_ollama_url")]
    pub ollama_base_url: String,   // default: http://localhost:11434
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default)]
    pub groq_api_key: String,
    #[serde(default)]
    pub anthropic_api_key: String,

    // v1 fields
    pub default_model: String,
    pub vision_model: String,
    #[serde(default)]
    pub stt_model: String,   // speech-to-text model; empty = provider default
    pub system_prompt: String,
    pub reversal_threshold: usize,
    pub window_ms: u64,
    pub min_displacement: i32,
    pub cooldown_ms: u64,
    pub capture_radius: u32,
    pub overlay_width: u32,
    pub overlay_height: u32,
    // v2 additions
    #[serde(default = "default_true")]
    pub enable_tools: bool,
    #[serde(default = "default_tool_categories")]
    pub allowed_tool_categories: Vec<String>,
    #[serde(default = "default_false")]
    pub always_ask_before_ui_control: bool,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
    #[serde(default = "default_true")]
    pub auto_dismiss_short_responses: bool,
    #[serde(default = "default_word_threshold")]
    pub auto_dismiss_word_threshold: usize,
    #[serde(default = "default_dismiss_delay")]
    pub auto_dismiss_delay_ms: u64,
    #[serde(default = "default_true")]
    pub quick_actions_enabled: bool,
    #[serde(default = "default_history_max")]
    pub history_max_entries: usize,
    #[serde(default = "default_read_roots")]
    pub read_allowed_roots: Vec<String>,
    /// When true, the UI strips explanatory preamble from responses before
    /// displaying and always strips it when inserting text.
    #[serde(default)]
    pub clean_responses: bool,
}

fn default_provider() -> String { "ollama".to_string() }
fn default_ollama_url() -> String { "http://localhost:11434".to_string() }
fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_tool_categories() -> Vec<String> {
    vec![
        "Screen".into(),
        "Clipboard".into(),
        "FileRead".into(),
        "FileWrite".into(),
        "Browser".into(),
        "Shell".into(),
        "UIControl".into(),
    ]
}
fn default_max_tool_iterations() -> usize { 10 }
fn default_word_threshold() -> usize { 40 }
fn default_dismiss_delay() -> u64 { 5000 }
fn default_history_max() -> usize { 50 }
fn default_read_roots() -> Vec<String> {
    dirs::home_dir()
        .map(|p| vec![p.to_string_lossy().to_string()])
        .unwrap_or_default()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            ollama_base_url: default_ollama_url(),
            openai_api_key: String::new(),
            groq_api_key: String::new(),
            anthropic_api_key: String::new(),
            default_model: "llama3.2".to_string(),
            vision_model: "".to_string(),
            stt_model: "".to_string(),
            system_prompt: "You are a helpful AI assistant. Be concise and direct.".to_string(),
            reversal_threshold: 3,
            window_ms: 600,
            min_displacement: 30,
            cooldown_ms: 2000,
            capture_radius: 350,
            overlay_width: 440,
            overlay_height: 520,
            enable_tools: true,
            allowed_tool_categories: default_tool_categories(),
            always_ask_before_ui_control: false,
            max_tool_iterations: 10,
            auto_dismiss_short_responses: true,
            auto_dismiss_word_threshold: 40,
            auto_dismiss_delay_ms: 5000,
            quick_actions_enabled: true,
            history_max_entries: 50,
            read_allowed_roots: default_read_roots(),
            clean_responses: false,
        }
    }
}

pub fn config_dir() -> std::path::PathBuf {
    let base = dirs::config_dir()
        .or_else(|| dirs::data_dir())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ai-cursor")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}
