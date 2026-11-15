//! Native quick actions. Checkmarks follow settings, regardless of where they change.
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, Manager};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use crate::{commands::MeetingState, overlay, settings::SettingsStore};

#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStatus {
    ready: bool,
    streaming: bool,
    meeting_busy: bool,
    selecting: bool,
}

struct TrayState {
    protection: CheckMenuItem<tauri::Wry>,
    clickthrough: CheckMenuItem<tauri::Wry>,
    meeting: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    actions: Vec<MenuItem<tauri::Wry>>,
    ui: Mutex<UiStatus>,
    quitting: AtomicBool,
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let item = |id, text, enabled| MenuItem::with_id(app, id, text, enabled, None::<&str>);
    let show = item("show", "Open Notely", true)?;
    let hide = item("hide", "Hide Notely", true)?;
    let new_chat = item("new_chat", "New chat", false)?;
    let analyze = item("analyze", "Analyze screen", false)?;
    let region = item("region", "Capture a region…", false)?;
    let meeting = item("meeting", "Start meeting…", false)?;
    let transcript = item("transcript", "Transcript", false)?;
    let notes = item("notes", "Meeting notes", false)?;
    let history = item("history", "Chat and meeting history", false)?;
    let workspace = Submenu::with_items(app, "Your workspace", true, &[&transcript, &notes, &history])?;
    let settings = item("settings", "Settings…", false)?;
    let help = item("help", "Help and setup…", false)?;
    let diagnostics = item("diagnostics", "Diagnostics…", false)?;
    let prefs = app.state::<SettingsStore>().get();
    let protection = CheckMenuItem::with_id(app, "protection", "Request screen-share protection", true, prefs.content_protection, None::<&str>)?;
    let clickthrough = CheckMenuItem::with_id(app, "clickthrough", "Pass clicks through window", true, prefs.click_through, None::<&str>)?;
    let quit = item("quit", "Quit Notely", true)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &hide, &sep1, &new_chat, &analyze, &region, &meeting, &workspace, &sep2, &protection, &clickthrough, &sep3, &settings, &help, &diagnostics, &sep4, &quit])?;
    app.manage(TrayState {
        protection, clickthrough, meeting, quit,
        actions: vec![new_chat, analyze, region, transcript, notes, history, settings, help, diagnostics],
        ui: Mutex::new(UiStatus::default()),
        quitting: AtomicBool::new(false),
    });
    #[cfg(target_os = "macos")]
    let bytes: &[u8] = include_bytes!("../icons/tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let bytes: &[u8] = include_bytes!("../icons/32x32.png");
    TrayIconBuilder::with_id("tray")
        .tooltip("Notely — your everyday companion")
        .icon(tauri::image::Image::from_bytes(bytes)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "show" => open_action(app, "assist"),
                "hide" => overlay::set_hidden(app, true),
                "clickthrough" => overlay::toggle_clickthrough(app).map(|_| ()),
                "protection" => overlay::toggle_protection(app).map(|_| ()),
                "quit" => { quit_safely(app); Ok(()) }
                action => open_action(app, action),
            };
            if let Err(error) = result {
                let _ = app.emit(crate::state::events::ERROR, serde_json::json!({"message": error}));
            }
            sync(app);
        })
        .build(app)?;
    Ok(())
}

fn open_action(app: &AppHandle, action: &str) -> Result<(), String> {
    overlay::set_hidden(app, false)?;
    app.emit(crate::state::events::TRAY_ACTION, serde_json::json!({"action": action})).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tray_update(app: AppHandle, status: UiStatus) {
    if let Some(tray) = app.try_state::<TrayState>() {
        *tray.ui.lock().unwrap() = status;
    }
    sync(&app);
}

pub fn sync(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.try_state::<TrayState>() else { return };
        let prefs = handle.state::<SettingsStore>().get();
        let ui = *tray.ui.lock().unwrap();
        let quitting = tray.quitting.load(Ordering::SeqCst);
        let state = handle.state::<MeetingState>();
        let (recording, starting) = { let m = state.0.lock().unwrap(); (m.recording, m.starting) };
        let _ = tray.protection.set_checked(prefs.content_protection);
        let _ = tray.clickthrough.set_checked(prefs.click_through);
        let _ = tray.meeting.set_text(if ui.meeting_busy || starting { "Meeting is updating…" } else if recording { "End meeting and save" } else { "Start meeting…" });
        let _ = tray.meeting.set_enabled(ui.ready && !ui.meeting_busy && !starting && !ui.selecting && !quitting);
        let _ = tray.quit.set_text(if recording { "Save meeting and quit Notely" } else { "Quit Notely" });
        let _ = tray.quit.set_enabled(!ui.meeting_busy && !starting && !quitting);
        for item in &tray.actions {
            let needs_idle = matches!(item.id().as_ref(), "new_chat" | "analyze" | "region");
            let _ = item.set_enabled(ui.ready && !ui.selecting && !quitting && (!needs_idle || (!ui.streaming && !ui.meeting_busy)));
        }
    });
}

fn quit_safely(app: &AppHandle) {
    let tray = app.state::<TrayState>();
    if tray.ui.lock().unwrap().meeting_busy || tray.quitting.swap(true, Ordering::SeqCst) { return; }
    sync(app);
    let app = app.clone();
    // Finish the same native save path as End meeting before exiting.
    tauri::async_runtime::spawn(async move {
        let state = app.state::<MeetingState>();
        let (recording, starting) = { let m = state.0.lock().unwrap(); (m.recording, m.starting) };
        if starting {
            app.state::<TrayState>().quitting.store(false, Ordering::SeqCst);
            sync(&app);
            return;
        }
        if recording {
            if let Err(error) = crate::commands::meeting_stop(app.clone(), state).await {
                app.state::<TrayState>().quitting.store(false, Ordering::SeqCst);
                sync(&app);
                let _ = overlay::set_hidden(&app, false);
                let _ = app.emit(crate::state::events::ERROR, serde_json::json!({"message": format!("Meeting could not be saved; Notely is still open. {error}")}));
                return;
            }
        }
        app.exit(0);
    });
}
