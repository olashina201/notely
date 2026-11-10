//! Assist: streaming generateContent with transcript + screenshots.
use crate::gemini;
use crate::state::{events, AssistRequest, CapturedImage, TranscriptLine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};


static CANCEL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

pub fn cancel_current() {
    if let Ok(guard) = CANCEL.lock() {
        if let Some(flag) = guard.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

fn set_cancel(flag: Arc<AtomicBool>) {
    if let Ok(mut guard) = CANCEL.lock() {
        *guard = Some(flag);
    }
}

pub const PROFILES: &[(&str, &str)] = &[
    ("interview", "You are helping the interviewee in a live job interview. Give concise, confident, natural-sounding answers they can say out loud. Prefer short bullet points and a quotable one-liner first. If shown code, give the working solution plus a one-sentence explanation."),
    ("sales", "You are helping a salesperson on a live call. Suggest sharp discovery questions, objection handling, and crisp value statements. Keep answers short enough to use mid-call."),
    ("lecture", "You are helping a student follow a live lecture. Explain concepts clearly, define jargon, and flag likely exam points. Use simple formatting and small worked examples."),
    ("coding", "You are helping an engineer in a live technical discussion. Give correct, runnable code first, then a brief explanation. Call out edge cases and complexity."),
];

pub const PRESETS: &[(&str, &str)] = &[
    ("say", "Based on the conversation so far, suggest exactly what I should say next. Give 2-3 options: a safe answer, a strong answer, and a clarifying question. Keep each under 2 sentences."),
    ("followup", "Suggest 4-6 smart follow-up questions I could ask, based on the conversation and anything visible on screen. Rank by usefulness, one line each."),
    ("recap", "Give a tight recap of the conversation so far: key points, open questions, and anything I may have missed. Under 150 words."),
    ("screen", "Analyze what is on screen in detail together with the conversation context. Answer any visible questions, explain errors, and point out what matters most."),
];

pub fn profile_system(id: &str, custom: &str) -> String {
    let base = PROFILES.iter().find(|(k, _)| *k == id).map(|(_, v)| *v).unwrap_or(PROFILES[0].1);
    if custom.trim().is_empty() { base.to_string() } else { format!("{base}\n\nAdditional user instructions: {custom}") }
}

pub fn preset_question(key: &str) -> Option<String> {
    PRESETS.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
}

/// Run streaming Assist, emitting assist.chunk / assist.done / error.
pub async fn run_assist(
    app: AppHandle,
    api_key: String,
    model: String,
    req: AssistRequest,
    transcript: Vec<TranscriptLine>,
    images: Vec<CapturedImage>,
    thinking_level: String,
    search_grounding: bool,
) -> Result<(), String> {
    cancel_current();
    let cancel = Arc::new(AtomicBool::new(false));
    set_cancel(cancel.clone());

    let system = profile_system(&req.profile_id, &req.custom_instructions.clone().unwrap_or_default());
    let window_text = transcript_window(&transcript, 40);
    let question = match &req.preset {
        Some(p) => preset_question(p).unwrap_or_else(|| req.question.clone()),
        None => req.question.clone(),
    };
    let imgs: Vec<String> = images.iter().map(|i| i.data_base64.clone()).collect();
    let mut body = gemini::build_contents(&system, &window_text, &question, &imgs);
    body["generationConfig"] = gemini::generation_config(&req.slot, &thinking_level, false);
    if let Some(tools) = gemini::grounding_tool(search_grounding) {
        body["tools"] = tools;
    }

    let url = gemini::stream_url(&model, &api_key);
    stream_post(&app, &url, &body, cancel, events::ASSIST_CHUNK, events::ASSIST_DONE).await
}
fn transcript_window(t: &[TranscriptLine], n: usize) -> String { t.iter().rev().take(n).rev().map(|l| format!("[{}] {}", l.speaker, l.text)).collect::<Vec<_>>().join("\n") }
pub async fn stream_post(app: &AppHandle, url: &str, body: &serde_json::Value, cancel: Arc<AtomicBool>, chunk_ev: &str, done_ev: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let mut attempt: u32 = 0;
    loop {
        if cancel.load(Ordering::SeqCst) { let _ = app.emit(done_ev, serde_json::json!({"cancelled": true})); return Ok(()); }
        let resp = client.post(url).json(body).send().await;
        let resp = match resp { Ok(r) => r, Err(e) => { attempt += 1; if attempt > 2 { return Err(format!("request failed: {e}")); } backoff(attempt).await; continue; } };
        let status = resp.status();
        if status.as_u16() == 429 {
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("quota exceeded (429): {}", trunc(&b)));
        }
        if !status.is_success() { let b = resp.text().await.unwrap_or_default(); return Err(format!("gemini error {status}: {}", trunc(&b))); }
        break read_sse(app, resp, &cancel, chunk_ev, done_ev).await;
    }
}
async fn backoff(a: u32) { tokio::time::sleep(std::time::Duration::from_millis(500 * 2u64.pow(a.min(3)))).await; }
fn trunc(s: &str) -> String { if s.len() > 400 { format!("{}...", &s[..400]) } else { s.to_string() } }
async fn read_sse(app: &AppHandle, resp: reqwest::Response, cancel: &Arc<AtomicBool>, chunk_ev: &str, done_ev: &str) -> Result<(), String> {
    use futures_util::StreamExt;
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();
    let mut received_any = false;
    let mut stream_error: Option<String> = None;

    while let Some(item) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            let _ = app.emit(done_ev, serde_json::json!({"cancelled": true}));
            return Ok(());
        }
        let bytes = item.map_err(|e| format!("stream read: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        // Normalize CRLF to LF so both \r\n\r\n and \n\n are handled cleanly
        if buf.contains("\r\n") {
            buf = buf.replace("\r\n", "\n");
        }
        while let Some(pos) = buf.find("\n\n") {
            let event: String = buf.drain(..pos + 2).collect();
            for line in event.lines() {
                let trimmed = line.trim();
                if let Some(data) = trimmed.strip_prefix("data:") {
                    let d = data.trim();
                    if let Some(err_msg) = gemini::extract_stream_error(d) {
                        stream_error = Some(err_msg);
                    }
                    for t in gemini::extract_stream_text(d) {
                        received_any = true;
                        let _ = app.emit(chunk_ev, serde_json::json!({"text": t}));
                    }
                }
            }
        }
    }
    // Drain any remaining lines in buffer
    for line in buf.lines() {
        let trimmed = line.trim();
        if let Some(data) = trimmed.strip_prefix("data:") {
            let d = data.trim();
            if let Some(err_msg) = gemini::extract_stream_error(d) {
                stream_error = Some(err_msg);
            }
            for t in gemini::extract_stream_text(d) {
                received_any = true;
                let _ = app.emit(chunk_ev, serde_json::json!({"text": t}));
            }
        }
    }
    if let Some(err) = stream_error {
        let _ = app.emit(events::ERROR, serde_json::json!({"message": err}));
        let _ = app.emit(done_ev, serde_json::json!({"error": err}));
        return Ok(());
    }
    if !received_any {
        let err_msg = "No response text received from model";
        let _ = app.emit(events::ERROR, serde_json::json!({"message": err_msg}));
        let _ = app.emit(done_ev, serde_json::json!({"error": err_msg}));
        return Ok(());
    }
    let _ = app.emit(done_ev, serde_json::json!({}));
    Ok(())
}
