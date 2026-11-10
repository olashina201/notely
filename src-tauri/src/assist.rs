//! Assist: streaming generateContent with transcript + screenshots.
use crate::gemini;
use crate::state::{events, AssistRequest, CapturedImage, TranscriptLine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

struct CancelHandle {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

static CANCEL_HANDLE: Mutex<Option<CancelHandle>> = Mutex::new(None);

pub fn cancel_current() {
    if let Ok(mut guard) = CANCEL_HANDLE.lock() {
        if let Some(handle) = guard.take() {
            handle.flag.store(true, Ordering::SeqCst);
            handle.notify.notify_waiters();
        }
    }
}

fn create_cancel_handle() -> (Arc<AtomicBool>, Arc<Notify>) {
    let flag = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    if let Ok(mut guard) = CANCEL_HANDLE.lock() {
        *guard = Some(CancelHandle {
            flag: flag.clone(),
            notify: notify.clone(),
        });
    }
    (flag, notify)
}

pub fn emit_all<S: serde::Serialize + Clone>(app: &AppHandle, ev: &str, payload: S) {
    let _ = app.emit(ev, payload);
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
    primary_model: String,
    req: AssistRequest,
    transcript: Vec<TranscriptLine>,
    images: Vec<CapturedImage>,
    thinking_level: String,
    search_grounding: bool,
) -> Result<(), String> {
    cancel_current();
    let (cancel_flag, cancel_notify) = create_cancel_handle();

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

    stream_with_fallbacks(
        &app,
        &api_key,
        &primary_model,
        &body,
        cancel_flag,
        cancel_notify,
        events::ASSIST_CHUNK,
        events::ASSIST_DONE,
    )
    .await
}

fn transcript_window(t: &[TranscriptLine], n: usize) -> String {
    t.iter().rev().take(n).rev().map(|l| format!("[{}] {}", l.speaker, l.text)).collect::<Vec<_>>().join("\n")
}

async fn stream_with_fallbacks(
    app: &AppHandle,
    api_key: &str,
    primary_model: &str,
    body: &serde_json::Value,
    cancel_flag: Arc<AtomicBool>,
    cancel_notify: Arc<Notify>,
    chunk_ev: &str,
    done_ev: &str,
) -> Result<(), String> {
    let mut candidate_models = vec![primary_model.to_string()];
    for fallback in &["gemini-3.1-flash-lite", "gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-3.1-flash-lite-preview"] {
        if !candidate_models.iter().any(|m| m == fallback) {
            candidate_models.push(fallback.to_string());
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut last_err = String::new();

    for (idx, model) in candidate_models.iter().enumerate() {
        if cancel_flag.load(Ordering::SeqCst) {
            eprintln!("[assist] Request cancelled before model {model}");
            emit_all(app, done_ev, serde_json::json!({"cancelled": true}));
            return Ok(());
        }

        let mut req_body = body.clone();
        if idx > 0 {
            if let Some(cfg) = req_body.get_mut("generationConfig") {
                if let Some(obj) = cfg.as_object_mut() {
                    obj.remove("thinkingConfig");
                }
            }
        }

        let url = gemini::stream_url(model, api_key);
        eprintln!("[assist] Sending stream request to model '{model}' (candidate {idx})...");
        let send_fut = client.post(&url).json(&req_body).send();

        let resp = tokio::select! {
            _ = cancel_notify.notified() => {
                if cancel_flag.load(Ordering::SeqCst) {
                    eprintln!("[assist] Request cancelled by user during connect");
                    emit_all(app, done_ev, serde_json::json!({"cancelled": true}));
                    return Ok(());
                }
                continue;
            }
            res = send_fut => res,
        };

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[assist] Connection error for {model}: {e}");
                last_err = format!("Connection failed for {model}: {e}");
                continue;
            }
        };

        let status = resp.status();
        eprintln!("[assist] Model '{model}' responded with HTTP {status}");
        if !status.is_success() {
            let b = resp.text().await.unwrap_or_default();
            eprintln!("[assist] Error body from {model}: {b}");
            last_err = format!("Model {model} ({status}): {}", clean_quota_msg(&b));
            // Automatically try next candidate in fallback chain
            continue;
        }

        // If fallback was activated, inform the user with a subtle hint
        if idx > 0 {
            let notice = format!("*[Note: Switched to {model} due to rate limits on {primary_model}]*\n\n");
            emit_all(app, chunk_ev, serde_json::json!({"text": notice}));
        }

        return read_sse(app, resp, cancel_flag, cancel_notify, chunk_ev, done_ev).await;
    }

    eprintln!("[assist] All candidate models failed. Last error: {last_err}");
    Err(if last_err.is_empty() {
        "All model requests failed. Please check your API quota or connection.".to_string()
    } else {
        last_err
    })
}

fn clean_quota_msg(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(msg) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            if let Some(first_sentence) = msg.lines().next() {
                return first_sentence.to_string();
            }
            return msg.to_string();
        }
    }
    trunc(raw)
}

fn trunc(s: &str) -> String {
    if s.len() > 300 {
        format!("{}...", &s[..300])
    } else {
        s.to_string()
    }
}

async fn read_sse(
    app: &AppHandle,
    resp: reqwest::Response,
    cancel_flag: Arc<AtomicBool>,
    cancel_notify: Arc<Notify>,
    chunk_ev: &str,
    done_ev: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();
    let mut received_any = false;
    let mut stream_error: Option<String> = None;

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            eprintln!("[assist] SSE stream cancelled by user.");
            emit_all(app, done_ev, serde_json::json!({"cancelled": true}));
            return Ok(());
        }

        let next_item = tokio::select! {
            _ = cancel_notify.notified() => {
                if cancel_flag.load(Ordering::SeqCst) {
                    eprintln!("[assist] SSE stream cancelled via notify.");
                    emit_all(app, done_ev, serde_json::json!({"cancelled": true}));
                    return Ok(());
                }
                continue;
            }
            item = stream.next() => item,
        };

        let Some(item) = next_item else {
            break;
        };

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
                        eprintln!("[assist] SSE data error: {err_msg}");
                        stream_error = Some(err_msg);
                    }
                    for t in gemini::extract_stream_text(d) {
                        received_any = true;
                        eprintln!("[assist] Emitting chunk: {:?}", t);
                        emit_all(app, chunk_ev, serde_json::json!({"text": t}));
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
                emit_all(app, chunk_ev, serde_json::json!({"text": t}));
            }
        }
    }

    if let Some(err) = stream_error {
        eprintln!("[assist] Stream error encountered: {err}");
        emit_all(app, events::ERROR, serde_json::json!({"message": err}));
        emit_all(app, done_ev, serde_json::json!({"error": err}));
        return Ok(());
    }

    if !received_any {
        let err_msg = "No response text received from model";
        eprintln!("[assist] {err_msg}");
        emit_all(app, events::ERROR, serde_json::json!({"message": err_msg}));
        emit_all(app, done_ev, serde_json::json!({"error": err_msg}));
        return Ok(());
    }

    eprintln!("[assist] Streaming completed successfully.");
    emit_all(app, done_ev, serde_json::json!({}));
    Ok(())
}
