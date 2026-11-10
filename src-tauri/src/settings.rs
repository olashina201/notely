use crate::state::Settings;
use std::sync::Mutex;
use tauri::AppHandle;

pub struct SettingsStore(pub Mutex<Settings>);

impl SettingsStore {
    pub fn get(&self) -> Settings {
        self.0.lock().unwrap().clone()
    }
    pub fn update(&self, s: Settings) {
        *self.0.lock().unwrap() = s;
    }
    pub fn modify(&self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.0.lock().unwrap());
    }
}

/// Load settings from SQLite (or defaults) at startup.
pub fn load_settings(app: &AppHandle) -> Settings {
    let mut s = Settings::default();
    if let Ok(db_path) = crate::db::db_path(app) {
        if let Ok(db) = crate::db::open(&db_path) {
            for key in [
                "live_model", "fast_model", "smart_model", "language", "overlay_opacity",
                "content_protection", "click_through", "save_captures", "thinking_level",
                "search_grounding", "auto_attach_screenshot", "instruction_profile", "custom_instructions",
            ] {
                if let Ok(Some(v)) = crate::db::get_setting(&db, key) {
                    match key {
                        "live_model" => s.live_model = v,
                        "fast_model" => s.fast_model = v,
                        "smart_model" => s.smart_model = v,
                        "language" => s.language = v,
                        "overlay_opacity" => s.overlay_opacity = v.parse().unwrap_or(s.overlay_opacity),
                        "content_protection" => s.content_protection = v == "1" || v == "true",
                        "click_through" => s.click_through = v == "1" || v == "true",
                        "save_captures" => s.save_captures = v == "1" || v == "true",
                        "thinking_level" => s.thinking_level = v,
                        "search_grounding" => s.search_grounding = v == "1" || v == "true",
                        "auto_attach_screenshot" => s.auto_attach_screenshot = v == "1" || v == "true",
                        "instruction_profile" => s.instruction_profile = v,
                        "custom_instructions" => s.custom_instructions = v,
                        _ => {}
                    }
                }
            }
            if let Ok(hotkeys) = crate::db::get_hotkeys(&db) {
                for (k, v) in hotkeys {
                    s.hotkeys.insert(k, v);
                }
            }
        }
    }
    s
}

/// Persist settings to SQLite.
pub fn save_settings(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let db_path = crate::db::db_path(app).map_err(|e| e.to_string())?;
    let db = crate::db::open(&db_path).map_err(|e| e.to_string())?;
    let bool = |b: bool| if b { "1" } else { "0" }.to_string();
    let pairs = [
        ("live_model", s.live_model.clone()),
        ("fast_model", s.fast_model.clone()),
        ("smart_model", s.smart_model.clone()),
        ("language", s.language.clone()),
        ("overlay_opacity", s.overlay_opacity.to_string()),
        ("content_protection", bool(s.content_protection)),
        ("click_through", bool(s.click_through)),
        ("save_captures", bool(s.save_captures)),
        ("thinking_level", s.thinking_level.clone()),
        ("search_grounding", bool(s.search_grounding)),
        ("auto_attach_screenshot", bool(s.auto_attach_screenshot)),
        ("instruction_profile", s.instruction_profile.clone()),
        ("custom_instructions", s.custom_instructions.clone()),
    ];
    for (k, v) in pairs {
        crate::db::set_setting(&db, k, &v).map_err(|e| e.to_string())?;
    }
    crate::db::set_hotkeys(&db, &s.hotkeys).map_err(|e| e.to_string())?;
    Ok(())
}

/// API key lives in the OS keychain, never in settings or SQLite.
const KEYCHAIN_SERVICE: &str = "com.notely.app";
const KEYCHAIN_USER: &str = "gemini-api-key";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| format!("keychain error: {e}"))
}

pub fn key_has() -> bool {
    entry().ok().and_then(|e| e.get_password().ok()).map(|p| !p.is_empty()).unwrap_or(false)
}

pub fn key_get() -> Result<String, String> {
    let e = entry()?;
    e.get_password().map_err(|e| format!("no API key stored: {e}"))
}

pub fn key_set(key: &str) -> Result<(), String> {
    entry()?.set_password(key).map_err(|e| format!("keychain store failed: {e}"))
}

pub fn key_clear() -> Result<(), String> {
    let e = entry()?;
    match e.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete failed: {e}")),
    }
}
