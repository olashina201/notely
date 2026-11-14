//! One show/hide path for commands, hotkeys, and the tray.
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use crate::settings::SettingsStore;

static HIDDEN: AtomicBool = AtomicBool::new(false);

pub fn set_protection(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let window = app.get_webview_window("overlay").ok_or("Overlay is unavailable")?;
    window.set_content_protected(enabled).map_err(|e| e.to_string())?;
    let store = app.state::<SettingsStore>();
    store.modify(|s| s.content_protection = enabled);
    crate::tray::sync(app);
    app.emit(crate::state::events::STEALTH_CHANGED, serde_json::json!({"stealth": enabled})).map_err(|e| e.to_string())?;
    crate::settings::save_settings(app, &store.get())?;
    Ok(enabled)
}

pub fn toggle_protection(app: &AppHandle) -> Result<bool, String> {
    set_protection(app, !app.state::<SettingsStore>().get().content_protection)
}

pub fn set_clickthrough(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let window = app.get_webview_window("overlay").ok_or("Overlay is unavailable")?;
    window.set_ignore_cursor_events(enabled).map_err(|e| e.to_string())?;
    app.state::<SettingsStore>().modify(|s| s.click_through = enabled);
    crate::tray::sync(app);
    crate::diagnostics::record("info", "overlay.clickthrough_changed", None, serde_json::json!({"enabled": enabled}));
    app.emit("overlay:clickthrough", serde_json::json!({"enabled": enabled})).map_err(|e| e.to_string())
}

pub fn toggle_clickthrough(app: &AppHandle) -> Result<bool, String> {
    let enabled = !app.state::<SettingsStore>().get().click_through;
    set_clickthrough(app, enabled)?;
    Ok(enabled)
}

pub fn set_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    crate::diagnostics::record(
        "info",
        "overlay.visibility_requested",
        None,
        serde_json::json!({"hidden": hidden}),
    );
    HIDDEN.store(hidden, Ordering::SeqCst);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        // A newer action wins over queued window operations.
        if HIDDEN.load(Ordering::SeqCst) != hidden {
            return;
        }
        if let Some(w) = handle.get_webview_window("overlay") {
            let result = if hidden {
                w.hide()
            } else {
                // Tray/hotkey Show is also the recovery path from click-through.
                let _ = set_clickthrough(&handle, false);
                let _ = w.set_visible_on_all_workspaces(true);
                let _ = w.set_always_on_top(true);
                let result = w.show();
                crate::make_window_always_on_top(&w);
                let _ = w.set_focus();
                #[cfg(target_os = "macos")]
                if let Ok(ptr) = w.ns_window() {
                    if !ptr.is_null() {
                        unsafe {
                            crate::notely_focus_window(ptr);
                        }
                    }
                }
                result
            };
            crate::diagnostics::record(
                if result.is_ok() { "info" } else { "error" },
                "overlay.visibility_finished",
                None,
                serde_json::json!({"hidden": hidden, "success": result.is_ok()}),
            );
            if let Err(error) = result {
                let _ = handle.emit(
                    "error",
                    serde_json::json!({"message": format!("Overlay visibility failed: {error}")}),
                );
            }
        }
    })
    .map_err(|e| e.to_string())
}

pub fn toggle(app: &AppHandle) -> Result<(), String> {
    set_hidden(app, !HIDDEN.load(Ordering::SeqCst))
}
