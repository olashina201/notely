use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::providers::Provider;

/// App settings persisted to SQLite. The Gemini API key is NOT here — it lives in the OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub assistant_provider: Provider,
    #[serde(default = "default_openai_fast")]
    pub openai_fast_model: String,
    #[serde(default = "default_openai_smart")]
    pub openai_smart_model: String,
    #[serde(default = "default_claude_fast")]
    pub claude_fast_model: String,
    #[serde(default = "default_claude_smart")]
    pub claude_smart_model: String,
    pub live_model: String,
    pub fast_model: String,
    pub smart_model: String,
    pub language: String,
    pub overlay_opacity: f64,
    pub content_protection: bool,
    pub click_through: bool,
    pub save_captures: bool,
    pub thinking_level: String,
    pub search_grounding: bool,
    pub auto_attach_screenshot: bool,
    pub instruction_profile: String,
    pub custom_instructions: String,
    #[serde(default)]
    pub profile_instructions: HashMap<String, String>,
    pub hotkeys: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut hotkeys = HashMap::new();
        hotkeys.insert("assist".into(), "CommandOrControl+Enter".into());
        hotkeys.insert("focusChat".into(), "CommandOrControl+Shift+Space".into());
        hotkeys.insert("toggleStealth".into(), "CommandOrControl+Shift+X".into());
        hotkeys.insert("captureFull".into(), "CommandOrControl+Shift+S".into());
        hotkeys.insert("captureRegion".into(), "CommandOrControl+Shift+A".into());
        hotkeys.insert("panicHide".into(), "CommandOrControl+Shift+H".into());
        hotkeys.insert("toggleClickThrough".into(), "CommandOrControl+Shift+T".into());
        hotkeys.insert("say".into(), "CommandOrControl+Shift+1".into());
        hotkeys.insert("followup".into(), "CommandOrControl+Shift+2".into());
        hotkeys.insert("recap".into(), "CommandOrControl+Shift+3".into());
        hotkeys.insert("screen".into(), "CommandOrControl+Shift+4".into());
        Self {
            assistant_provider: Provider::Gemini,
            openai_fast_model: default_openai_fast(),
            openai_smart_model: default_openai_smart(),
            claude_fast_model: default_claude_fast(),
            claude_smart_model: default_claude_smart(),
            live_model: crate::gemini::FALLBACK_LIVE_MODEL.into(),
            fast_model: crate::gemini::FALLBACK_FAST_MODEL.into(),
            smart_model: crate::gemini::FALLBACK_SMART_MODEL.into(),
            language: "en-US".into(),
            overlay_opacity: 0.95,
            content_protection: true,
            click_through: false,
            save_captures: false,
            thinking_level: "low".into(),
            search_grounding: false,
            auto_attach_screenshot: true,
            instruction_profile: "general".into(),
            custom_instructions: String::new(),
            profile_instructions: HashMap::new(),
            hotkeys,
        }
    }
}

fn default_openai_fast() -> String { "gpt-4.1-mini".into() }
fn default_openai_smart() -> String { "gpt-4.1".into() }
fn default_claude_fast() -> String { "claude-haiku-4-5-20251001".into() }
fn default_claude_smart() -> String { "claude-sonnet-4-6".into() }

impl Settings {
    pub fn instructions_for(&self, profile: &str) -> String {
        let specific = self.profile_instructions.get(profile).map(String::as_str).unwrap_or("");
        [self.custom_instructions.trim(), specific.trim()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n")
    }

    pub fn assistant_model(&self, smart: bool) -> &str {
        match (self.assistant_provider, smart) {
            (Provider::Gemini, false) => &self.fast_model,
            (Provider::Gemini, true) => &self.smart_model,
            (Provider::Openai, false) => &self.openai_fast_model,
            (Provider::Openai, true) => &self.openai_smart_model,
            (Provider::Claude, false) => &self.claude_fast_model,
            (Provider::Claude, true) => &self.claude_smart_model,
        }
    }
}

/// One transcript line produced by the Live API input transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLine {
    pub id: String,
    pub speaker: String,
    pub text: String,
    pub is_final: bool,
    pub started_at: i64,
}

/// A screenshot held in memory as context for Assist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedImage {
    pub data_base64: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub taken_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistRequest {
    pub request_id: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub question: String,
    pub preset: Option<String>,
    pub include_screenshot: bool,
    pub profile_id: String,
    pub custom_instructions: Option<String>,
    pub slot: String,
}

/// Events emitted from Rust to the webview.
pub mod events {
    pub const TRANSCRIPT_PARTIAL: &str = "transcript:partial";
    pub const TRANSCRIPT_FINAL: &str = "transcript:final";
    pub const TRANSCRIPT_STATUS: &str = "transcript:status";
    pub const CAPTURE_TAKEN: &str = "capture:taken";
    pub const ASSIST_CHUNK: &str = "assist:chunk";
    pub const ASSIST_DONE: &str = "assist:done";
    pub const STEALTH_CHANGED: &str = "stealth:changed";
    pub const ERROR: &str = "error";
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_load_and_provider_models_stay_independent() {
        let mut legacy = serde_json::to_value(Settings::default()).unwrap();
        for field in ["assistantProvider", "openaiFastModel", "openaiSmartModel", "claudeFastModel", "claudeSmartModel", "profileInstructions"] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["fastModel"] = serde_json::json!("my-gemini-model");
        let mut settings: Settings = serde_json::from_value(legacy).unwrap();
        assert_eq!(settings.assistant_provider, Provider::Gemini);
        assert_eq!(settings.assistant_model(false), "my-gemini-model");
        settings.assistant_provider = Provider::Openai;
        assert_eq!(settings.assistant_model(false), "gpt-4.1-mini");
        settings.assistant_provider = Provider::Claude;
        assert_eq!(settings.assistant_model(true), "claude-sonnet-4-6");
        assert_eq!(settings.fast_model, "my-gemini-model");
    }

    #[test]
    fn profile_context_stays_isolated_and_preserves_shared_preferences() {
        let mut settings = Settings::default();
        settings.custom_instructions = "Use British English.".into();
        settings.profile_instructions.insert("coding".into(), "Rust, SQLite, no new dependencies.".into());
        settings.profile_instructions.insert("interview".into(), "I have 5 years of frontend experience.".into());
        let coding = settings.instructions_for("coding");
        assert!(coding.contains("British English") && coding.contains("Rust"));
        assert!(!coding.contains("5 years"));
        assert!(!settings.instructions_for("general").contains("Rust"));
        let roundtrip: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(roundtrip.instructions_for("interview"), settings.instructions_for("interview"));
    }

    #[test]
    fn frontend_events_match_backend_and_pass_tauri_validation() {
        use tauri::{Emitter, Listener};
        let app = tauri::test::mock_app();
        let names: serde_json::Value = serde_json::from_str(include_str!("../../src/events.json")).unwrap();
        for (name, expected) in [
            ("transcriptPartial", events::TRANSCRIPT_PARTIAL), ("transcriptFinal", events::TRANSCRIPT_FINAL),
            ("transcriptStatus", events::TRANSCRIPT_STATUS), ("captureTaken", events::CAPTURE_TAKEN),
            ("assistChunk", events::ASSIST_CHUNK), ("assistDone", events::ASSIST_DONE), ("stealthChanged", events::STEALTH_CHANGED),
        ] { assert_eq!(names[name], expected); }
        for name in names.as_object().unwrap().values() {
            app.emit(name.as_str().unwrap(), serde_json::json!({})).unwrap();
        }
        // Reproduce the original integration failure, using the real Tauri validator.
        assert!(app.emit("assist.chunk", "ignored response").is_err());
        let (tx, rx) = std::sync::mpsc::channel();
        app.listen(events::ASSIST_CHUNK, move |event| { tx.send(event.payload().to_string()).unwrap(); });
        app.emit(events::ASSIST_CHUNK, serde_json::json!({"requestId": "test", "text": "answer"})).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(payload["text"], "answer");
        assert!(rx.try_recv().is_err(), "each response chunk must be delivered once");
    }
}
