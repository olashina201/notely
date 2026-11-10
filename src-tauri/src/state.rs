use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// App settings persisted to SQLite. The Gemini API key is NOT here — it lives in the OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
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
    pub hotkeys: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut hotkeys = HashMap::new();
        hotkeys.insert("assist".into(), "CommandOrControl+Enter".into());
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
            instruction_profile: "interview".into(),
            custom_instructions: String::new(),
            hotkeys,
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
    pub question: String,
    pub preset: Option<String>,
    pub include_screenshot: bool,
    pub profile_id: String,
    pub custom_instructions: Option<String>,
    pub slot: String,
}

/// Events emitted from Rust to the webview.
pub mod events {
    pub const TRANSCRIPT_PARTIAL: &str = "transcript.partial";
    pub const TRANSCRIPT_FINAL: &str = "transcript.final";
    pub const TRANSCRIPT_STATUS: &str = "transcript.status";
    pub const CAPTURE_TAKEN: &str = "capture.taken";
    pub const ASSIST_CHUNK: &str = "assist.chunk";
    pub const ASSIST_DONE: &str = "assist.done";
    pub const STEALTH_CHANGED: &str = "stealth.changed";
    pub const ERROR: &str = "error";
}
