#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod context;
mod entity;
mod history;
mod mouse_hook;
mod ollama_client;
mod providers;
mod shake_detector;
mod window_manager;

use config::{load_config, save_config, AppConfig};
use history::SessionEntry;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter as _, Manager as _,
};

// ── App State ─────────────────────────────────────────────────────────────────

pub struct AppState {}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Insert text into the previously-focused window using clipboard + Ctrl+V.
///
/// Strategy:
///   1. Write the text to the clipboard.
///   2. Hide the overlay so the OS can return focus to the previous app.
///   3. Wait 500 ms — enough time for WebView2 to release focus and the
///      previous window to become foreground on Windows.
///   4. Send Ctrl+V (paste) — works universally across all apps.
///   5. Wait briefly, then restore the original clipboard content.
#[tauri::command]
async fn insert_text(app: AppHandle, text: String) -> Result<(), String> {
    // 1. Save old clipboard & put our text in
    let old_clipboard = tokio::task::spawn_blocking(|| {
        use arboard::Clipboard;
        let mut cb = Clipboard::new().ok()?;
        cb.get_text().ok()
    })
    .await
    .unwrap_or(None);

    let text_for_clip = text.clone();
    tokio::task::spawn_blocking(move || {
        use arboard::Clipboard;
        if let Ok(mut cb) = Clipboard::new() {
            let _ = cb.set_text(text_for_clip);
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    // 2. Hide the overlay — OS will now return focus to previous window
    window_manager::hide_overlay(&app);

    // 3. Wait for focus to fully transfer back
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 4. Ctrl+V in the now-focused window
    tokio::task::spawn_blocking(|| {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        enigo.key(Key::Control, Direction::Press).map_err(|e| e.to_string())?;
        enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| e.to_string())?;
        enigo.key(Key::Control, Direction::Release).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // 5. Restore old clipboard after a brief pause (so paste completes first)
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    if let Some(old) = old_clipboard {
        tokio::task::spawn_blocking(move || {
            use arboard::Clipboard;
            if let Ok(mut cb) = Clipboard::new() {
                let _ = cb.set_text(old);
            }
        })
        .await
        .ok();
    }

    Ok(())
}

#[tauri::command]
async fn stream_query(
    app: AppHandle,
    prompt: String,
    model: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    conversation_history: Option<Vec<serde_json::Value>>,
    query: String,
    content_type: String,
    context_preview: String,
) -> Result<(), String> {
    let config = load_config();
    providers::stream_chat(
        app, prompt, model, system, images,
        conversation_history.unwrap_or_default(),
        query, content_type, context_preview,
        &config,
    ).await
}

#[tauri::command]
async fn list_models() -> Result<Vec<String>, String> {
    let config = load_config();
    providers::list_models(&config.provider, &active_api_key(&config), &config.ollama_base_url).await
}

/// Fetch models for an arbitrary provider+key — used by Settings before saving.
#[tauri::command]
async fn list_provider_models(
    provider: String,
    api_key: String,
    base_url: String,
) -> Result<Vec<String>, String> {
    providers::list_models(&provider, &api_key, &base_url).await
}

#[tauri::command]
async fn list_provider_vision_models(
    provider: String,
    api_key: String,
    base_url: String,
) -> Result<Vec<String>, String> {
    providers::list_vision_models(&provider, &api_key, &base_url).await
}

fn active_api_key(config: &config::AppConfig) -> String {
    match config.provider.as_str() {
        "openai"    => config.openai_api_key.clone(),
        "groq"      => config.groq_api_key.clone(),
        "anthropic" => config.anthropic_api_key.clone(),
        _           => String::new(),
    }
}

#[tauri::command]
async fn test_model(model: String) -> Result<(), String> {
    ollama_client::test_model(model).await
}

#[tauri::command]
fn hide_overlay(app: AppHandle) {
    window_manager::hide_overlay(&app);
}

#[tauri::command]
fn resize_overlay(app: AppHandle, height: f64) {
    window_manager::resize_overlay(&app, height);
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.center();
    }
}

#[tauri::command]
fn hide_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn get_config() -> AppConfig {
    load_config()
}

#[tauri::command]
fn save_config_cmd(app: AppHandle, config: AppConfig) -> Result<(), String> {
    save_config(&config)?;
    let _ = app.emit("config-updated", serde_json::json!({ "default_model": config.default_model }));
    Ok(())
}

#[tauri::command]
fn get_history() -> Vec<SessionEntry> {
    history::load_history()
}

#[tauri::command]
fn clear_history() {
    history::clear_history();
}

#[tauri::command]
fn delete_session(id: String) {
    history::delete_session(&id);
}

#[tauri::command]
async fn transcribe_audio(
    audio_base64: String,
    mime_type: String,
) -> Result<String, String> {
    use base64::Engine as _;
    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(&audio_base64)
        .map_err(|e| e.to_string())?;
    let config = load_config();
    providers::transcribe(audio_bytes, mime_type, &config).await
}

// ── Entry Point ───────────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {})
        .invoke_handler(tauri::generate_handler![
            insert_text,
            stream_query,
            list_models,
            list_provider_models,
            list_provider_vision_models,
            test_model,
            hide_overlay,
            resize_overlay,
            show_settings,
            hide_settings,
            get_config,
            save_config_cmd,
            get_history,
            clear_history,
            delete_session,
            transcribe_audio,
        ])
        .setup(|app| {
            let app_config = load_config();

            let settings_item = MenuItemBuilder::new("Settings…").id("settings").build(app)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit = MenuItemBuilder::new("Quit Magic Cursor").id("quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[&settings_item, &separator, &quit])
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Magic Cursor")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("settings") {
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = w.center();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            context::init_clipboard_tracker();
            mouse_hook::start(app.handle().clone(), app_config);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
