use regex::Regex;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntityAction {
    pub label: String,
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DetectedEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub value: String,
    pub actions: Vec<EntityAction>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Code,
    Image,
    Chart,
    Table,
    Ui,
    Url,
    Mixed,
    Unknown,
}

impl ContentType {
    pub fn as_str(&self) -> &str {
        match self {
            ContentType::Text => "text",
            ContentType::Code => "code",
            ContentType::Image => "image",
            ContentType::Chart => "chart",
            ContentType::Table => "table",
            ContentType::Ui => "ui",
            ContentType::Url => "url",
            ContentType::Mixed => "mixed",
            ContentType::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuickAction {
    pub label: String,
    pub prompt: String,
}

static URL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)https?://[^\s<>\x22\{\}\|\\\^\[\]]+").unwrap()
});
static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap()
});
static DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4}|(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]* \d{1,2},? \d{4})\b").unwrap()
});
static FILEPATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:[A-Za-z]:\\[^\s<>\x22\{\}\|\\\^\[\]]+|/(?:[a-zA-Z0-9._\-]+/)+[a-zA-Z0-9._\-]*)").unwrap()
});
static PRICE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[$\x{00A3}\x{20AC}\x{00A5}]\s*\d[\d,.]*|\d[\d,.]*\s*(?:USD|EUR|GBP|JPY)").unwrap()
});
static CODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:fn |def |class |import |#include|var |let |const |function |\bif\s*\()").unwrap()
});

pub fn detect_entity(text: &str) -> Option<DetectedEntity> {
    if text.trim().is_empty() {
        return None;
    }

    if let Some(m) = URL_RE.find(text) {
        let url = m.as_str().to_string();
        return Some(DetectedEntity {
            entity_type: "url".into(),
            value: url.clone(),
            actions: vec![
                EntityAction {
                    label: "Open".into(),
                    tool: "open_url".into(),
                    args: serde_json::json!({"url": url.clone()}),
                },
                EntityAction {
                    label: "Copy".into(),
                    tool: "copy_to_clipboard".into(),
                    args: serde_json::json!({"text": url}),
                },
            ],
        });
    }

    if let Some(m) = EMAIL_RE.find(text) {
        let email = m.as_str().to_string();
        return Some(DetectedEntity {
            entity_type: "email".into(),
            value: email.clone(),
            actions: vec![EntityAction {
                label: "Copy".into(),
                tool: "copy_to_clipboard".into(),
                args: serde_json::json!({"text": email}),
            }],
        });
    }

    if let Some(m) = DATE_RE.find(text) {
        let date = m.as_str().to_string();
        return Some(DetectedEntity {
            entity_type: "date".into(),
            value: date.clone(),
            actions: vec![EntityAction {
                label: "Copy".into(),
                tool: "copy_to_clipboard".into(),
                args: serde_json::json!({"text": date}),
            }],
        });
    }

    if let Some(m) = FILEPATH_RE.find(text) {
        let path = m.as_str().to_string();
        return Some(DetectedEntity {
            entity_type: "file_path".into(),
            value: path.clone(),
            actions: vec![
                EntityAction {
                    label: "Read".into(),
                    tool: "read_file".into(),
                    args: serde_json::json!({"path": path.clone()}),
                },
                EntityAction {
                    label: "Copy path".into(),
                    tool: "copy_to_clipboard".into(),
                    args: serde_json::json!({"text": path}),
                },
            ],
        });
    }

    if let Some(m) = PRICE_RE.find(text) {
        let price = m.as_str().to_string();
        return Some(DetectedEntity {
            entity_type: "price".into(),
            value: price.clone(),
            actions: vec![EntityAction {
                label: "Copy".into(),
                tool: "copy_to_clipboard".into(),
                args: serde_json::json!({"text": price}),
            }],
        });
    }

    None
}

pub fn detect_content_type(text: &str) -> ContentType {
    if text.trim().is_empty() {
        return ContentType::Unknown;
    }
    if URL_RE.is_match(text) {
        return ContentType::Url;
    }
    if CODE_RE.is_match(text) {
        return ContentType::Code;
    }
    // Simple table detection: multiple | chars across multiple lines
    if text.contains('|') && text.lines().filter(|l| l.contains('|')).count() >= 2 {
        return ContentType::Table;
    }
    ContentType::Text
}

pub fn suggested_actions(ct: &ContentType) -> Vec<QuickAction> {
    match ct {
        ContentType::Text => vec![
            QuickAction {
                label: "Summarize".into(),
                prompt: "Summarize the selected text concisely.".into(),
            },
            QuickAction {
                label: "Explain".into(),
                prompt: "Explain the selected text in simple terms.".into(),
            },
            QuickAction {
                label: "Translate".into(),
                prompt: "Translate the selected text to English.".into(),
            },
            QuickAction {
                label: "Simplify".into(),
                prompt: "Rewrite the selected text in simpler language.".into(),
            },
        ],
        ContentType::Code => vec![
            QuickAction {
                label: "Explain".into(),
                prompt: "Explain what this code does step by step.".into(),
            },
            QuickAction {
                label: "Fix bugs".into(),
                prompt: "Find and fix any bugs in this code.".into(),
            },
            QuickAction {
                label: "Add comments".into(),
                prompt: "Add clear comments to this code.".into(),
            },
            QuickAction {
                label: "Convert".into(),
                prompt: "Convert this code to Python.".into(),
            },
        ],
        ContentType::Url => vec![
            QuickAction {
                label: "Open".into(),
                prompt: "Open this URL in the browser.".into(),
            },
            QuickAction {
                label: "Summarize page".into(),
                prompt: "Summarize what this URL/page is about.".into(),
            },
            QuickAction {
                label: "Copy".into(),
                prompt: "Copy this URL to clipboard.".into(),
            },
        ],
        ContentType::Table => vec![
            QuickAction {
                label: "Summarize".into(),
                prompt: "Summarize the data in this table.".into(),
            },
            QuickAction {
                label: "Export as list".into(),
                prompt: "Convert this table to a bullet list.".into(),
            },
            QuickAction {
                label: "Find patterns".into(),
                prompt: "Identify patterns or trends in this table.".into(),
            },
        ],
        ContentType::Image | ContentType::Chart => vec![
            QuickAction {
                label: "Describe".into(),
                prompt: "Describe what you see in this image.".into(),
            },
            QuickAction {
                label: "Extract text".into(),
                prompt: "Extract any text visible in this image.".into(),
            },
        ],
        _ => vec![
            QuickAction {
                label: "Explain".into(),
                prompt: "Explain what you see.".into(),
            },
            QuickAction {
                label: "Summarize".into(),
                prompt: "Summarize this content.".into(),
            },
            QuickAction {
                label: "Ask anything".into(),
                prompt: "What would you like to know?".into(),
            },
        ],
    }
}
