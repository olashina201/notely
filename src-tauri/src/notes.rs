//! Post-meeting notes: single generateContent call with JSON response schema.
use crate::gemini;
use crate::state::TranscriptLine;
pub const NOTES_SCHEMA: &str = r#"{"type":"object","properties":{"summary":{"type":"string"},"decisions":{"type":"array","items":{"type":"string"}},"actionItems":{"type":"array","items":{"type":"object","properties":{"text":{"type":"string"},"owner":{"type":"string"}},"required":["text"]}},"followUpEmail":{"type":"string"}},"required":["summary","decisions","actionItems","followUpEmail"]}"#;
pub async fn generate_notes(api_key: &str, model: &str, transcript: &[TranscriptLine], profile: &str) -> Result<serde_json::Value, String> {
    let full: String = transcript.iter().map(|l| format!("[{}] {}", l.speaker, l.text)).collect::<Vec<_>>().join("\n");
    let system = format!("You generate structured meeting notes. Profile context: {profile}. Return ONLY valid JSON matching the schema: summary (2-4 sentences), decisions (list), actionItems (list of {{text, owner}}), followUpEmail (ready-to-send email body).");
    let mut body = gemini::build_contents(&system, &full, "Generate the meeting notes JSON now.", &[]);
    let schema: serde_json::Value = serde_json::from_str(NOTES_SCHEMA).unwrap();
    let mut cfg = gemini::generation_config("smart", "high", true);
    cfg["responseSchema"] = schema;
    body["generationConfig"] = cfg;
    let url = gemini::generate_url(model, api_key);
    let mut attempt: u32 = 0;
    loop {
        let resp = reqwest::Client::new().post(&url).json(&body).send().await.map_err(|e| format!("notes request failed: {e}"))?;
        if resp.status().as_u16() == 429 { attempt += 1; if attempt > 5 { return Err("rate limited (429)".into()); } tokio::time::sleep(std::time::Duration::from_millis(500 * 2u64.pow(attempt.min(4)))).await; continue; }
        if !resp.status().is_success() { let b = resp.text().await.unwrap_or_default(); return Err(format!("notes failed: {}", &b[..b.len().min(300)])); }
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("notes parse failed: {e}"))?;
        let text = v.pointer("/candidates/0/content/parts/0/text").and_then(|t| t.as_str()).ok_or("empty notes response")?;
        return serde_json::from_str(text).map_err(|e| format!("notes JSON parse failed: {e}"));
    }
}
