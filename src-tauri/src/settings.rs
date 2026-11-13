use crate::state::Settings;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use crate::providers::Provider;

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
                "assistant_provider", "openai_fast_model", "openai_smart_model", "claude_fast_model", "claude_smart_model",
                "live_model", "fast_model", "smart_model", "language", "overlay_opacity",
                "content_protection", "click_through", "save_captures", "thinking_level",
                "search_grounding", "auto_attach_screenshot", "instruction_profile", "custom_instructions", "profile_instructions",
            ] {
                if let Ok(Some(v)) = crate::db::get_setting(&db, key) {
                    match key {
                        "assistant_provider" => s.assistant_provider = serde_json::from_value(serde_json::json!(v)).unwrap_or_default(),
                        "openai_fast_model" => s.openai_fast_model = v,
                        "openai_smart_model" => s.openai_smart_model = v,
                        "claude_fast_model" => s.claude_fast_model = v,
                        "claude_smart_model" => s.claude_smart_model = v,
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
                        "profile_instructions" => s.profile_instructions = serde_json::from_str(&v).unwrap_or_default(),
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
    save_settings_to_db(&db, s)
}

fn save_settings_to_db(db: &rusqlite::Connection, s: &Settings) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let bool = |b: bool| if b { "1" } else { "0" }.to_string();
    let pairs = [
        ("assistant_provider", s.assistant_provider.id().to_string()),
        ("openai_fast_model", s.openai_fast_model.clone()),
        ("openai_smart_model", s.openai_smart_model.clone()),
        ("claude_fast_model", s.claude_fast_model.clone()),
        ("claude_smart_model", s.claude_smart_model.clone()),
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
        ("profile_instructions", serde_json::to_string(&s.profile_instructions).map_err(|e| e.to_string())?),
    ];
    for (k, v) in pairs {
        crate::db::set_setting(&tx, k, &v).map_err(|e| e.to_string())?;
    }
    crate::db::set_hotkeys(&tx, &s.hotkeys).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// API key lives in the OS keychain, never in settings or SQLite.
const KEYCHAIN_SERVICE: &str = "com.notely.app";
fn entry(provider: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("{}-api-key", provider.id())).map_err(|e| format!("keychain error: {e}"))
}

// Session-only caching: approve Keychain access once per provider per process.
// This never writes credentials into SQLite or the frontend.
fn credential_cache(provider: Provider) -> &'static Mutex<Option<String>> {
    static CACHES: OnceLock<[Mutex<Option<String>>; 3]> = OnceLock::new();
    let index = match provider { Provider::Gemini => 0, Provider::Openai => 1, Provider::Claude => 2 };
    &CACHES.get_or_init(|| std::array::from_fn(|_| Mutex::new(None)))[index]
}

fn cached_key(cache: &Mutex<Option<String>>, load: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let mut cached = cache.lock().map_err(|_| "Credential cache lock failed")?;
    if let Some(key) = cached.as_ref() { return Ok(key.clone()); }
    let key = load()?;
    *cached = Some(key.clone());
    Ok(key)
}

pub fn key_get(provider: Provider) -> Result<String, String> {
    cached_key(credential_cache(provider), || {
        entry(provider)?.get_password().map_err(|e| format!("no API key stored: {e}"))
    })
}

pub fn key_set(provider: Provider, key: &str) -> Result<(), String> {
    if key.trim().is_empty() { return Err("API key cannot be empty".into()); }
    let mut cached = credential_cache(provider).lock().map_err(|_| "Credential cache lock failed")?;
    entry(provider)?.set_password(key.trim()).map_err(|e| format!("keychain store failed: {e}"))?;
    *cached = Some(key.trim().to_string());
    Ok(())
}

pub fn key_clear(provider: Provider) -> Result<(), String> {
    let mut cached = credential_cache(provider).lock().map_err(|_| "Credential cache lock failed")?;
    // Remove the session copy even if the persistent delete needs OS approval.
    *cached = None;
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_instructions_are_saved_to_sqlite_without_losing_legacy_preferences() {
        let db = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let mut settings = Settings::default();
        settings.custom_instructions = "Use short paragraphs.".into();
        settings.profile_instructions.insert("coding".into(), "Rust and React.".into());
        settings.profile_instructions.insert("writing".into(), "Warm, professional tone.".into());
        save_settings_to_db(&db, &settings).unwrap();
        let saved: std::collections::HashMap<String,String> = serde_json::from_str(&crate::db::get_setting(&db,"profile_instructions").unwrap().unwrap()).unwrap();
        assert_eq!(saved.get("coding").unwrap(), "Rust and React.");
        assert_eq!(saved.get("writing").unwrap(), "Warm, professional tone.");
        assert_eq!(crate::db::get_setting(&db,"custom_instructions").unwrap().unwrap(), "Use short paragraphs.");
        assert!(crate::db::get_setting(&db,"api_key").unwrap().is_none());
    }

    #[test]
    fn repeated_requests_load_keychain_only_once_and_errors_are_retryable() {
        let cache = Mutex::new(None);
        assert!(cached_key(&cache, || Err("denied".into())).is_err());
        assert_eq!(cached_key(&cache, || Ok("test-key".into())).unwrap(), "test-key");
        assert_eq!(cached_key(&cache, || panic!("must not query Keychain again")).unwrap(), "test-key");
        *cache.lock().unwrap() = None;
        assert_eq!(cached_key(&cache, || Ok("new-key".into())).unwrap(), "new-key");
    }
}
