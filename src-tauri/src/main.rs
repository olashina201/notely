//! Notely — stealth meeting overlay. All Gemini calls happen here in Rust.
//! The webview is UI-only; the API key never leaves this process.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assist;
mod audio;
mod commands;
mod db;
mod gemini;
mod live;
mod notes;
mod settings;
mod state;
mod vision;
use commands::MeetingState;
use settings::SettingsStore;
use tauri::{AppHandle, Emitter, Manager};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
#[cfg(target_os = "macos")]
pub fn make_window_always_on_top(w: &tauri::WebviewWindow) {
    let app = w.app_handle().clone();
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = app2.get_webview_window("overlay") {
            apply_macos_window_levels(&win);
        }
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn yes_imp(_this: *mut std::ffi::c_void, _cmd: *const std::ffi::c_void) -> i8 {
    1
}

#[cfg(target_os = "macos")]
pub fn apply_macos_window_levels(w: &tauri::WebviewWindow) {
    use std::ffi::{c_char, c_void};

    extern "C" {
        fn object_getClass(obj: *mut c_void) -> *mut c_void;
        fn class_replaceMethod(cls: *mut c_void, name: *const c_void, imp: unsafe extern "C" fn(*mut c_void, *const c_void) -> i8, types: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend();
    }

    type MsgSendLevel = unsafe extern "C" fn(*mut c_void, *const c_void, i64);
    type MsgSendBehavior = unsafe extern "C" fn(*mut c_void, *const c_void, usize);
    type MsgSendBool = unsafe extern "C" fn(*mut c_void, *const c_void, i32);
    type MsgSendVoid = unsafe extern "C" fn(*mut c_void, *const c_void);

    if let Ok(ptr) = w.ns_window() {
        if !ptr.is_null() {
            unsafe {
                // Ensure the window class allows key & main window focus so input fields accept typing.
                let cls = object_getClass(ptr);
                if !cls.is_null() {
                    let sel_can_become_key = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as *const c_char);
                    let sel_can_become_main = sel_registerName(b"canBecomeMainWindow\0".as_ptr() as *const c_char);
                    let types = b"c@:\0".as_ptr() as *const c_char;
                    class_replaceMethod(cls, sel_can_become_key, yes_imp, types);
                    class_replaceMethod(cls, sel_can_become_main, yes_imp, types);
                }

                // 1. Level 1000 (kCGScreenSaverWindowLevel / NSWindowLevel(kCGScreenSaverWindowLevelKey))
                // Essential on macOS: Standard .floating (3) or .popUpMenu (101) sit BELOW native fullscreen spaces.
                // Level 1000 sits ABOVE native fullscreen spaces (Antigravity IDE, Google Chrome, etc.).
                let sel_set_level = sel_registerName(b"setLevel:\0".as_ptr() as *const c_char);
                let set_level: MsgSendLevel = std::mem::transmute(objc_msgSend as *const ());
                set_level(ptr, sel_set_level, 1000i64);

                // 2. Set collection behavior:
                // NSWindowCollectionBehaviorCanJoinAllSpaces = 1 << 0 (1)
                // NSWindowCollectionBehaviorIgnoresCycle = 1 << 6 (64)
                // NSWindowCollectionBehaviorFullScreenAuxiliary = 1 << 8 (256)
                // NSWindowCollectionBehaviorCanJoinAllApplications = 1 << 18 (262144)
                // Total = 1 | 64 | 256 | 262144 = 262465 (0x40141).
                let behavior: usize = (1 << 0) | (1 << 6) | (1 << 8) | (1 << 18);
                let sel_set_behavior = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as *const c_char);
                let set_behavior: MsgSendBehavior = std::mem::transmute(objc_msgSend as *const ());
                set_behavior(ptr, sel_set_behavior, behavior);

                // 3. Do NOT hide when user switches to or clicks another app
                let sel_set_hides = sel_registerName(b"setHidesOnDeactivate:\0".as_ptr() as *const c_char);
                let set_hides: MsgSendBool = std::mem::transmute(objc_msgSend as *const ());
                set_hides(ptr, sel_set_hides, 0i32); // NO / false

                // 4. orderFrontRegardless brings it in front across all spaces and apps
                let sel_order_front = sel_registerName(b"orderFrontRegardless\0".as_ptr() as *const c_char);
                let order_front: MsgSendVoid = std::mem::transmute(objc_msgSend as *const ());
                order_front(ptr, sel_order_front);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn make_window_always_on_top(_w: &tauri::WebviewWindow) {}

fn main() {
    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::{c_char, c_void};
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *const c_void;
            fn objc_msgSend();
        }
        type MsgSendGetApp = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
        type MsgSendSetPolicy = unsafe extern "C" fn(*mut c_void, *const c_void, i64) -> i8;

        let ns_app_cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
        let sel_shared_app = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
        let shared_app: MsgSendGetApp = std::mem::transmute(objc_msgSend as *const ());
        let app_inst = shared_app(ns_app_cls, sel_shared_app);

        // Pre-configure Accessory activation policy before any windows are created by Tauri
        let sel_set_policy = sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const c_char);
        let set_policy: MsgSendSetPolicy = std::mem::transmute(objc_msgSend as *const ());
        set_policy(app_inst, sel_set_policy, 1); // 1 = NSApplicationActivationPolicyAccessory
    }

    tauri::Builder::default()
        .manage(SettingsStore(std::sync::Mutex::new(state::Settings::default())))
        .manage(MeetingState(std::sync::Mutex::new(Default::default())))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            // Load persisted settings, apply window prefs.
            let loaded = settings::load_settings(&handle);
            handle.state::<SettingsStore>().update(loaded.clone());
            if let Some(w) = handle.get_webview_window("overlay") {
                let _ = w.set_content_protected(loaded.content_protection);
                let _ = w.set_ignore_cursor_events(loaded.click_through);
                let _ = w.set_always_on_top(true);
                make_window_always_on_top(&w);
                #[cfg(target_os = "macos")]
                {
                    use tauri::ActivationPolicy;
                    let _ = app.set_activation_policy(ActivationPolicy::Accessory);
                }
            }

            // Continuous float monitor: ensures Notely stays pinned on top even when other apps
            // expand, maximize, open modal dialogs, or transition to fullscreen Spaces.
            let monitor_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(300));
                loop {
                    interval.tick().await;
                    let h = monitor_handle.clone();
                    let h2 = monitor_handle.clone();
                    let _ = h.run_on_main_thread(move || {
                        if let Some(w) = h2.get_webview_window("overlay") {
                            #[cfg(target_os = "macos")]
                            apply_macos_window_levels(&w);
                        }
                    });
                }
            });
            register_hotkeys(&handle, &loaded.hotkeys);

            // System tray icon: gives users a permanent recovery path to show/hide
            // Notely and quit, even in stealth/accessory mode without Dock presence.
            let show_i = MenuItem::with_id(app, "show", "Show Notely", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "Hide Notely", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let toggle_stealth = MenuItem::with_id(app, "toggle_stealth", "Toggle Stealth Mode (🥷)", true, None::<&str>)?;
            let toggle_ct = MenuItem::with_id(app, "toggle_clickthrough", "Toggle Click-Through", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Notely", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_i, &hide_i, &sep1, &toggle_stealth, &toggle_ct, &sep2, &quit_i])?;

            let img = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .map_err(|e| format!("failed to load tray icon: {e}"))?;

            TrayIconBuilder::with_id("tray")
                .tooltip("Notely Meeting Assistant")
                .icon(img)
                .menu(&tray_menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("overlay") {
                                let _ = w.show();
                                let _ = w.set_always_on_top(true);
                                make_window_always_on_top(&w);
                                let _ = w.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(w) = app.get_webview_window("overlay") {
                                let _ = w.hide();
                            }
                        }
                        "toggle_clickthrough" => {
                            let store = app.state::<SettingsStore>();
                            let next = !store.get().click_through;
                            store.modify(|s| s.click_through = next);
                            if let Some(w) = app.get_webview_window("overlay") {
                                let _ = w.set_ignore_cursor_events(next);
                            }
                        }
                        "toggle_stealth" => {
                            let store = app.state::<SettingsStore>();
                            let next = !store.get().content_protection;
                            store.modify(|s| s.content_protection = next);
                            let _ = settings::save_settings(app, &store.get());
                            if let Some(w) = app.get_webview_window("overlay") {
                                let _ = w.set_content_protected(next);
                            }
                            #[cfg(target_os = "macos")]
                            {
                                use tauri::ActivationPolicy;
                                let _ = app.set_activation_policy(ActivationPolicy::Accessory);
                            }
                            let _ = app.emit(state::events::STEALTH_CHANGED, serde_json::json!({"stealth": next}));
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("overlay") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_always_on_top(true);
                                let _ = w.set_visible_on_all_workspaces(true);
                                make_window_always_on_top(&w);
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings_get, commands::settings_set,
            commands::key_has, commands::key_set, commands::key_clear, commands::key_test,
            commands::models_list,
            commands::capture_full_cmd, commands::capture_active_cmd, commands::capture_region_cmd, commands::capture_clear, commands::monitors_list,
            commands::selector_begin, commands::selector_end,
            commands::meeting_start, commands::meeting_stop, commands::meeting_transcript_add,
            commands::assist_run, commands::assist_cancel,
            commands::notes_generate,
            commands::history_list, commands::history_search, commands::history_get, commands::history_delete,
            commands::overlay_hide, commands::overlay_show, commands::overlay_toggle_clickthrough,
            commands::overlay_toggle_stealth, commands::overlay_set_stealth, commands::overlay_focus,
            commands::register_hotkeys_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run notely");
}
/// (Re)register global hotkeys from settings bindings.
pub(crate) fn register_hotkeys(app: &AppHandle, hotkeys: &std::collections::HashMap<String, String>) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let actions: &[(&str, &str)] = &[
        ("assist", "assist"), ("toggleStealth", "toggle_stealth"),
        ("captureFull", "capture_full"), ("captureRegion", "capture_region"),
        ("panicHide", "panic_hide"), ("toggleClickThrough", "toggle_clickthrough"),
        ("say", "preset:say"), ("followup", "preset:followup"), ("recap", "preset:recap"), ("screen", "preset:screen"),
    ];
    for (action, tag) in actions {
        let Some(binding) = hotkeys.get(*action) else { continue };
        let tag = tag.to_string();
        if gs.on_shortcut(binding.as_str(), move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                handle_hotkey(app, &tag);
            }
        }).is_err() {
            let _ = app.emit(state::events::ERROR, serde_json::json!({"message": format!("hotkey register failed: {binding}")}));
        }
    }
    // Nudge overlay position: Cmd/Ctrl + arrows move the window.
    for (binding, dx, dy) in [("CommandOrControl+Left", -20, 0), ("CommandOrControl+Right", 20, 0), ("CommandOrControl+Up", 0, -20), ("CommandOrControl+Down", 0, 20)] {
        let (dx, dy) = (dx, dy);
        let _ = gs.on_shortcut(binding, move |app, _s, e| {
            if e.state == ShortcutState::Pressed {
                if let Some(w) = app.get_webview_window("overlay") {
                    if let Ok(p) = w.outer_position() {
                        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: p.x + dx, y: p.y + dy }));
                    }
                }
            }
        });
    }
}
fn handle_hotkey(app: &AppHandle, tag: &str) {
    match tag {
        // Toggle: the global shortcut still fires while hidden, so this is recoverable.
        "panic_hide" => {
            if let Some(w) = app.get_webview_window("overlay") {
                if w.is_visible().unwrap_or(true) {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                    let _ = w.set_always_on_top(true);
                    let _ = w.set_visible_on_all_workspaces(true);
                    make_window_always_on_top(&w);
                    let _ = w.set_focus();
                }
            }
        }
        "toggle_clickthrough" => {
            let store = app.state::<SettingsStore>();
            let next = !store.get().click_through;
            store.modify(|s| s.click_through = next);
            if let Some(w) = app.get_webview_window("overlay") { let _ = w.set_ignore_cursor_events(next); }
        }
        "toggle_stealth" => {
            let store = app.state::<SettingsStore>();
            let next = !store.get().content_protection;
            store.modify(|s| s.content_protection = next);
            let _ = settings::save_settings(app, &store.get());
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.set_content_protected(next);
            }
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                let _ = app.set_activation_policy(ActivationPolicy::Accessory);
            }
            let _ = app.emit(state::events::STEALTH_CHANGED, serde_json::json!({"stealth": next}));
        }
        "capture_full" => {
            let a = app.clone();
            tauri::async_runtime::spawn(async move {
                match vision::capture_full(None) {
                    Ok(img) => { push_hotkey_capture(&a, img); }
                    Err(e) => { let _ = a.emit(state::events::ERROR, serde_json::json!({"message": e})); }
                }
            });
        }
        "assist" => { let _ = app.emit("hotkey.assist", ()); }
        "capture_region" => { let _ = app.emit("hotkey.capture_region", ()); }
        t if t.starts_with("preset:") => {
            let preset = t.trim_start_matches("preset:").to_string();
            let _ = app.emit("hotkey.preset", serde_json::json!({"preset": preset}));
        }
        _ => {}
    }
}
fn push_hotkey_capture(app: &AppHandle, img: state::CapturedImage) {
    if let Some(meeting) = app.try_state::<MeetingState>() {
        if let Ok(mut m) = meeting.0.lock() {
            m.captures.push(img.clone());
            while m.captures.len() > 4 { m.captures.remove(0); }
        }
    }
    let _ = app.emit(state::events::CAPTURE_TAKEN, &img);
}
