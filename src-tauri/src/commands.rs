//! Tauri commands: the only bridge between webview UI and the Rust backend.
//! The API key never leaves Rust; the frontend only gets success/failure.
use crate::{assist, audio, db, diagnostics::{self, Trace}, gemini, live, notes, providers::{self, Provider, Prompt}, settings, vision};
use crate::state::{events, AssistRequest, CapturedImage, Settings, TranscriptLine};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
pub struct MeetingState(pub Mutex<MeetingData>);
pub struct MeetingData {
    pub recording: bool,
    pub starting: bool,
    pub meeting_id: Option<i64>,
    pub transcript: Vec<TranscriptLine>,
    pub captures: Vec<CapturedImage>,
    pub live: Option<live::LiveHandle>,
    pub mixer: Option<audio::Mixer>,
    pub started_at: i64,
}
impl Default for MeetingData {
    fn default() -> Self {
        Self { recording: false, starting: false, meeting_id: None, transcript: Vec::new(), captures: Vec::new(), live: None, mixer: None, started_at: 0 }
    }
}
#[tauri::command]
pub async fn settings_get(app: AppHandle, store: State<'_, settings::SettingsStore>) -> Result<Settings, String> {
    let _ = app;
    Ok(store.get())
}
#[tauri::command]
pub async fn settings_set(app: AppHandle, store: State<'_, settings::SettingsStore>, mut s: Settings) -> Result<(), String> {
    // Saving Settings must always leave its navigation clickable.
    s.click_through = false;
    settings::save_settings(&app, &s)?;
    store.update(s.clone());
    apply_window_prefs(&app, &s);
    crate::tray::sync(&app);
    diagnostics::record("info", "settings.saved", None, serde_json::json!({"provider": s.assistant_provider.id()}));
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
pub(crate) fn resolve_key(app: &AppHandle, provider: Provider) -> Result<String, String> {
    if let Ok(k) = settings::key_get(provider) {
        if !k.trim().is_empty() { return Ok(k.trim().to_string()); }
    }
    // Read legacy Gemini installs, migrating out of SQLite only after Keychain succeeds.
    if provider == Provider::Gemini {
        let path = db::db_path(app)?;
        let dbc = db::open(&path).map_err(|e| e.to_string())?;
        if let Some(k) = db::get_setting(&dbc, "_api_key").map_err(|e| e.to_string())? {
            if !k.trim().is_empty() {
                if settings::key_set(provider, k.trim()).is_ok() {
                    dbc.execute("DELETE FROM settings WHERE key = '_api_key'", []).map_err(|e| e.to_string())?;
                }
                return Ok(k.trim().to_string());
            }
        }
    }
    Err(format!("No {} API key set — add it in Settings", provider.id()))
}
async fn resolve_key_async(app: AppHandle, provider: Provider) -> Result<String, String> {
    tokio::time::timeout(std::time::Duration::from_secs(8), tokio::task::spawn_blocking(move || resolve_key(&app, provider)))
        .await.map_err(|_| "OS Keychain did not respond within 8 seconds. Check for a macOS access prompt and retry.")?
        .map_err(|_| "Credential lookup worker failed")?
}
#[tauri::command]
pub async fn key_has(app: AppHandle, provider: Provider) -> Result<bool, String> {
    Ok(resolve_key_async(app, provider).await.is_ok())
}
#[tauri::command]
pub async fn key_set(app: AppHandle, provider: Provider, key: String) -> Result<(), String> {
    diagnostics::record("info", "credentials.save_started", None, serde_json::json!({"provider": provider.id()}));
    tokio::task::spawn_blocking(move || settings::key_set(provider, &key)).await.map_err(|_| "Credential storage worker failed")??;
    diagnostics::record("info", "credentials.saved", None, serde_json::json!({"provider": provider.id()}));
    if provider == Provider::Gemini {
        let dbc = db::open(&db::db_path(&app)?).map_err(|e| e.to_string())?;
        dbc.execute("DELETE FROM settings WHERE key = '_api_key'", []).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub async fn key_clear(app: AppHandle, provider: Provider) -> Result<(), String> {
    tokio::task::spawn_blocking(move || settings::key_clear(provider)).await.map_err(|_| "Credential removal worker failed")??;
    diagnostics::record("info", "credentials.removed", None, serde_json::json!({"provider": provider.id()}));
    if provider == Provider::Gemini {
        let dbc = db::open(&db::db_path(&app)?).map_err(|e| e.to_string())?;
        dbc.execute("DELETE FROM settings WHERE key = '_api_key'", []).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub async fn key_test(app: AppHandle, provider: Provider, model: String) -> Result<String, String> {
    diagnostics::record("info", "connection_test.started", None, serde_json::json!({"provider": provider.id(), "model": model}));
    let key = resolve_key_async(app.clone(), provider).await?;
    validate_model(&model)?;
    let started = std::time::Instant::now();
    providers::complete(provider, &key, &Prompt {
        model: &model, system: "Reply briefly.", context: "", question: "Reply with OK.",
        images: &[], smart: false, thinking: "off", grounding: false, json: false,
    }).await?;
    Ok(format!("{} / {} responded in {:.1}s", provider.id(), model, started.elapsed().as_secs_f32()))
}
#[tauri::command]
pub async fn models_list(app: AppHandle, provider: Provider) -> Result<Vec<gemini::ModelInfo>, String> {
    let key = resolve_key_async(app.clone(), provider).await?;
    providers::list_models(provider, &key).await
}
fn validate_model(model: &str) -> Result<(), String> {
    if model.is_empty() || !model.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._:".contains(&c)) {
        return Err("Enter a valid model ID in Settings".into());
    }
    Ok(())
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPermission { granted: bool, platform: &'static str }
fn screen_permission_status() -> ScreenPermission {
    ScreenPermission { granted: vision::screen_permission(), platform: std::env::consts::OS }
}
#[tauri::command]
pub fn screen_permission_get() -> ScreenPermission { screen_permission_status() }
#[tauri::command]
pub async fn screen_permission_request(app: AppHandle) -> Result<ScreenPermission, String> {
    if vision::screen_permission() { return Ok(screen_permission_status()); }
    crate::overlay::set_hidden(&app, true)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || { let _ = sender.send(vision::request_screen_permission()); }).map_err(|e| e.to_string())?;
    let granted = tokio::time::timeout(std::time::Duration::from_secs(30), receiver).await
        .map_err(|_| "macOS is waiting for a Screen Recording permission decision. Check System Settings.")?
        .map_err(|_| "Screen Recording permission request failed")?;
    if granted { crate::overlay::set_hidden(&app, false)?; }
    diagnostics::record("info", "capture.permission_requested", None, serde_json::json!({"granted": granted}));
    Ok(screen_permission_status())
}
#[tauri::command]
pub async fn screen_permission_open_settings(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::overlay::set_hidden(&app, true)?;
        tokio::task::spawn_blocking(|| std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture").status())
            .await.map_err(|_| "Could not open macOS Settings")?.map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = app; Err("Open your operating system's screen capture permissions manually".into()) }
}

pub(crate) async fn capture_with_timeout<F>(capture: F) -> Result<CapturedImage, String>
where F: FnOnce() -> Result<CapturedImage, String> + Send + 'static {
    vision::ensure_screen_permission()?;
    diagnostics::record("info", "capture.worker_started", None, serde_json::json!({}));
    let result = tokio::time::timeout(std::time::Duration::from_secs(12), tokio::task::spawn_blocking(capture)).await
        .map_err(|_| "Screen capture timed out after 12 seconds. Check Screen Recording permission in Settings.")?
        .map_err(|_| "Screen capture worker failed")?;
    diagnostics::record(if result.is_ok() { "info" } else { "error" }, "capture.worker_finished", None, serde_json::json!({"success": result.is_ok()}));
    result
}
#[tauri::command]
pub async fn capture_full_cmd(app: AppHandle, monitor_index: Option<usize>, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<CapturedImage, String> {
    let img = capture_with_timeout(move || vision::capture_full(monitor_index)).await?;
    push_capture(&app, &store, &meeting, img.clone());
    Ok(img)
}
#[tauri::command]
pub async fn capture_active_cmd(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<CapturedImage, String> {
    let img = capture_with_timeout(vision::capture_active_window).await?;
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
    vision::ensure_screen_permission()?;
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
    crate::overlay::set_hidden(&app, false)?;
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
    let img = capture_with_timeout(move || vision::capture_region(x, y, w, h)).await?;
    push_capture(&app, &store, &meeting, img.clone());
    Ok(img)
}
struct MeetingStartGuard(AppHandle);
#[tauri::command]
pub async fn transcription_test(app: AppHandle, provider: crate::state::TranscriptionProvider, model: String, language: String) -> Result<String, String> {
    validate_model(&model)?;
    let key = resolve_key_async(app, provider.credentials()).await?;
    let started = std::time::Instant::now();
    diagnostics::record("info", "audio.connection_test_started", None, serde_json::json!({"provider":provider.credentials().id(),"model":model}));
    let config = live::LiveConfig { provider, api_key: key, model, language, system_audio: false };
    if let Err(error) = live::test_connection(&config).await {
        diagnostics::record("error", "audio.connection_test_failed", None, serde_json::json!({"provider":provider.credentials().id(),"failureKind":diagnostics::failure_kind(&error),"elapsedMs":started.elapsed().as_millis() as u64}));
        return Err(error);
    }
    diagnostics::record("info", "audio.connection_test_finished", None, serde_json::json!({"provider":provider.credentials().id(),"elapsedMs":started.elapsed().as_millis() as u64,"success":true}));
    Ok("Transcription connection ready. No microphone or screen content was sent.".into())
}
impl Drop for MeetingStartGuard {
    fn drop(&mut self){if let Some(state)=self.0.try_state::<MeetingState>(){if let Ok(mut m)=state.0.lock(){m.starting=false;}}}
}
#[tauri::command]
pub async fn meeting_start(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>) -> Result<serde_json::Value, String> {
    {
        let mut m = meeting.0.lock().map_err(|_| "state lock failed")?;
        if m.recording || m.starting { return Err("A meeting is already recording or starting".into()); }
        m.starting=true;
    }
    let _starting=MeetingStartGuard(app.clone());
    diagnostics::record("info", "meeting.start_requested", None, serde_json::json!({}));
    let s = store.get();
    let key = resolve_key_async(app.clone(), s.transcription_provider.credentials()).await?;
    let model = if s.transcription_provider == crate::state::TranscriptionProvider::Openai { s.openai_transcription_model.clone() } else { s.live_model.clone() };
    validate_model(&model)?;
    let started_at = chrono::Utc::now().timestamp_millis();
    // Mixer tags mic "me" + system "other" and forwards into the Live session.
    // System audio may be absent (stock macOS): meeting still runs, mic-only.
    let (live_tx, live_rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<i16>)>();
    let (mixer, sys_warning) = audio::Mixer::start(live_tx.clone()).map_err(|e| format!("audio start failed: {e}"))?;
    if let Some(w) = &sys_warning {
        let _ = app.emit(events::ERROR, serde_json::json!({"message": w}));
    }
    let meeting_id = match (|| -> Result<i64,String> {
        let path=db::db_path(&app)?;let db=db::open(&path).map_err(|e|e.to_string())?;
        db::insert_meeting(&db,"Meeting",&s.instruction_profile,started_at).map_err(|e|e.to_string())
    })() {Ok(id)=>id,Err(error)=>{mixer.stop();return Err(error);}};
    {
        let mut m=meeting.0.lock().map_err(|_|"Meeting state lock failed")?;
        m.recording=true;m.meeting_id=Some(meeting_id);m.started_at=started_at;m.transcript.clear();
    }
    let _=app.emit(events::TRANSCRIPT_STATUS,serde_json::json!({"status":"connecting"}));
    let live=match live::spawn_live_session(app.clone(),live::LiveConfig{provider:s.transcription_provider,api_key:key,model,language:s.language.clone(),system_audio:sys_warning.is_none()},live_rx,live_tx) {
        Ok(live)=>live,
        Err(error)=>{
            mixer.stop();
            if let Ok(mut m)=meeting.0.lock(){m.recording=false;m.meeting_id=None;}
            if let Ok(path)=db::db_path(&app){if let Ok(db)=db::open(&path){let _=db::delete_meeting(&db,meeting_id);}}
            return Err(error);
        }
    };
    {let mut m=meeting.0.lock().map_err(|_|"Meeting state lock failed")?;m.mixer=Some(mixer);m.live=Some(live);}
    Ok(serde_json::json!({"systemAudio":sys_warning.is_none(),"transcriptionProvider":s.transcription_provider.credentials().id()}))
}
#[tauri::command]
pub async fn meeting_transcript_add(app:AppHandle, line:TranscriptLine) -> Result<(),String> {
    // Compatibility with older webviews. Native transcription is the primary writer.
    persist_transcript_line(&app,&line)
}
pub fn persist_transcript_line(app:&AppHandle,line:&TranscriptLine) -> Result<(),String> {
    if line.text.trim().is_empty(){return Ok(());}
    let state=app.state::<MeetingState>();let mut m=state.0.lock().map_err(|_|"Meeting state lock failed")?;
    if !m.recording{return Ok(());}
    if let Some(existing)=m.transcript.iter_mut().find(|item|item.id==line.id){
        if existing.is_final && (!line.is_final || existing.text==line.text){return Ok(());}
        *existing=line.clone();
    }else{m.transcript.push(line.clone());}
    m.transcript.sort_by_key(|line| line.started_at);
    if let Some(id)=m.meeting_id {
        // Keep snapshot writes ordered across mic/system and compatibility callbacks.
        let json=serde_json::to_string(&m.transcript).map_err(|e|e.to_string())?;
        let path=db::db_path(app)?;let db=db::open(&path).map_err(|e|e.to_string())?;
        db::checkpoint_meeting(&db,id,&json).map_err(|e|e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub async fn meeting_stop(app:AppHandle,meeting:State<'_,MeetingState>)->Result<serde_json::Value,String>{
    let (mixer,live)={let mut m=meeting.0.lock().map_err(|_|"Meeting state lock failed")?;if m.starting{return Err("Meeting is still starting".into());}(m.mixer.take(),m.live.take())};
    // Join off the async command executor, allowing final native transcript events to save.
    tokio::task::spawn_blocking(move||{if let Some(mixer)=mixer{mixer.stop();}if let Some(live)=live{live.stop();}}).await.map_err(|_|"Meeting shutdown failed")?;
    let (id,transcript)={let mut m=meeting.0.lock().map_err(|_|"Meeting state lock failed")?;m.recording=false;(m.meeting_id,m.transcript.clone())};
    let path=db::db_path(&app)?;let db=db::open(&path).map_err(|e|e.to_string())?;
    if transcript.is_empty(){if let Some(id)=id{db::delete_meeting(&db,id).map_err(|e|e.to_string())?;}meeting.0.lock().map_err(|_|"Meeting state lock failed")?.meeting_id=None;}
    else if let Some(id)=id{let json=serde_json::to_string(&transcript).map_err(|e|e.to_string())?;db::finish_meeting(&db,id,chrono::Utc::now().timestamp_millis(),&json,None).map_err(|e|e.to_string())?;}
    let _=app.emit(events::TRANSCRIPT_STATUS,serde_json::json!({"status":"idle"}));
    diagnostics::record("info","meeting.saved",None,serde_json::json!({"lines":transcript.len()}));
    Ok(serde_json::json!({"saved":!transcript.is_empty(),"empty":transcript.is_empty(),"lines":transcript.len()}))
}
#[tauri::command]
pub async fn notes_generate(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>, meeting_id: Option<i64>) -> Result<serde_json::Value, String> {
    let s = store.get();
    let key = resolve_key_async(app.clone(), s.assistant_provider).await?;
    validate_model(s.assistant_model(true))?;
    let (active_id,recording,current)={let m=meeting.0.lock().map_err(|_|"Meeting state lock failed")?;(m.meeting_id,m.recording,m.transcript.clone())};
    let requested=meeting_id.or(active_id);
    if recording && requested==active_id{return Err("End the meeting before generating notes.".into());}
    let (id,transcript,profile)=if let Some(id)=requested{
        let path=db::db_path(&app)?;let db=db::open(&path).map_err(|e|e.to_string())?;
        let row=db::get_meeting(&db,id).map_err(|e|e.to_string())?.ok_or("Saved meeting not found")?;
        (Some(id),serde_json::from_str::<Vec<TranscriptLine>>(&row.transcript_json).map_err(|_|"Saved transcript is invalid")?,row.profile_id)
    }else{(active_id,current,s.instruction_profile.clone())};
    if transcript.is_empty() { return Err("no transcript to summarize".into()); }
    let notes = notes::generate_notes(s.assistant_provider, &key, s.assistant_model(true), &transcript, &format!("{}\n{}",profile,s.instructions_for(&profile))).await?;
    if let Some(id) = id {
        let path = db::db_path(&app)?;
        let dbc = db::open(&path).map_err(|e| e.to_string())?;
        let nj=serde_json::to_string(&notes).map_err(|e|e.to_string())?;
        db::save_meeting_notes(&dbc,id,&nj).map_err(|e|e.to_string())?;
    }
    Ok(notes)
}
#[tauri::command]
pub async fn assist_run(app: AppHandle, store: State<'_, settings::SettingsStore>, meeting: State<'_, MeetingState>, req: AssistRequest) -> Result<(), String> {
    if req.request_id.is_empty() { return Err("Missing request ID".into()); }
    let s = store.get();
    let mut req = req;
    req.custom_instructions = Some(s.instructions_for(&req.profile_id));
    let provider = s.assistant_provider;
    let model = s.assistant_model(req.slot == "smart").to_string();
    let id = req.request_id.clone();
    let conversation_id = req.conversation_id.clone().unwrap_or_else(|| id.clone());
    let saved_profile = req.profile_id.clone();
    let saved_question=if req.question.trim().is_empty(){req.preset.as_deref().and_then(assist::action).map(|p|p.label.clone()).unwrap_or_else(||"New question".into())}else if req.preset.as_deref().and_then(assist::action).is_some_and(|p|p.needs_topic){format!("Revisit a topic: {}",req.question.trim())}else{req.question.clone()};
    let trace = Trace::new(&id, provider.id(), &model);
    trace.log("info", "request.received", serde_json::json!({"slot": req.slot, "includeScreenshot": req.include_screenshot}));
    validate_model(&model)?;
    let is_chat_action=req.preset.as_deref().and_then(assist::action).is_some_and(|p|p.scope=="chat");
    if is_chat_action {req.include_meeting=false;req.include_screenshot=false;}
    let has_chat=if is_chat_action {let path=db::db_path(&app)?;let db=db::open(&path).map_err(|e|e.to_string())?;db.query_row("SELECT EXISTS(SELECT 1 FROM chat_turns WHERE conversation_id=?1)",[&conversation_id],|row|row.get::<_,bool>(0)).map_err(|e|e.to_string())?}else{false};
    let (transcript, existing_imgs) = {
        let m = meeting.0.lock().map_err(|_| "state lock failed")?;
        assist::validate_action(req.preset.as_deref(),&req.question,m.recording,req.include_meeting,m.transcript.iter().any(|line|!line.text.trim().is_empty()),has_chat)?;
        (if req.include_meeting {m.transcript.clone()}else{Vec::new()}, if req.include_screenshot { m.captures.clone() } else { Vec::new() })
    };
    let overall_limit = if req.include_screenshot { 110 } else { 75 };
    let cancel = assist::begin_request(&id);
    tauri::async_runtime::spawn(async move {
        let result = assist::run_bounded(cancel, std::time::Duration::from_secs(overall_limit), async {
            trace.log("info", "credentials.lookup_started", serde_json::json!({}));
            let key = match resolve_key_async(app.clone(), provider).await {
                Ok(key) => key,
                Err(error) => {
                    trace.log("error", "credentials.lookup_failed", serde_json::json!({"kind": diagnostics::failure_kind(&error)}));
                    return Err(error);
                }
            };
            trace.log("info", "credentials.lookup_finished", serde_json::json!({"available": true}));
            let mut images = existing_imgs;
            if req.include_screenshot && images.is_empty() && (s.auto_attach_screenshot || req.preset.as_deref()==Some("screen")) {
                trace.log("info", "capture.started", serde_json::json!({}));
                images.push(capture_with_timeout(|| vision::capture_full(None)).await?);
                trace.log("info", "capture.finished", serde_json::json!({"images": images.len()}));
            }
            let recent: Vec<_> = images.into_iter().rev().take(4).rev().collect();
            let original_bytes = recent.iter().map(|i| i.data_base64.len() * 3 / 4).sum::<usize>();
            let smart = req.slot == "smart";
            let recent = tokio::task::spawn_blocking(move || recent.into_iter().map(|image| vision::prepare_for_assist(image, smart)).collect::<Result<Vec<_>, String>>())
                .await.map_err(|_| "Screenshot preparation worker failed")??;
            trace.log("info", "capture.prepared", serde_json::json!({"images": recent.len(), "imageBytesBefore": original_bytes, "imageBytes": recent.iter().map(|i| i.data_base64.len() * 3 / 4).sum::<usize>()}));
            assist::run_assist(app.clone(), provider, key, model.clone(), req, transcript, recent, s.thinking_level, s.search_grounding, &trace).await
        }).await;
        let payload = match result {
            Ok(Some(answer)) => {
                let saved = (|| -> Result<(), String> {
                    let path = db::db_path(&app)?;
                    let dbc = db::open(&path).map_err(|e| e.to_string())?;
                    db::save_chat_turn(&dbc, &conversation_id, &db::ChatTurn {
                        request_id: id.clone(), question: saved_question, answer,
                        profile_id: saved_profile, provider: provider.id().into(), model: model.clone(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                    }).map_err(|e| e.to_string())
                })();
                if saved.is_err() { trace.log("error", "history.save_failed", serde_json::json!({})); }
                trace.log("info", "request.completed", serde_json::json!({}));
                serde_json::json!({"requestId": id, "historySaved": saved.is_ok()})
            }
            Ok(None) => {
                trace.log("info", "request.cancelled", serde_json::json!({}));
                serde_json::json!({"requestId": id, "cancelled": true})
            }
            Err(error) => {
                trace.log("error", "request.failed", serde_json::json!({"kind": diagnostics::failure_kind(&error)}));
                serde_json::json!({"requestId": id, "error": error})
            }
        };
        assist::emit_all(&app, events::ASSIST_DONE, payload);
        assist::finish_request(&id);
    });
    Ok(())
}
#[tauri::command]
pub async fn assist_cancel(request_id: String) -> Result<(), String> { assist::cancel_request(&request_id); Ok(()) }
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
    crate::overlay::set_hidden(&app, true)
}
#[tauri::command]
pub async fn overlay_show(app: AppHandle) -> Result<(), String> {
    crate::overlay::set_hidden(&app, false)
}
#[tauri::command]
pub async fn overlay_toggle_clickthrough(app: AppHandle) -> Result<bool, String> {
    crate::overlay::toggle_clickthrough(&app)
}
#[tauri::command]
pub async fn overlay_interactive(app: AppHandle) -> Result<(), String> {
    crate::overlay::set_clickthrough(&app, false)
}
#[tauri::command]
pub async fn overlay_toggle_stealth(app: AppHandle) -> Result<bool, String> {
    crate::overlay::toggle_protection(&app)
}
#[tauri::command]
pub async fn overlay_set_stealth(app: AppHandle, enabled: bool) -> Result<bool, String> {
    crate::overlay::set_protection(&app, enabled)
}

#[tauri::command]
pub async fn overlay_focus(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focus();
        #[cfg(target_os = "macos")]
        {
            if let Ok(ptr) = w.ns_window() {
                if !ptr.is_null() {
                    let ptr_val = ptr as usize;
                    let _ = w.app_handle().run_on_main_thread(move || {
                        unsafe {
                            crate::notely_focus_window(ptr_val as *mut std::ffi::c_void);
                        }
                    });
                }
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn register_hotkeys_cmd(app: AppHandle, hotkeys: std::collections::HashMap<String, String>) -> Result<(), String> {
    crate::register_hotkeys(&app, &hotkeys);
    Ok(())
}

#[tauri::command]
pub async fn chats_list(app: AppHandle, query: Option<String>) -> Result<Vec<db::ChatSummary>, String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::list_chats(&dbc, query.as_deref().unwrap_or("")).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn chats_get(app: AppHandle, id: String) -> Result<Vec<db::ChatTurn>, String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::chat_turns(&dbc, &id).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn chats_delete(app: AppHandle, id: String) -> Result<(), String> {
    let path = db::db_path(&app)?;
    let dbc = db::open(&path).map_err(|e| e.to_string())?;
    db::delete_chat(&dbc, &id).map_err(|e| e.to_string())
}
