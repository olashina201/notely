use serde::{Deserialize, Serialize};

/// Model IDs live here and ONLY here. Google retires and renames models often;
/// Settings model pickers are populated from the live models list endpoint.
pub const FALLBACK_LIVE_MODEL: &str = "gemini-2.0-flash-live-001";
pub const FALLBACK_FAST_MODEL: &str = "gemini-2.5-flash";
pub const FALLBACK_SMART_MODEL: &str = "gemini-3.5-flash";

const REST_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// GET /v1beta/models — used to populate the Settings model pickers.
pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    let url = format!("{REST_BASE}/models?pageSize=100&key={api_key}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("models request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("models list failed ({status}): {t}", t = truncate(&body, 300)));
    }
    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("models parse failed: {e}"))?;
    Ok(body
        .models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.name.trim_start_matches("models/").to_string(),
            display_name: m.display_name.unwrap_or_default(),
            methods: m.supported_generation_methods,
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    models: Vec<RestModel>,
}

#[derive(Debug, Deserialize)]
struct RestModel {
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
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

/// generationConfig for a slot. Fast = low/no thinking, Smart = more thinking.
pub fn generation_config(slot: &str, thinking_level: &str, json_mode: bool) -> serde_json::Value {
    let budget: Option<i32> = match (slot, thinking_level) {
        ("fast", _) => Some(0),
        (_, "off") => Some(0),
        (_, "low") => Some(512),
        (_, "high") => Some(2048),
        _ => None,
    };
    let mut cfg = serde_json::json!({ "temperature": 0.7 });
    if let Some(b) = budget {
        cfg["thinkingConfig"] = serde_json::json!({ "thinkingBudget": b });
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

pub fn stream_url(model: &str, api_key: &str) -> String {
    format!("{REST_BASE}/models/{model}:streamGenerateContent?alt=sse&key={api_key}")
}

pub fn generate_url(model: &str, api_key: &str) -> String {
    format!("{REST_BASE}/models/{model}:generateContent?key={api_key}")
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

/// Extract API error from an SSE `data:` JSON line if present.
pub fn extract_stream_error(line: &str) -> Option<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return None;
    };
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown Gemini API error");
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        return Some(format!("API Error ({code}): {msg}"));
    }
    None
}
