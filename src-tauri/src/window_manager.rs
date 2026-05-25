use tauri::{AppHandle, Manager as _, PhysicalPosition};

pub fn show_overlay(app: &AppHandle, x: i32, y: i32) {
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };

    const OVERLAY_W: i32 = 440;
    const OVERLAY_H: i32 = 400; // reserve full expanded height for clamping
    const MARGIN: i32 = 8;      // keep a small gap from the screen edge

    // Default position: 20px right of cursor, 40px above
    let mut pos_x = x + 20;
    let mut pos_y = y - 40;

    // Find the monitor containing the cursor and clamp the overlay inside it.
    //
    // Why not window.current_monitor()?
    //   current_monitor() returns the monitor the overlay window is currently on.
    //   Before the first shake that may be wrong (the window is off-screen or on
    //   a different display than the cursor).
    //
    // Why divide by scale_factor?
    //   rdev reports cursor coordinates in LOGICAL pixels (macOS points / Windows
    //   DIPs).  Tauri's Monitor::position() and Monitor::size() are in PHYSICAL
    //   pixels.  On a 2× Retina display the monitor reports 2880 × 1800 physical
    //   pixels but the logical space is 1440 × 900.  Dividing by scale_factor
    //   converts the monitor bounds to the same logical coordinate space as (x, y).
    let monitors = window.available_monitors().unwrap_or_default();

    let monitor = monitors
        .iter()
        .find(|m| {
            let sf = m.scale_factor();
            let mp = m.position();
            let ms = m.size();
            let lx = (mp.x as f64 / sf) as i32;
            let ly = (mp.y as f64 / sf) as i32;
            let lw = (ms.width as f64 / sf) as i32;
            let lh = (ms.height as f64 / sf) as i32;
            x >= lx && x < lx + lw && y >= ly && y < ly + lh
        })
        .or_else(|| monitors.first()); // fallback to primary monitor

    if let Some(m) = monitor {
        let sf = m.scale_factor();
        let mp = m.position();
        let ms = m.size();

        // Logical monitor bounds
        let lx = (mp.x as f64 / sf) as i32;
        let ly = (mp.y as f64 / sf) as i32;
        let lw = (ms.width as f64 / sf) as i32;
        let lh = (ms.height as f64 / sf) as i32;

        let min_x = lx + MARGIN;
        let min_y = ly + MARGIN;
        let max_x = (lx + lw - OVERLAY_W - MARGIN).max(min_x);
        let max_y = (ly + lh - OVERLAY_H - MARGIN).max(min_y);

        pos_x = pos_x.clamp(min_x, max_x);
        pos_y = pos_y.clamp(min_y, max_y);
    }

    let _ = window.set_position(PhysicalPosition::new(pos_x, pos_y));
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        // Reset to bar size before hiding so next show starts compact
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: 440.0,
            height: 64.0,
        }));
        let _ = window.hide();
    }
}

pub fn resize_overlay(app: &AppHandle, height: f64) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: 440.0,
            height,
        }));
    }
}
