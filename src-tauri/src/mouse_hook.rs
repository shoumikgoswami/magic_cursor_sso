use crate::{config::AppConfig, context, entity, shake_detector::{ShakeConfig, ShakeDetector}, window_manager};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn start(app: AppHandle, config: AppConfig) {
    // Keep a second handle for error reporting outside the rdev closure.
    let app_for_err = app.clone();

    std::thread::spawn(move || {
        let shake_config = ShakeConfig {
            reversal_threshold: config.reversal_threshold,
            window: Duration::from_millis(config.window_ms),
            min_displacement: config.min_displacement,
            cooldown: Duration::from_millis(config.cooldown_ms),
        };
        let mut detector = ShakeDetector::new(shake_config);
        let capture_radius = config.capture_radius;

        // Monotonically increasing shake generation counter.
        // Each shake bumps this; context-ready threads read back the current
        // value at emit time and drop the event if a newer shake already fired.
        let generation = Arc::new(AtomicU64::new(0));

        // ── macOS: check Accessibility permission before starting ─────────────
        // rdev uses CGEventTap which requires the app to be listed under
        // System Settings → Privacy & Security → Accessibility.
        // We probe this early so the user sees a helpful message immediately
        // rather than silently getting no shake detection.
        #[cfg(target_os = "macos")]
        check_and_request_accessibility(&app_for_err);

        // ── Start the global mouse listener ────────────────────────────────────
        // IMPORTANT: On macOS the rdev callback runs on the CGEventTap thread,
        // which is a real-time OS thread with a strict return-time budget.
        // Doing ANY blocking work here (subprocess, Accessibility API, sleep)
        // causes macOS to disable the tap and can crash the process with
        // EXC_BREAKPOINT / SIGTRAP.
        //
        // Rule: the callback does ONLY position reading + thread spawn.
        // Everything else (window title, trigger_copy, screenshot, context)
        // happens inside the spawned thread.
        if let Err(e) = rdev::listen(move |event| {
            if let rdev::EventType::MouseMove { x, y } = event.event_type {
                let xi = x as i32;
                let yi = y as i32;
                if detector.process(xi, yi) {
                    let app_clone = app.clone();
                    let gen_counter = Arc::clone(&generation);

                    // Bump the generation counter for this shake.
                    let my_gen = gen_counter.fetch_add(1, Ordering::SeqCst) + 1;

                    // Spawn immediately — return the CGEventTap callback ASAP.
                    std::thread::spawn(move || {
                        // 1. Capture the active window title before the overlay
                        //    steals focus (still works because this thread starts
                        //    executing before the overlay window becomes visible).
                        let window_title = context::get_foreground_window_title();

                        // 2. Send Cmd+C / Ctrl+C to copy the current selection into
                        //    the clipboard while the original app still has focus.
                        context::trigger_copy();

                        // 3. Show the overlay and notify the frontend.
                        window_manager::show_overlay(&app_clone, xi, yi);
                        let _ = app_clone.emit("shake-detected", serde_json::json!({
                            "x": xi, "y": yi, "gen": my_gen
                        }));

                        // 4. Take the screenshot AFTER the overlay is visible so the
                        //    captured image shows the app context the user is working
                        //    in, with the overlay positioned over it as they see it.
                        let screenshot = context::screenshot_now(xi, yi, capture_radius);

                        // 5. Finish context capture (clipboard settle + app context).
                        let ctx = context::capture(window_title, screenshot);

                        // 6. Drop this result if a newer shake has already fired.
                        //    Prevents a slow old thread from overwriting fresh context.
                        if gen_counter.load(Ordering::SeqCst) != my_gen {
                            return;
                        }

                        let has_image = ctx.screenshot_b64.is_some();
                        let selected = ctx.selected_text.as_deref().unwrap_or("");
                        let detected_entity = entity::detect_entity(selected);
                        let content_type = entity::detect_content_type(selected);
                        let quick_actions = entity::suggested_actions(&content_type);

                        let _ = app_clone.emit(
                            "context-ready",
                            serde_json::json!({
                                "selected_text": ctx.selected_text,
                                "has_image": has_image,
                                "screenshot_b64": ctx.screenshot_b64,
                                "window_title": ctx.window_title,
                                "app_context": ctx.app_context,
                                "content_type": content_type.as_str(),
                                "quick_actions": quick_actions,
                                "entity": detected_entity,
                            }),
                        );
                    });
                }
            }
        }) {
            let msg = format!("{:?}", e);
            eprintln!("[AI Cursor] Mouse listener failed to start: {}", msg);
            // On macOS the most common cause is missing Accessibility permission.
            // Emit the same event the pre-check emits so the frontend shows the banner.
            #[cfg(target_os = "macos")]
            let _ = app_for_err.emit("macos-accessibility-needed", &msg);
            #[cfg(not(target_os = "macos"))]
            let _ = app_for_err.emit("shake-listener-error", &msg);
        }
    });
}

/// Check whether this process has macOS Accessibility permission and, if not,
/// emit an event to the frontend and open the System Settings pane.
///
/// We probe by asking System Events to list process names — this succeeds only
/// when Accessibility (or Input Monitoring) is granted.
#[cfg(target_os = "macos")]
fn check_and_request_accessibility(app: &AppHandle) {
    use std::process::Command;

    let trusted = Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "System Events" to name of every process"#)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !trusted {
        // Tell the frontend so it can show a persistent banner.
        let _ = app.emit("macos-accessibility-needed", "");

        // Open the Accessibility pane directly so the user doesn't have to hunt.
        // The URL scheme works on macOS 13+ (Ventura); older versions fall back
        // to System Preferences via the same `open` command.
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .ok();
    }
}
