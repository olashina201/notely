//! Tauri commands: the only bridge between webview UI and the Rust backend.
//! The API key never leaves Rust; the frontend only gets success/failure.
use crate::{assist, audio, db, gemini, live, notes, settings, vision};
use crate::state::{events, AssistRequest, CapturedImage, Settings, TranscriptLine};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
pub struct MeetingState(pub Mutex<MeetingData>);
pub struct MeetingData {
    pub recording: bool,
    pub meeting_id: Option<i64>,
    pub transcript: Vec<TranscriptLine>,
    pub captures: Vec<CapturedImage>,
    pub live: Option<live::LiveHandle>,
    pub mixer: Option<audio::Mixer>,
    pub summary_context: String,
    pub started_at: i64,
}
impl Default for MeetingData {
    fn default() -> Self {
        Self { recording: false, meeting_id: None, transcript: Vec::new(), captures: Vec::new(), live: None, mixer: None, summary_context: String::new(), started_at: 0 }
    }
}
#[tauri::command]
pub async fn settings_get(app: AppHandle, store: State<'_, settings::SettingsStore>) -> Result<Settings, String> {
    let _ = app;
    Ok(store.get())
}
#[tauri::command]
pub async fn settings_set(app: AppHandle, store: State<'_, settings::SettingsStore>, s: Settings) -> Result<(), String> {
    settings::save_settings(&app, &s)?;
    store.update(s.clone());
    apply_window_prefs(&app, &s);
    Ok(())
}
fn apply_window_prefs(app: &AppHandle, s: &Settings) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_content_protected(s.content_protection);
        let _ = w.set_ignore_cursor_events(s.click_through).map_err(|_| ());
        #[cfg(target_os = "macos")]
        {
            use tauri::ActivationPolicy;
            let _ = app.set_activation_policy(ActivationPolicy::Accessory);
        }
    }
}
pub(crate) fn resolve_key(app: &AppHandle) -> Result<String, String> {
    if let Ok(k) = settings::key_get() {
        if !k.trim().is_empty() {
            return Ok(k.trim().to_string());
        }
    }
    if let Ok(path) = db::db_path(app) {
        if let Ok(dbc) = db::open(&path) {
            if let Ok(Some(k)) = db::get_setting(&dbc, "_api_key") {
                if !k.trim().is_empty() {
                    return Ok(k.trim().to_string());
                }
            }
        }
    }
    Err("no API key set — add it in Settings".to_string())
}
#[tauri::command]
pub async fn key_has(app: AppHandle) -> Result<bool, String> { Ok(resolve_key(&app).is_ok()) }
#[tauri::command]
pub async fn key_set(app: AppHandle, key: String) -> Result<(), String> {
    let trimmed = key.trim();
    let _ = settings::key_set(trimmed);
    if let Ok(path) = db::db_path(&app) {
        if let Ok(dbc) = db::open(&path) {
            let _ = db::set_setting(&dbc, "_api_key", trimmed);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn key_clear(app: AppHandle) -> Result<(), String> {
    let _ = settings::key_clear();
    if let Ok(path) = db::db_path(&app) {
        if let Ok(dbc) = db::open(&path) {
            let _ = db::set_setting(&dbc, "_api_key", "");
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn key_test(app: AppHandle) -> Result<String, String> {
    let key = resolve_key(&app)?;
    let models = gemini::list_models(&key).await?;
    let live_ok = models.iter().any(|m| m.id.contains("live"));
    Ok(format!("OK: {} models listed, live-capable model {}", models.len(), if live_ok { "found" } else { "NOT found" }))
}
#[tauri::command]
pub async fn models_list(app: AppHandle) -> Result<Vec<gemini::ModelInfo>, String> {
    let key = resolve_key(&app)?;
    gemini::list_models(&key).await
}
#[tauri::command]
pub async fn capture_full_cmd(app: AppHandle, monitor_index: Option<usize>, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<CapturedImage, String> {
    let img = vision::capture_full(monitor_index)?;
    push_capture(&app, &store, &meeting, img.clone());
    Ok(img)
}
#[tauri::command]
pub async fn capture_active_cmd(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<CapturedImage, String> {
    let img = vision::capture_active_window()?;
    push_capture(&app, &store, &meeting, img.clone());
    Ok(img)
}
fn push_capture(app: &AppHandle, store: &State<settings::SettingsStore>, meeting: &State<MeetingState>, img: CapturedImage) {
    let save = store.get().save_captures;
    if let Ok(mut m) = meeting.0.lock() {
        m.captures.push(img.clone());
        if !save {
            while m.captures.len() > 4 { m.captures.remove(0); }
        }
    }
    let _ = app.emit(events::CAPTURE_TAKEN, &img);
}
#[tauri::command]
pub async fn capture_clear(meeting: State<'_, MeetingState>) -> Result<(), String> {
    if let Ok(mut m) = meeting.0.lock() { m.captures.clear(); }
    Ok(())
}
#[tauri::command]
pub async fn selector_begin(app: AppHandle) -> Result<serde_json::Value, String> {
    let Some(w) = app.get_webview_window("overlay") else { return Err("no overlay window".into()) };
    let monitors = vision::list_monitors()?;
    let Some(current) = w.current_monitor().map_err(|e| e.to_string())? else {
        return Err("no current monitor".into());
    };
    // Tauri geometry is physical pixels; xcap capture coordinates are points on
    // macOS (physical elsewhere). Convert so we can identify the xcap monitor
    // under the window, then hand its xcap-space rect to the frontend. The
    // selector reports its drag as window fractions, which are unit-free.
    let scale = current.scale_factor().max(0.5);
    let pos = current.position();
    let size = current.size();
    let (mx, my, mw, mh) = (pos.x, pos.y, size.width, size.height);
    #[cfg(target_os = "macos")]
    let (tx, ty, tw, th) = (
        (mx as f64 / scale).round() as i32,
        (my as f64 / scale).round() as i32,
        (mw as f64 / scale).round() as u32,
        (mh as f64 / scale).round() as u32,
    );
    #[cfg(not(target_os = "macos"))]
    let (tx, ty, tw, th) = { let _ = scale; (mx, my, mw, mh) };
    let cap = monitors
        .iter()
        .find(|m| m.x == tx && m.y == ty && m.width == tw && m.height == th)
        .or_else(|| {
            monitors.iter().min_by_key(|m| {
                let cx = m.x + m.width as i32 / 2 - (tx + tw as i32 / 2);
                let cy = m.y + m.height as i32 / 2 - (ty + th as i32 / 2);
                (cx as i64 * cx as i64 + cy as i64 * cy as i64) as u64
            })
        })
        .ok_or("no monitors found")?;
    let saved_pos = w.outer_position().map_err(|e| e.to_string())?;
    let saved_size = w.outer_size().map_err(|e| e.to_string())?;
    // Cover the whole monitor directly (no native fullscreen: avoids the macOS
    // Space-switch animation and keeps the overlay on top). Also make sure the
    // window receives mouse events and isn't captured into the screenshot.
    w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: mx, y: my }))
        .map_err(|e| e.to_string())?;
    w.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: mw, height: mh }))
        .map_err(|e| e.to_string())?;
    let _ = w.set_ignore_cursor_events(false);
    let _ = w.set_content_protected(true);
    let _ = w.set_always_on_top(true);
    let _ = w.set_visible_on_all_workspaces(true);
    let _ = w.show();
    let _ = w.set_focus();
    Ok(serde_json::json!({
        "monitors": monitors,
        "saved": {"x": saved_pos.x, "y": saved_pos.y, "w": saved_size.width, "h": saved_size.height},
        "monitor": {"x": cap.x, "y": cap.y, "w": cap.width, "h": cap.height},
    }))
}
#[tauri::command]
pub async fn selector_end(app: AppHandle, saved: Option<serde_json::Value>) -> Result<(), String> {
    let Some(win) = app.get_webview_window("overlay") else { return Err("no overlay window".into()) };
    if let Some(s) = saved {
        let (x, y, w, h) = (
            s.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            s.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            s.get("w").and_then(|v| v.as_u64()).unwrap_or(820) as u32,
            s.get("h").and_then(|v| v.as_u64()).unwrap_or(540) as u32,
        );
        let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
        let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: w, height: h }));
    }
    // Restore click-through / content-protection prefs, and re-assert float.
    if let Some(store) = app.try_state::<settings::SettingsStore>() {
        let s = store.get();
        let _ = win.set_ignore_cursor_events(s.click_through);
        let _ = win.set_content_protected(s.content_protection);
    }
    let _ = win.set_always_on_top(true);
    let _ = win.set_visible_on_all_workspaces(true);
    crate::make_window_always_on_top(&win);
    Ok(())
}
#[tauri::command]
pub async fn monitors_list() -> Result<Vec<vision::MonitorInfo>, String> { vision::list_monitors() }
#[tauri::command]
pub async fn capture_region_cmd(app: AppHandle, x: i32, y: i32, w: u32, h: u32, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<CapturedImage, String> {
    let img = vision::capture_region(x, y, w, h)?;
    push_capture(&app, &store, &meeting, img.clone());
    Ok(img)
}
#[tauri::command]
pub async fn meeting_start(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<(), String> {
    {
        let m = meeting.0.lock().map_err(|_| "state lock failed")?;
        if m.recording { return Err("already recording".into()); }
    }
    let key = resolve_key(&app)?;
    let s = store.get();
    let started_at = chrono::Utc::now().timestamp_millis();
    let meeting_id = {
        let path = db::db_path(&app)?;
        let dbc = db::open(&path).map_err(|e| e.to_string())?;
        db::insert_meeting(&dbc, "Meeting", &s.instruction_profile, started_at).map_err(|e| e.to_string())?
    };
    // Mixer tags mic "me" + system "other" and forwards into the Live session.
    // System audio may be absent (stock macOS): meeting still runs, mic-only.
    let (live_tx, live_rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<i16>)>();
    let (mixer, sys_warning) = audio::Mixer::start(live_tx.clone()).map_err(|e| format!("audio start failed: {e}"))?;
    if let Some(w) = &sys_warning {
        let _ = app.emit(events::ERROR, serde_json::json!({"message": w}));
    }
    let summary = meeting.0.lock().map(|m| m.summary_context.clone()).unwrap_or_default();
    let live = live::spawn_live_session(app.clone(), live::LiveConfig { api_key: key, model: s.live_model.clone(), language: s.language.clone() }, summary, live_rx, live_tx)?;
    {
        let mut m = meeting.0.lock().map_err(|_| "state lock failed")?;
        m.mixer = Some(mixer);
        m.live = Some(live);
    }
    {
        let mut m = meeting.0.lock().map_err(|_| "state lock failed")?;
        m.recording = true;
        m.meeting_id = Some(meeting_id);
        m.started_at = started_at;
        m.transcript.clear();
    }
    let _ = app.emit(events::TRANSCRIPT_STATUS, serde_json::json!({"status": "recording"}));
    Ok(())
}
#[tauri::command]
pub async fn meeting_transcript_add(meeting: State<'_, MeetingState>, line: TranscriptLine) -> Result<(), String> {
    if let Ok(mut m) = meeting.0.lock() {
        if line.is_final {
            m.transcript.push(line);
        } else if let Some(last) = m.transcript.last_mut() {
            if !last.is_final { *last = line; return Ok(()); }
            m.transcript.push(line);
        } else {
            m.transcript.push(line);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn meeting_stop(app: AppHandle, meeting: State<'_, MeetingState>) -> Result<serde_json::Value, String> {
    // Stop capture first, then the Live session.
    let mixer = meeting.0.lock().map(|mut m| m.mixer.take()).unwrap_or(None);
    if let Some(mx) = mixer { mx.stop(); }
    let live = meeting.0.lock().map(|mut m| m.live.take()).unwrap_or(None);
    if let Some(lh) = live { lh.stop(); }
    let (meeting_id, started_at, transcript) = {
        let mut m = meeting.0.lock().map_err(|_| "state lock failed")?;
        m.recording = false;
        (m.meeting_id.take(), m.started_at, std::mem::take(&mut m.transcript))
    };
    let _ = app.emit(events::TRANSCRIPT_STATUS, serde_json::json!({"status": "idle"}));
    if transcript.is_empty() {
        return Ok(serde_json::json!({"empty": true}));
    }
    if let Some(id) = meeting_id {
        let path = db::db_path(&app)?;
        let dbc = db::open(&path).map_err(|e| e.to_string())?;
        let ended = chrono::Utc::now().timestamp_millis();
        let tj = serde_json::to_string(&transcript).unwrap_or_else(|_| "[]".into());
        db::finish_meeting(&dbc, id, ended, &tj, None).map_err(|e| e.to_string())?;
        let mut m = meeting.0.lock().map_err(|_| "state lock failed")?;
        m.meeting_id = Some(id);
        m.started_at = started_at;
        m.transcript = transcript.clone();
    }
    Ok(serde_json::json!({"saved": true, "lines": transcript.len()}))
}
#[tauri::command]
pub async fn notes_generate(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<serde_json::Value, String> {
    let key = resolve_key(&app)?;
    let s = store.get();
    let (id, transcript) = {
        let m = meeting.0.lock().map_err(|_| "state lock failed")?;
        (m.meeting_id, m.transcript.clone())
    };
    if transcript.is_empty() { return Err("no transcript to summarize".into()); }
    let notes = notes::generate_notes(&key, &s.smart_model, &transcript, &s.instruction_profile).await?;
    if let Some(id) = id {
        let path = db::db_path(&app)?;
        let dbc = db::open(&path).map_err(|e| e.to_string())?;
        let tj = serde_json::to_string(&transcript).unwrap_or_else(|_| "[]".into());
        let nj = serde_json::to_string(&notes).unwrap_or_default();
        db::finish_meeting(&dbc, id, chrono::Utc::now().timestamp_millis(), &tj, Some(&nj)).map_err(|e| e.to_string())?;
    }
    Ok(notes)
}
#[tauri::command]
pub async fn assist_run(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>, req: AssistRequest) -> Result<(), String> {
    let key = resolve_key(&app)?;
    let s = store.get();
    let model = if req.slot == "smart" { s.smart_model.clone() } else { s.fast_model.clone() };
    let (transcript, existing_imgs) = {
        let m = meeting.0.lock().map_err(|_| "state lock failed")?;
        (m.transcript.clone(), m.captures.clone())
    };
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut images = existing_imgs;
        if req.include_screenshot && images.is_empty() && s.auto_attach_screenshot {
            if let Ok(Ok(Ok(img))) = tokio::time::timeout(
                std::time::Duration::from_millis(1500),
                tokio::task::spawn_blocking(|| vision::capture_full(None))
            ).await {
                images.push(img);
            }
        }
        let recent: Vec<CapturedImage> = images.into_iter().rev().take(4).rev().collect();
        if let Err(e) = assist::run_assist(app2.clone(), key, model, req, transcript, recent, s.thinking_level.clone(), s.search_grounding).await {
            assist::emit_all(&app2, events::ERROR, serde_json::json!({"message": e}));
            assist::emit_all(&app2, events::ASSIST_DONE, serde_json::json!({"error": e}));
        }
    });
    Ok(())
}
#[tauri::command]
pub async fn assist_cancel() -> Result<(), String> { assist::cancel_current(); Ok(()) }
#[tauri::command]
pub async fn history_list(app: AppHandle, limit: Option<i64>) -> Result<Vec<db::MeetingRow>, String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::list_meetings(&dbc, limit.unwrap_or(50)).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn history_search(app: AppHandle, q: String) -> Result<Vec<db::MeetingRow>, String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::search_meetings(&dbc, &q).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn history_get(app: AppHandle, id: i64) -> Result<Option<db::MeetingRow>, String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::get_meeting(&dbc, id).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn history_delete(app: AppHandle, id: i64) -> Result<(), String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::delete_meeting(&dbc, id).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn overlay_hide(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") { w.hide().map_err(|e| e.to_string())?; }
    Ok(())
}
#[tauri::command]
pub async fn overlay_show(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
        let _ = w.set_visible_on_all_workspaces(true);
        crate::make_window_always_on_top(&w);
        let _ = w.set_focus();
    }
    Ok(())
}
#[tauri::command]
pub async fn overlay_toggle_clickthrough(app: AppHandle) -> Result<bool, String> {
    let s = app.state::<settings::SettingsStore>().get();
    let next = !s.click_through;
    app.state::<settings::SettingsStore>().modify(|x| x.click_through = next);
    if let Some(w) = app.get_webview_window("overlay") { w.set_ignore_cursor_events(next).map_err(|e| e.to_string())?; }
    Ok(next)
}
#[tauri::command]
pub async fn overlay_toggle_stealth(app: AppHandle) -> Result<bool, String> {
    let s = app.state::<settings::SettingsStore>().get();
    let next = !s.content_protection;
    app.state::<settings::SettingsStore>().modify(|x| x.content_protection = next);
    let updated = app.state::<settings::SettingsStore>().get();
    let _ = settings::save_settings(&app, &updated);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_content_protected(next);
        let _ = w.set_always_on_top(true);
        let _ = w.set_visible_on_all_workspaces(true);
        crate::make_window_always_on_top(&w);
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    }
    let _ = app.emit(events::STEALTH_CHANGED, serde_json::json!({ "stealth": next }));
    Ok(next)
}
#[tauri::command]
pub async fn overlay_set_stealth(app: AppHandle, enabled: bool) -> Result<bool, String> {
    app.state::<settings::SettingsStore>().modify(|x| x.content_protection = enabled);
    let updated = app.state::<settings::SettingsStore>().get();
    let _ = settings::save_settings(&app, &updated);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_content_protected(enabled);
        let _ = w.set_always_on_top(true);
        let _ = w.set_visible_on_all_workspaces(true);
        crate::make_window_always_on_top(&w);
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    }
    let _ = app.emit(events::STEALTH_CHANGED, serde_json::json!({ "stealth": enabled }));
    Ok(enabled)
}
#[tauri::command]
pub async fn overlay_focus(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focus();
        #[cfg(target_os = "macos")]
        {
            use std::ffi::{c_char, c_void};
            extern "C" {
                fn objc_getClass(name: *const c_char) -> *mut c_void;
                fn sel_registerName(name: *const c_char) -> *const c_void;
                fn objc_msgSend();
            }
            type MsgSendBool = unsafe extern "C" fn(*mut c_void, *const c_void, i32);
            let _ = w.app_handle().run_on_main_thread(move || {
                unsafe {
                    let cls_app = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
                    let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
                    let get_app: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                    let ns_app = get_app(cls_app, sel_shared);
                    if !ns_app.is_null() {
                        let sel_activate = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const c_char);
                        let activate: MsgSendBool = std::mem::transmute(objc_msgSend as *const ());
                        activate(ns_app, sel_activate, 1i32);
                    }
                }
            });
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn register_hotkeys_cmd(app: AppHandle, hotkeys: std::collections::HashMap<String, String>) -> Result<(), String> {
    crate::register_hotkeys(&app, &hotkeys);
    Ok(())
}
