use base64::{engine::general_purpose, Engine as _};
use image::ImageFormat;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Mutex;

// ── Clipboard freshness tracker ───────────────────────────────────────────────
//
// We only inject clipboard content when it actually changed since the last
// time we saw it. This prevents stale content copied hours ago from always
// appearing as context.
//
// Call `init_clipboard_tracker()` once on startup to prime the tracker with
// whatever is currently in the clipboard so it is treated as "old".

static LAST_CLIPBOARD: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

/// Prime the tracker with the current clipboard content on app startup.
/// Any content already in the clipboard before the app launched is treated
/// as "old" and will not be injected on the first shake.
pub fn init_clipboard_tracker() {
    let current = arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .unwrap_or_default();
    if let Ok(mut guard) = LAST_CLIPBOARD.lock() {
        *guard = current.trim().to_string();
    }
}

/// Structured context gathered from the active app at shake time.
/// Fields are `None` when the information isn't available for the current app.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppContext {
    /// Human-readable application name, e.g. "Google Chrome", "Finder", "VS Code".
    pub app_name: Option<String>,
    /// Current browser tab URL (browsers only).
    pub browser_url: Option<String>,
    /// Current directory path shown in the file manager (Finder / File Explorer only).
    pub file_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CapturedContext {
    pub selected_text: Option<String>,
    pub screenshot_b64: Option<String>,
    pub window_title: Option<String>,
    /// Structured app context gathered from the OS at shake time.
    pub app_context: Option<AppContext>,
}


/// Grab the foreground window title from the OS **before** the overlay steals focus.
/// Call this synchronously on the rdev thread, before show_overlay().
pub fn get_foreground_window_title() -> Option<String> {
    #[cfg(windows)]
    {
        use winapi::um::winuser::{GetForegroundWindow, GetWindowTextW};
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() { return None; }
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
            if len <= 0 { return None; }
            let title = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
            if title.is_empty() { None } else { Some(title) }
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // Ask System Events for the frontmost app's window title.
        // Falls back to just the app name if Accessibility isn't granted.
        let script = r#"tell application "System Events"
            set frontProc to first application process whose frontmost is true
            set appName to name of frontProc
            try
                set winTitle to name of front window of frontProc
                if winTitle is not "" then
                    return winTitle & " - " & appName
                end if
            end try
            return appName
        end tell"#;
        let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    { None }
}

/// Send the OS copy keystroke to the currently focused window so that any
/// selected text or files land in the clipboard before we read it.
/// Must be called BEFORE the overlay steals focus (i.e. on the rdev thread).
pub fn trigger_copy() {
    #[cfg(windows)]
    {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        if let Ok(mut e) = Enigo::new(&Settings::default()) {
            let _ = e.key(Key::Control, Direction::Press);
            let _ = e.key(Key::Unicode('c'), Direction::Click);
            let _ = e.key(Key::Control, Direction::Release);
        }
    }
    #[cfg(target_os = "macos")]
    {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        if let Ok(mut e) = Enigo::new(&Settings::default()) {
            let _ = e.key(Key::Meta, Direction::Press);
            let _ = e.key(Key::Unicode('c'), Direction::Click);
            let _ = e.key(Key::Meta, Direction::Release);
        }
    }
}

/// Take a screenshot of the area around (x, y) immediately — before the overlay
/// is shown. Call this as early as possible in the shake handler so the captured
/// image shows the underlying app, not the overlay window.
pub fn screenshot_now(x: i32, y: i32, radius: u32) -> Option<String> {
    capture_screenshot(x, y, radius)
}

/// Finish context capture (clipboard text + app context) using a screenshot that
/// was already taken before the overlay appeared. Accepts the pre-captured image
/// so the caller controls the exact moment the screenshot fires.
pub fn capture(
    window_title: Option<String>,
    pre_screenshot: Option<String>,
) -> CapturedContext {
    // Give the OS time to finish updating the clipboard from the trigger_copy()
    // keystroke that was sent just before this is called (~150 ms is enough
    // for all apps we've tested).
    std::thread::sleep(std::time::Duration::from_millis(150));

    let selected_text = capture_selected_text();
    // Gather structured app context (app name, browser URL, file path).
    // The 500 ms timeout inside gather_app_context keeps it from blocking.
    let app_context = gather_app_context(window_title.as_deref());

    CapturedContext {
        selected_text,
        screenshot_b64: pre_screenshot,
        window_title,
        app_context,
    }
}

// ── Structured app context ────────────────────────────────────────────────────

/// Query the OS for the active app name, browser URL, and/or file manager path.
/// Returns `None` if nothing useful can be determined.
/// Keeps a hard 500 ms wall-clock budget so it never delays the overlay.
fn gather_app_context(window_title: Option<&str>) -> Option<AppContext> {
    let title = window_title.unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }

    #[cfg(target_os = "macos")]
    return gather_app_context_macos(title);

    #[cfg(windows)]
    return gather_app_context_windows(title);

    #[cfg(not(any(target_os = "macos", windows)))]
    return Some(AppContext {
        app_name: Some(title.to_string()),
        ..Default::default()
    });
}

/// macOS implementation — uses AppleScript (osascript) with a timeout.
/// Each call is wrapped in `timeout 0.5` so it never hangs.
#[cfg(target_os = "macos")]
fn gather_app_context_macos(window_title: &str) -> Option<AppContext> {
    use std::process::Command;

    // Identify the app from the window title.
    // File Explorer equivalent on macOS is "Finder".
    // Browsers append their name after " - " or " — ".
    let (app_name, kind) = detect_macos_app(window_title);

    let mut ctx = AppContext {
        app_name: Some(app_name.clone()),
        ..Default::default()
    };

    match kind {
        MacOSAppKind::Finder => {
            // Ask Finder for the POSIX path of the front window.
            let script = r#"tell application "Finder"
                try
                    return POSIX path of (target of front window as alias)
                end try
                return ""
            end tell"#;
            if let Some(path) = run_osascript(script) {
                if !path.is_empty() {
                    ctx.file_path = Some(path);
                }
            }
        }
        MacOSAppKind::Browser(ref browser) => {
            // Ask the browser for the URL of the active tab.
            let script = browser_url_script(browser);
            if let Some(url) = run_osascript(&script) {
                if !url.is_empty() && url.starts_with("http") {
                    ctx.browser_url = Some(url);
                }
            }
        }
        MacOSAppKind::Other => {}
    }

    Some(ctx)
}

#[cfg(target_os = "macos")]
enum MacOSAppKind {
    Finder,
    Browser(String),
    Other,
}

#[cfg(target_os = "macos")]
fn detect_macos_app(window_title: &str) -> (String, MacOSAppKind) {
    // Browsers append their name: "Page title - Google Chrome"
    const BROWSERS: &[(&str, &str)] = &[
        ("Google Chrome",   "Google Chrome"),
        ("Chromium",        "Chromium"),
        ("Firefox",         "Firefox"),
        ("Safari",          "Safari"),
        ("Microsoft Edge",  "Microsoft Edge"),
        ("Brave Browser",   "Brave Browser"),
        ("Arc",             "Arc"),
        ("Opera",           "Opera"),
    ];
    for (suffix, name) in BROWSERS {
        if window_title.ends_with(suffix)
            || window_title.contains(&format!(" - {}", suffix))
            || window_title.contains(&format!(" — {}", suffix))
        {
            return (name.to_string(), MacOSAppKind::Browser(name.to_string()));
        }
    }
    // Finder windows: just the folder name (e.g. "Documents", "Downloads")
    // We detect by asking System Events for the frontmost app name later,
    // but a simpler heuristic: if window_title has no " - " separator it
    // might be Finder. We'll just try both.
    // Actually check the title for "Finder" directly.
    if window_title.ends_with("Finder") || window_title == "Finder" {
        return ("Finder".to_string(), MacOSAppKind::Finder);
    }
    // For all other apps, extract name from the title (last segment after " - ").
    let app_name = window_title
        .split(" - ")
        .last()
        .unwrap_or(window_title)
        .trim()
        .to_string();
    // Heuristic: a short last segment without spaces is probably the app name.
    // For Finder the window title IS the folder name, so we use a second probe.
    (app_name, MacOSAppKind::Other)
}

/// Returns the AppleScript to get the URL of the active tab for a given browser.
#[cfg(target_os = "macos")]
fn browser_url_script(browser: &str) -> String {
    match browser {
        "Firefox" => r#"tell application "Firefox"
            try
                return URL of active tab of front window
            end try
            return ""
        end tell"#.to_string(),
        "Safari" => r#"tell application "Safari"
            try
                return URL of current tab of front window
            end try
            return ""
        end tell"#.to_string(),
        "Arc" => r#"tell application "Arc"
            try
                return URL of active tab of front window
            end try
            return ""
        end tell"#.to_string(),
        // Chrome, Edge, Brave, Opera, Chromium all use the same AppleScript API
        _ => format!(r#"tell application "{browser}"
            try
                return URL of active tab of front window
            end try
            return ""
        end tell"#, browser = browser),
    }
}

/// Run an osascript snippet with a hard 2-second timeout.
/// Returns trimmed stdout, or `None` on error / timeout.
#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    use std::process::Command;
    // `timeout 2` kills osascript if it hangs (e.g. app is unresponsive).
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Windows implementation — uses PowerShell for File Explorer path.
#[cfg(windows)]
fn gather_app_context_windows(window_title: &str) -> Option<AppContext> {
    // Detect browser vs File Explorer from the window title.
    let (app_name, kind) = detect_windows_app(window_title);

    let mut ctx = AppContext {
        app_name: Some(app_name),
        ..Default::default()
    };

    match kind {
        WindowsAppKind::FileExplorer => {
            // Use PowerShell's Shell.Application COM object to get the current folder.
            if let Some(path) = get_explorer_path_windows() {
                ctx.file_path = Some(path);
            }
        }
        WindowsAppKind::Browser => {
            // On Windows, the URL is not easily accessible without UI Automation.
            // The window title often contains the page name but not the URL.
            // Leave browser_url empty; the screenshot shows the address bar.
        }
        WindowsAppKind::Other => {}
    }

    Some(ctx)
}

#[cfg(windows)]
enum WindowsAppKind {
    FileExplorer,
    Browser,
    Other,
}

#[cfg(windows)]
fn detect_windows_app(window_title: &str) -> (String, WindowsAppKind) {
    // File Explorer windows end with " - File Explorer" or just "File Explorer"
    if window_title.ends_with("File Explorer") || window_title == "File Explorer" {
        return ("File Explorer".to_string(), WindowsAppKind::FileExplorer);
    }
    // Browsers
    const BROWSERS: &[&str] = &[
        "Google Chrome", "Mozilla Firefox", "Microsoft Edge",
        "Brave", "Opera", "Arc",
    ];
    for browser in BROWSERS {
        if window_title.ends_with(browser)
            || window_title.contains(&format!(" - {}", browser))
            || window_title.contains(&format!(" — {}", browser))
        {
            return (browser.to_string(), WindowsAppKind::Browser);
        }
    }
    // Generic: extract last segment after " - "
    let name = window_title.split(" - ").last().unwrap_or(window_title).trim().to_string();
    (name, WindowsAppKind::Other)
}

/// Query the frontmost File Explorer window's path via PowerShell.
/// CREATE_NO_WINDOW (0x08000000) prevents a console window from flashing
/// on screen — critical for a tray-only app with no console of its own.
#[cfg(windows)]
fn get_explorer_path_windows() -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script = "\
$shell = New-Object -ComObject Shell.Application; \
$win = $shell.Windows() | Where-Object { $_.Name -eq 'File Explorer' } | Select-Object -First 1; \
if ($win) { $win.Document.Folder.Self.Path } else { '' }";

    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn capture_selected_text() -> Option<String> {
    // Read current clipboard. Users copy text first, then shake to query it.
    let text = arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())?;

    let trimmed = text.trim();

    // Only inject clipboard content if it changed since we last saw it.
    // This prevents stale content (copied before the app launched, or hours
    // ago) from always appearing as context on every shake.
    {
        let mut guard = LAST_CLIPBOARD.lock().ok()?;
        if *guard == trimmed {
            return None; // unchanged — treat as "no clipboard context"
        }
        *guard = trimmed.to_string(); // remember it for next time
    }

    // Reject empty, very short, or non-prose content.
    if trimmed.len() < 4 {
        return None;
    }

    // Reject JSON / structured data (starts with { or [).
    let first = trimmed.chars().next().unwrap_or(' ');
    if first == '{' || first == '[' {
        return None;
    }

    // Reject single-word tokens that look like secrets or URLs,
    // but allow file/folder paths (contain path separators).
    let has_space = trimmed.contains(' ') || trimmed.contains('\n');
    let looks_like_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    let looks_like_path = trimmed.contains('/') || trimmed.contains('\\');
    if !has_space && !looks_like_path && (trimmed.len() > 60 || looks_like_url) {
        return None;
    }
    // Still reject bare URLs even if they look like a path
    if looks_like_url && !has_space {
        return None;
    }

    Some(trimmed.to_string())
}

fn capture_screenshot(x: i32, y: i32, radius: u32) -> Option<String> {
    let screen = screenshots::Screen::from_point(x, y).ok()?;

    let r = radius as i32;
    // capture_area expects coordinates relative to the screen's own logical origin,
    // not absolute desktop coordinates. Subtract the monitor's top-left corner.
    // (On the primary monitor at (0,0) this is a no-op; on secondary monitors it
    // prevents capturing the wrong region of the desktop.)
    let origin_x = screen.display_info.x;
    let origin_y = screen.display_info.y;
    let cap_x = x - r - origin_x;
    let cap_y = y - r - origin_y;
    let width = (radius * 2) as u32;
    let height = (radius * 2) as u32;

    let img = screen.capture_area(cap_x, cap_y, width, height).ok()?;

    // screenshots returns an image::RgbaImage
    let rgba = img.as_raw();
    let dyn_img = image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(img.width(), img.height(), rgba.to_vec())?,
    );

    let mut png_bytes: Vec<u8> = Vec::new();
    dyn_img
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .ok()?;

    Some(general_purpose::STANDARD.encode(&png_bytes))
}
