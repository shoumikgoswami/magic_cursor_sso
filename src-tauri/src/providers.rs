/// Provider dispatch layer.
///
/// Routes `stream_chat` and `list_models` calls to the right backend:
///   - "ollama"    → local Ollama NDJSON API
///   - "openai"    → OpenAI /v1/chat/completions (SSE)
///   - "groq"      → Groq   /v1/chat/completions (same SSE shape as OpenAI)
///   - "anthropic" → Anthropic /v1/messages (SSE, different shape)
///
/// `agent_loop.rs` calls `request_response()` to get a `reqwest::Response`,
/// then parses lines via `normalise_line()` identically regardless of provider.

use crate::config::AppConfig;
use crate::history::{new_session_id, now_iso, save_session, SessionEntry};
use crate::ollama_client;
use futures_util::StreamExt;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

// ── Public: stream a full chat turn (used by stream_query command) ────────────

pub async fn stream_chat(
    app: AppHandle,
    prompt: String,
    model: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    conversation_history: Vec<Value>,
    query: String,
    content_type: String,
    context_preview: String,
    config: &AppConfig,
) -> Result<(), String> {
    // Apply the user-configured system prompt when the caller doesn't supply one.
    // (The frontend always passes `system: undefined` for regular queries.)
    let effective_system = system.or_else(|| {
        if !config.system_prompt.is_empty() {
            Some(config.system_prompt.clone())
        } else {
            None
        }
    });

    // For cloud providers, resolve which model and images to actually send.
    // Most Groq / many OpenAI models are text-only and reject array content.
    // Rules:
    //  • images present + vision_model configured  → use vision_model
    //  • images present + no vision_model          → strip images (silent text-only)
    //  • no images                                 → use model as-is
    let (effective_model, effective_images) = resolve_vision(
        model, images, &config.vision_model, &config.provider,
    );

    // When a screenshot is attached on the first turn, prepend a brief framing
    // note so the AI treats the image as background context — NOT as the subject
    // of the response. The user's actual query (text or voice) always takes priority.
    let effective_prompt = if !conversation_history.is_empty() || effective_images.is_none() {
        prompt
    } else {
        format!(
            "[SCREEN CONTEXT: A screenshot of the user's screen is attached for background \
             context only. Respond directly to the user's message below. Only reference the \
             screen if the user's message is specifically about what is shown on screen.]\n\n{prompt}"
        )
    };

    match config.provider.as_str() {
        "openai" => {
            stream_openai_compat(
                app, effective_prompt, effective_model, effective_system, effective_images, conversation_history,
                query, content_type, context_preview,
                &config.openai_api_key,
                "https://api.openai.com/v1/chat/completions",
                config.history_max_entries,
            ).await
        }
        "groq" => {
            stream_openai_compat(
                app, effective_prompt, effective_model, effective_system, effective_images, conversation_history,
                query, content_type, context_preview,
                &config.groq_api_key,
                "https://api.groq.com/openai/v1/chat/completions",
                config.history_max_entries,
            ).await
        }
        "anthropic" => {
            stream_anthropic(
                app, effective_prompt, effective_model, effective_system, effective_images, conversation_history,
                query, content_type, context_preview,
                &config.anthropic_api_key,
                config.history_max_entries,
            ).await
        }
        // "ollama" or anything else → existing Ollama client
        _ => {
            ollama_client::stream_chat(
                app, effective_prompt, effective_model, effective_system, effective_images, conversation_history,
                query, content_type, context_preview,
            ).await
        }
    }
}

/// Choose the right model and image list for a cloud query.
///
/// - If images are present AND a dedicated vision_model is configured → use vision_model.
/// - If images are present but no vision_model → strip images so text-only models don't
///   receive array content and return a cryptic "content must be a string" error.
/// - Ollama handles its own vision routing; pass through unchanged.
/// Remove the `images` key from every message in the array.
/// Used before sending to text-only OpenAI/Groq models so they receive
/// plain string content instead of a multi-part array.
fn drop_image_fields(messages: &[Value]) -> Vec<Value> {
    messages.iter().map(|m| {
        let mut msg = m.clone();
        if let Some(obj) = msg.as_object_mut() {
            obj.remove("images");
        }
        msg
    }).collect()
}

fn resolve_vision(
    model: String,
    images: Option<Vec<String>>,
    vision_model: &str,
    provider: &str,
) -> (String, Option<Vec<String>>) {
    // Ollama does its own model routing — leave it alone
    if provider == "ollama" {
        return (model, images);
    }
    let has_images = images.as_ref().map_or(false, |v| !v.is_empty());
    if !has_images {
        return (model, images);
    }
    // Images present: use vision_model if configured, else strip images
    if !vision_model.is_empty() {
        (vision_model.to_string(), images)
    } else {
        (model, None)
    }
}

// ── Public: fetch available models for a provider (used by list_provider_models) ─

pub async fn list_models(
    provider: &str,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, String> {
    match provider {
        "openai" => list_openai_models(api_key, "https://api.openai.com/v1/models").await,
        "groq"   => list_openai_models(api_key, "https://api.groq.com/openai/v1/models").await,
        "anthropic" => Ok(anthropic_model_list()),
        // "ollama" or unknown
        _ => ollama_client::list_models_with_url(base_url).await,
    }
}

/// Return only the subset of models that are known to support vision (image input).
///
/// - OpenAI: gpt-4o*, gpt-4-turbo*, gpt-4-vision*, o1 (not mini/nano), chatgpt-4o*
/// - Groq:   llama-4*, llama-3.2-*-vision*, llava* (explicitly vision-capable family)
/// - Anthropic: every model in the list supports vision
/// - Ollama: filter installed models by common vision model name patterns
pub async fn list_vision_models(
    provider: &str,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let all = list_models(provider, api_key, base_url).await?;
    let vision: Vec<String> = match provider {
        "openai" => all.into_iter().filter(|m| is_openai_vision(m)).collect(),
        "groq"   => all.into_iter().filter(|m| is_groq_vision(m)).collect(),
        "anthropic" => all, // every Anthropic model supports vision
        _ => all.into_iter().filter(|m| is_ollama_vision(m)).collect(),
    };
    Ok(vision)
}

fn is_openai_vision(id: &str) -> bool {
    let id = id.to_lowercase();
    // gpt-4o family (all tiers), gpt-4-turbo, gpt-4-vision, chatgpt-4o
    // o1 full — NOT o1-mini or o1-nano (no vision in mini/nano)
    id.starts_with("gpt-4o")
        || id.starts_with("chatgpt-4o")
        || id.starts_with("gpt-4-turbo")
        || id.contains("gpt-4-vision")
        || id == "o1"
        || id.starts_with("o1-preview")
        || id.starts_with("o1-202")  // dated releases like o1-2024-12-17
        || id.starts_with("o3")      // o3 and o4 support vision
        || id.starts_with("o4")
}

fn is_groq_vision(id: &str) -> bool {
    let id = id.to_lowercase();
    // Llama 4 Scout/Maverick are multimodal; llama-3.2-*b-vision; llava family
    id.contains("llama-4")
        || id.contains("llama3.2")     // llama3.2 on Groq is the vision variant
        || id.contains("llama-3.2")
        || id.contains("vision")
        || id.contains("llava")
}

fn is_ollama_vision(id: &str) -> bool {
    let id = id.to_lowercase();
    // Common Ollama vision model families
    id.contains("llava")
        || id.contains("vision")
        || id.contains("minicpm-v")
        || id.contains("moondream")
        || id.contains("bakllava")
        || id.contains("cogvlm")
        || id.contains("llama3.2")  // llama3.2 has vision variants on Ollama
        || id.contains("llama-3.2")
        || id.contains("llama4")
        || id.contains("llama-4")
        || id.contains("gemma3")    // gemma3 is multimodal
        || id.contains("qwen2-vl")
        || id.contains("qwen2.5vl")
        || id.contains("internvl")
        || id.contains("phi3-vision")
        || id.contains("phi-3-vision")
}

// ── Public: make one streaming request for agent_loop ────────────────────────
//
// Returns a stream of normalised NDJSON lines in Ollama's format:
//   {"message":{"content":"token"},"done":false}
//   {"done":true}
// agent_loop.rs can parse these identically regardless of provider.

/// Make one streaming request for the agent loop and return the raw Response.
/// The caller calls `.bytes_stream()` on it and parses lines via `normalise_line`.
pub async fn request_response(
    client: &reqwest::Client,
    messages: &[Value],
    config: &AppConfig,
) -> Result<reqwest::Response, String> {
    let resp = match config.provider.as_str() {
        "openai" => {
            // Strip images from messages when the model is text-only
            let msgs = if is_openai_vision(&config.default_model) {
                messages.to_vec()
            } else {
                drop_image_fields(messages)
            };
            let body = serde_json::json!({
                "model": config.default_model,
                "messages": openai_messages(&msgs, false),
                "stream": true,
            });
            client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&config.openai_api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Connection error: {e}"))?
        }
        "groq" => {
            // Strip images from messages when the model is text-only
            let msgs = if is_groq_vision(&config.default_model) {
                messages.to_vec()
            } else {
                drop_image_fields(messages)
            };
            let body = serde_json::json!({
                "model": config.default_model,
                "messages": openai_messages(&msgs, false),
                "stream": true,
            });
            client
                .post("https://api.groq.com/openai/v1/chat/completions")
                .bearer_auth(&config.groq_api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Connection error: {e}"))?
        }
        "anthropic" => {
            let (system_text, user_messages) = split_anthropic_messages(messages);
            let mut body = serde_json::json!({
                "model": config.default_model,
                "messages": user_messages,
                "max_tokens": 8192,
                "stream": true,
            });
            if let Some(sys) = system_text {
                body["system"] = Value::String(sys);
            }
            client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &config.anthropic_api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Connection error: {e}"))?
        }
        _ => {
            let base = config.ollama_base_url.trim_end_matches('/');
            let body = serde_json::json!({
                "model": config.default_model,
                "messages": messages,
                "stream": true,
            });
            client
                .post(format!("{base}/api/chat"))
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Connection error: {e}"))?
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {text}"));
    }

    Ok(resp)
}

/// Parse one raw SSE/NDJSON line from the provider's stream into normalised
/// Ollama-shaped JSON, or return `None` to skip the line.
pub fn normalise_line(line: &str, provider: &str) -> Option<Value> {
    let line = line.trim();
    if line.is_empty() { return None; }

    match provider {
        "openai" | "groq" => {
            // SSE format: "data: {...}" or "data: [DONE]"
            let data = line.strip_prefix("data: ")?;
            if data == "[DONE]" {
                return Some(serde_json::json!({"done": true}));
            }
            let v: Value = serde_json::from_str(data).ok()?;
            let token = v["choices"][0]["delta"]["content"].as_str()?;
            if token.is_empty() { return None; }
            Some(serde_json::json!({"message":{"content": token},"done":false}))
        }
        "anthropic" => {
            // SSE format: "data: {...}"
            let data = line.strip_prefix("data: ")?;
            let v: Value = serde_json::from_str(data).ok()?;
            match v["type"].as_str()? {
                "content_block_delta" => {
                    let token = v["delta"]["text"].as_str()?;
                    if token.is_empty() { return None; }
                    Some(serde_json::json!({"message":{"content": token},"done":false}))
                }
                "message_stop" => Some(serde_json::json!({"done": true})),
                _ => None,
            }
        }
        // Ollama: already NDJSON, pass through
        _ => serde_json::from_str(line).ok(),
    }
}

// ── OpenAI-compatible streaming (OpenAI + Groq) ───────────────────────────────

async fn stream_openai_compat(
    app: AppHandle,
    prompt: String,
    model: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    conversation_history: Vec<Value>,
    query: String,
    content_type: String,
    context_preview: String,
    api_key: &str,
    url: &str,
    max_entries: usize,
) -> Result<(), String> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(sys) = system.filter(|s| !s.is_empty()) {
        messages.push(serde_json::json!({"role":"system","content":sys}));
    }

    let is_follow_up = !conversation_history.is_empty();
    for msg in &conversation_history {
        // Re-encode as plain text for follow-ups (no images in history)
        messages.push(serde_json::json!({
            "role": msg["role"],
            "content": msg["content"],
        }));
    }

    let user_content: Value = if !is_follow_up {
        let mut parts: Vec<Value> = Vec::new();
        if let Some(ref imgs) = images {
            for b64 in imgs {
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:image/png;base64,{b64}") }
                }));
            }
        }
        parts.push(serde_json::json!({"type":"text","text":prompt}));
        if parts.len() == 1 {
            Value::String(prompt)
        } else {
            Value::Array(parts)
        }
    } else {
        Value::String(prompt)
    };

    messages.push(serde_json::json!({"role":"user","content":user_content}));

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("HTTP {status}: {text}"));
        let _ = app.emit("ollama-error", &msg);
        return Err(msg);
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut full_response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(nl) = buffer.find('\n') {
            let line = buffer[..nl].trim().to_string();
            buffer = buffer[nl + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    let _ = app.emit("ollama-done", ());
                    save_session(SessionEntry {
                        id: new_session_id(),
                        timestamp: now_iso(),
                        content_type,
                        context_preview,
                        query,
                        response: full_response,
                        tool_calls: vec![],
                    }, max_entries);
                    return Ok(());
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(token) = v["choices"][0]["delta"]["content"].as_str() {
                        if !token.is_empty() {
                            let _ = app.emit("ollama-chunk", token);
                            full_response.push_str(token);
                        }
                    }
                }
            }
        }
    }

    let _ = app.emit("ollama-done", ());
    save_session(SessionEntry {
        id: new_session_id(), timestamp: now_iso(),
        content_type, context_preview, query, response: full_response, tool_calls: vec![],
    }, max_entries);
    Ok(())
}

// ── Anthropic streaming ───────────────────────────────────────────────────────

async fn stream_anthropic(
    app: AppHandle,
    prompt: String,
    model: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    conversation_history: Vec<Value>,
    query: String,
    content_type: String,
    context_preview: String,
    api_key: &str,
    max_entries: usize,
) -> Result<(), String> {
    let is_follow_up = !conversation_history.is_empty();
    let mut messages: Vec<Value> = Vec::new();

    for msg in &conversation_history {
        messages.push(serde_json::json!({
            "role": msg["role"],
            "content": msg["content"],
        }));
    }

    // Build user content with optional image
    let user_content: Value = if !is_follow_up {
        let mut parts: Vec<Value> = Vec::new();
        if let Some(ref imgs) = images {
            for b64 in imgs {
                parts.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": b64,
                    }
                }));
            }
        }
        parts.push(serde_json::json!({"type":"text","text":prompt}));
        Value::Array(parts)
    } else {
        Value::Array(vec![serde_json::json!({"type":"text","text":prompt})])
    };

    messages.push(serde_json::json!({"role":"user","content":user_content}));

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": 8192,
        "stream": true,
    });
    if let Some(sys) = system.filter(|s| !s.is_empty()) {
        body["system"] = Value::String(sys);
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("HTTP {status}: {text}"));
        let _ = app.emit("ollama-error", &msg);
        return Err(msg);
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut full_response = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(nl) = buffer.find('\n') {
            let line = buffer[..nl].trim().to_string();
            buffer = buffer[nl + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    match v["type"].as_str() {
                        Some("content_block_delta") => {
                            if let Some(token) = v["delta"]["text"].as_str() {
                                if !token.is_empty() {
                                    let _ = app.emit("ollama-chunk", token);
                                    full_response.push_str(token);
                                }
                            }
                        }
                        Some("message_stop") => {
                            let _ = app.emit("ollama-done", ());
                            save_session(SessionEntry {
                                id: new_session_id(), timestamp: now_iso(),
                                content_type, context_preview, query,
                                response: full_response, tool_calls: vec![],
                            }, max_entries);
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let _ = app.emit("ollama-done", ());
    save_session(SessionEntry {
        id: new_session_id(), timestamp: now_iso(),
        content_type, context_preview, query, response: full_response, tool_calls: vec![],
    }, max_entries);
    Ok(())
}

// ── Model listing ─────────────────────────────────────────────────────────────

async fn list_openai_models(api_key: &str, url: &str) -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct ModelsResp { data: Vec<ModelEntry> }
    #[derive(serde::Deserialize)]
    struct ModelEntry { id: String }

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or(text);
        return Err(msg);
    }

    let data: ModelsResp = resp.json().await.map_err(|e| e.to_string())?;
    let mut ids: Vec<String> = data.data.into_iter().map(|m| m.id).collect();

    // For OpenAI, keep only chat-capable models and sort them sensibly
    if url.contains("openai.com") {
        ids.retain(|id| {
            id.starts_with("gpt-") || id.starts_with("o1") || id.starts_with("o3")
            || id.starts_with("chatgpt-")
        });
        ids.sort_by(|a, b| b.cmp(a)); // newest first
    } else {
        ids.sort();
    }

    Ok(ids)
}

fn anthropic_model_list() -> Vec<String> {
    vec![
        "claude-opus-4-5".to_string(),
        "claude-sonnet-4-5".to_string(),
        "claude-haiku-4-5".to_string(),
        "claude-3-5-sonnet-20241022".to_string(),
        "claude-3-5-haiku-20241022".to_string(),
        "claude-3-opus-20240229".to_string(),
        "claude-3-sonnet-20240229".to_string(),
        "claude-3-haiku-20240307".to_string(),
    ]
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert Ollama-style messages (with optional top-level `images` field) into
/// OpenAI multi-part content blocks.
fn openai_messages(messages: &[Value], _strip_images: bool) -> Vec<Value> {
    messages.iter().map(|m| {
        let role = m["role"].as_str().unwrap_or("user");
        // If images are attached, build multi-part content
        if let Some(imgs) = m["images"].as_array() {
            let mut parts: Vec<Value> = imgs.iter().map(|b64| serde_json::json!({
                "type": "image_url",
                "image_url": { "url": format!("data:image/png;base64,{}", b64.as_str().unwrap_or("")) }
            })).collect();
            parts.push(serde_json::json!({"type":"text","text": m["content"]}));
            serde_json::json!({"role": role, "content": parts})
        } else {
            serde_json::json!({"role": role, "content": m["content"]})
        }
    }).collect()
}

// ── Speech-to-Text (Whisper) ──────────────────────────────────────────────────

/// Transcribe raw audio bytes using the configured provider's Whisper endpoint.
/// Returns the transcript string on success, or `Err("no-stt-provider")` when
/// the active provider has no STT endpoint (Anthropic, Ollama) so the frontend
/// can fall back to the browser Web Speech API.
pub async fn transcribe(
    audio_bytes: Vec<u8>,
    mime_type: String,
    config: &AppConfig,
) -> Result<String, String> {
    match config.provider.as_str() {
        "openai" => {
            let model = if config.stt_model.is_empty() { "whisper-1" } else { &config.stt_model };
            transcribe_openai_compat(
                audio_bytes, mime_type,
                "https://api.openai.com/v1/audio/transcriptions",
                &config.openai_api_key,
                model,
            ).await
        },
        "groq" => {
            let model = if config.stt_model.is_empty() { "whisper-large-v3-turbo" } else { &config.stt_model };
            transcribe_openai_compat(
                audio_bytes, mime_type,
                "https://api.groq.com/openai/v1/audio/transcriptions",
                &config.groq_api_key,
                model,
            ).await
        },
        // Anthropic and Ollama don't offer STT — signal the frontend to fall back
        _ => Err("no-stt-provider".to_string()),
    }
}

async fn transcribe_openai_compat(
    audio_bytes: Vec<u8>,
    mime_type: String,
    url: &str,
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    let ext = if mime_type.contains("webm") { "webm" }
              else if mime_type.contains("mp4") || mime_type.contains("m4a") { "m4a" }
              else { "wav" };

    let part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name(format!("audio.{}", ext))
        .mime_str(&mime_type)
        .map_err(|e| e.to_string())?;

    let form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "text")
        .part("file", part);

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("STT HTTP {}: {}", status, body));
    }

    // response_format=text returns plain text (not JSON)
    resp.text().await.map(|t| t.trim().to_string()).map_err(|e| e.to_string())
}

/// Split an Ollama-style message array into (system_text, user/assistant messages)
/// for Anthropic's API which takes `system` as a top-level field.
fn split_anthropic_messages(messages: &[Value]) -> (Option<String>, Vec<Value>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut out: Vec<Value> = Vec::new();

    for m in messages {
        if m["role"].as_str() == Some("system") {
            if let Some(s) = m["content"].as_str() {
                system_parts.push(s.to_string());
            }
        } else {
            let role = m["role"].as_str().unwrap_or("user");
            // Build Anthropic content blocks
            if let Some(imgs) = m["images"].as_array() {
                let mut parts: Vec<Value> = imgs.iter().map(|b64| serde_json::json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": "image/png", "data": b64 }
                })).collect();
                parts.push(serde_json::json!({"type":"text","text": m["content"]}));
                out.push(serde_json::json!({"role": role, "content": parts}));
            } else {
                out.push(serde_json::json!({
                    "role": role,
                    "content": [{"type":"text","text": m["content"]}]
                }));
            }
        }
    }

    let system = if system_parts.is_empty() { None } else { Some(system_parts.join("\n\n")) };
    (system, out)
}
