use serde::{Deserialize, Serialize};

/// Default Gemini models; users can select available IDs in Settings.
pub const FALLBACK_LIVE_MODEL: &str = "gemini-2.0-flash-live-001";
pub const FALLBACK_FAST_MODEL: &str = "gemini-3.1-flash-lite";
pub const FALLBACK_SMART_MODEL: &str = "gemini-3.1-flash-lite";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub methods: Vec<String>,
}

/// Build the multimodal generateContent body shared by Assist and notes.
pub fn build_contents(
    system: &str,
    transcript_window: &str,
    question: &str,
    images_b64: &[String],
) -> serde_json::Value {
    let mut parts: Vec<serde_json::Value> = Vec::new();
    let ctx = format!(
        "Conversation transcript (most recent last):\n{transcript_window}\n\nUser request: {question}"
    );
    parts.push(serde_json::json!({ "text": ctx }));
    for img in images_b64 {
        parts.push(serde_json::json!({
            "inlineData": { "mimeType": "image/jpeg", "data": img }
        }));
    }
    serde_json::json!({
        "systemInstruction": { "role": "user", "parts": [{ "text": system }] },
        "contents": [{ "role": "user", "parts": parts }],
    })
}

/// generationConfig for a slot. Fast = no thinkingConfig (avoids HTTP 400), Smart = more thinking.
pub fn generation_config(model: &str, slot: &str, thinking_level: &str, json_mode: bool) -> serde_json::Value {
    // Let the selected model use its own reasoning/sampling defaults. In particular,
    // the old "low" setting must not send thinkingBudget: 0 and disable reasoning.
    let mut cfg = serde_json::json!({});
    if slot == "smart" && thinking_level == "high" {
        if model.starts_with("gemini-3") {
            cfg["thinkingConfig"] = serde_json::json!({ "thinkingLevel": "HIGH" });
        } else if model.starts_with("gemini-2.5") {
            cfg["thinkingConfig"] = serde_json::json!({ "thinkingBudget": -1 });
        }
    }
    if json_mode {
        cfg["responseMimeType"] = serde_json::json!("application/json");
    }
    cfg
}

/// POST body tool for optional Google Search grounding.
pub fn grounding_tool(enabled: bool) -> Option<serde_json::Value> {
    enabled.then(|| serde_json::json!([{ "googleSearch": {} }]))
}

/// Extract text chunks from one SSE `data:` JSON line of streamGenerateContent.
pub fn extract_stream_text(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return out;
    };
    if let Some(cands) = v.get("candidates").and_then(|c| c.as_array()) {
        for c in cands {
            if let Some(parts) = c.get("content").and_then(|x| x.get("parts")).and_then(|p| p.as_array()) {
                for p in parts {
                    // Ignore internal thought reasoning tokens; only emit final answer text.
                    if p.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                        continue;
                    }
                    if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                        if !t.is_empty() {
                            out.push(t.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}
