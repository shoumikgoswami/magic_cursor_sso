use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use crate::history::{new_session_id, now_iso, save_session, SessionEntry};

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ChatChunk {
    message: Option<ChunkMessage>,
    done: Option<bool>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    content: String,
}

pub async fn stream_chat(
    app: AppHandle,
    prompt: String,
    model: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    conversation_history: Vec<serde_json::Value>,
    query: String,
    content_type: String,
    context_preview: String,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    let mut messages: Vec<Message> = Vec::new();

    if let Some(sys) = system {
        if !sys.is_empty() {
            messages.push(Message {
                role: "system".to_string(),
                content: sys,
                images: None,
            });
        }
    }

    // Prepend conversation history (follow-up turns)
    let is_follow_up = !conversation_history.is_empty();
    for msg in &conversation_history {
        let role = msg["role"].as_str().unwrap_or("user").to_string();
        let content = msg["content"].as_str().unwrap_or("").to_string();
        messages.push(Message { role, content, images: None });
    }

    // The stream_chat dispatch layer already prepends the context framing note
    // for the first turn (when images are present), so just pass through here.
    let user_content = prompt;

    // Only attach image on the first turn
    let user_images = if is_follow_up { None } else { images };

    messages.push(Message {
        role: "user".to_string(),
        content: user_content,
        images: user_images,
    });

    let body = ChatRequest {
        model,
        messages,
        stream: true,
    };

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let _ = app.emit("ollama-error", format!("HTTP {}: {}", status, text));
        return Err(format!("HTTP {}", status));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full_response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete NDJSON lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Ok(chunk_data) = serde_json::from_str::<ChatChunk>(&line) {
                if let Some(msg) = chunk_data.message {
                    if !msg.content.is_empty() {
                        let _ = app.emit("ollama-chunk", msg.content.clone());
                        full_response.push_str(&msg.content);
                    }
                }
                if chunk_data.done.unwrap_or(false) {
                    let _ = app.emit("ollama-done", ());
                    // Save session
                    save_session(
                        SessionEntry {
                            id: new_session_id(),
                            timestamp: now_iso(),
                            content_type,
                            context_preview,
                            query,
                            response: full_response,
                            tool_calls: vec![],
                        },
                        50, // max_entries — should ideally come from config
                    );
                    return Ok(());
                }
            }
        }
    }

    let _ = app.emit("ollama-done", ());
    // Save session at end of stream
    save_session(
        SessionEntry {
            id: new_session_id(),
            timestamp: now_iso(),
            content_type,
            context_preview,
            query,
            response: full_response,
            tool_calls: vec![],
        },
        50,
    );
    Ok(())
}

pub async fn test_model(model: String) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Hi"}],
        "stream": false,
        "options": { "num_predict": 3 }
    });

    let resp = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Cannot connect to Ollama: {}", e))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
            .unwrap_or(text);
        Err(msg)
    }
}

#[allow(dead_code)]
pub async fn list_models() -> Result<Vec<String>, String> {
    list_models_with_url("http://localhost:11434").await
}

pub async fn list_models_with_url(base_url: &str) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct ModelsResponse {
        models: Vec<ModelInfo>,
    }
    #[derive(Deserialize)]
    struct ModelInfo {
        name: String,
    }

    let base = base_url.trim_end_matches('/');
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: ModelsResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(data.models.into_iter().map(|m| m.name).collect())
}
