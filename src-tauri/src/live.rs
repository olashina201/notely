//! Gemini Live API session over WebSocket (tokio-tungstenite).
//! Two speaker channels: "me" (mic) and "other" (system audio). TEXT-only responses,
//! input-audio transcription drives the live transcript. Auto-reconnect with context resume.

use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as TokioMutex;
use tokio_tungstenite::tungstenite::Message;

use crate::state::{events, TranscriptLine};

const LIVE_HOST: &str = "generativelanguage.googleapis.com";
const LIVE_PATH: &str = "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

pub struct LiveHandle {
    stop: Arc<AtomicBool>,
    audio_tx: UnboundedSender<(String, Vec<i16>)>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl LiveHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock the audio loop.
        let _ = self.audio_tx.send(("__stop__".into(), Vec::new()));
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

pub struct LiveConfig {
    pub api_key: String,
    pub model: String,
    pub language: String,
}

pub fn spawn_live_session(
    app: AppHandle,
    cfg: LiveConfig,
    mut summary_context: String,
    audio_rx: UnboundedReceiver<(String, Vec<i16>)>,
    audio_tx: UnboundedSender<(String, Vec<i16>)>,
) -> Result<LiveHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let audio_tx_handle = audio_tx.clone();

    let worker = std::thread::Builder::new()
        .name("notely-live".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    emit_err(&app, &format!("live runtime failed: {e}"));
                    return;
                }
            };
            rt.block_on(run_session_loop(app, cfg, audio_rx, stop2, &mut summary_context));
        })
        .map_err(|e| format!("live thread failed: {e}"))?;

    Ok(LiveHandle {
        stop,
        audio_tx: audio_tx_handle,
        worker: Some(worker),
    })
}

async fn run_session_loop(
    app: AppHandle,
    cfg: LiveConfig,
    audio_rx: UnboundedReceiver<(String, Vec<i16>)>,
    stop: Arc<AtomicBool>,
    summary_context: &mut String,
) {
    let mut backoff = std::time::Duration::from_secs(1);
    let mut rx_opt: Option<UnboundedReceiver<(String, Vec<i16>)>> = Some(audio_rx);
    let _ = &mut rx_opt;

    while !stop.load(Ordering::SeqCst) {
        let rx = rx_opt.take().unwrap();
        match run_one_session(&app, &cfg, rx, &stop, summary_context).await {
            (Ok(ended_by_user), rx_back) => {
                rx_opt = Some(rx_back);
                if ended_by_user {
                    return;
                }
                backoff = std::time::Duration::from_secs(1);
            }
            (Err(e), rx_back) => {
                rx_opt = Some(rx_back);
                emit_err(&app, &format!("live session error: {e} (reconnecting\u{2026})"));
                emit_status(&app, "reconnecting");
                let wait = backoff.min(std::time::Duration::from_secs(15));
                let start = std::time::Instant::now();
                while start.elapsed() < wait && !stop.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                backoff = (backoff * 2).min(std::time::Duration::from_secs(15));
            }
        }
    }
}

/// Runs one websocket session. Returns (Ok(true) user-stopped / Ok(false) dropped / Err, rx back).
async fn run_one_session(
    app: &AppHandle,
    cfg: &LiveConfig,
    mut audio_rx: UnboundedReceiver<(String, Vec<i16>)>,
    stop: &Arc<AtomicBool>,
    summary_context: &mut String,
) -> (Result<bool, String>, UnboundedReceiver<(String, Vec<i16>)>) {
    let url = format!("wss://{LIVE_HOST}{LIVE_PATH}?key={key}", key = cfg.api_key);
    let (ws, _) = match tokio_tungstenite::connect_async(&url).await {
        Ok(x) => x,
        Err(e) => return (Err(format!("connect failed: {e}")), audio_rx),
    };
    let (write, mut read) = ws.split();
    let write = Arc::new(TokioMutex::new(write));

    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", cfg.model),
            "generationConfig": { "responseModalities": ["TEXT"] },
            "realtimeInputConfig": { "automaticActivityDetection": {} },
            "inputAudioTranscription": { "languageCodes": [cfg.language.clone()] },
            "outputAudioTranscription": {},
            "contextWindowCompression": { "triggerTokens": "20000", "slidingWindow": { "targetTokens": "10000" } },
            "sessionResumption": { "handle": "" },
        }
    });
    {
        let mut w = write.lock().await;
        if w.send(Message::Text(setup.to_string().into())).await.is_err() {
            return (Err("setup send failed".into()), audio_rx);
        }
    }
    if !summary_context.trim().is_empty() {
        let ctx = serde_json::json!({
            "clientContent": {
                "turns": [{ "role": "user", "parts": [{ "text": format!("Earlier in this meeting (summary): {}", summary_context) }] }],
                "turnComplete": true
            }
        });
        let mut w = write.lock().await;
        let _ = w.send(Message::Text(ctx.to_string().into())).await;
    }
    emit_status(app, "connected");

    const CHUNK: usize = 1600;
    let mut buf_me: Vec<i16> = Vec::new();
    let mut buf_other: Vec<i16> = Vec::new();
    let mut partial_text = String::new();

    let result: Result<bool, String> = loop {
        if stop.load(Ordering::SeqCst) {
            break Ok(true);
        }
        tokio::select! {
            biased;
            msg = read.next() => {
                let Some(msg) = msg else { break Ok(false) };
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => break Err(format!("read failed: {e}")),
                };
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break Ok(false),
                    _ => continue,
                };
                let v: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => break Err(format!("bad server message: {e}")),
                };
                if let Some(t) = v.pointer("/serverContent/inputTranscription/text").and_then(|t| t.as_str()) {
                    partial_text.push_str(t);
                    emit_partial(app, "other", &partial_text);
                }
                if let Some(parts) = v.pointer("/serverContent/modelTurn/parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(x) = part.get("text").and_then(|x| x.as_str()) {
                            partial_text.push_str(x);
                            emit_partial(app, "other", &partial_text);
                        }
                    }
                }
                if v.pointer("/serverContent/turnComplete").and_then(|b| b.as_bool()) == Some(true) {
                    if !partial_text.trim().is_empty() {
                        emit_final(app, "other", std::mem::take(&mut partial_text), summary_context);
                    }
                    partial_text.clear();
                }
                if v.pointer("/serverContent/interrupted").and_then(|b| b.as_bool()) == Some(true) {
                    partial_text.clear();
                }
            }
            item = audio_rx.recv() => {
                let Some((speaker, pcm)) = item else { break Ok(true) };
                if speaker == "__stop__" { break Ok(true); }
                let buf = if speaker == "me" { &mut buf_me } else { &mut buf_other };
                buf.extend_from_slice(&pcm);
                while buf.len() >= CHUNK {
                    let chunk: Vec<i16> = buf.drain(..CHUNK).collect();
                    let bytes: Vec<u8> = chunk.iter().flat_map(|x| x.to_le_bytes()).collect();
                    let msg = serde_json::json!({
                        "realtimeInput": {
                            "audio": {
                                "mimeType": "audio/pcm;rate=16000",
                                "data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
                            }
                        }
                    });
                    let mut w = write.lock().await;
                    if w.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    };
    if !partial_text.trim().is_empty() {
        emit_final(app, "other", partial_text, summary_context);
    }
    (result, audio_rx)
}

fn emit_partial(app: &AppHandle, speaker: &str, text: &str) {
    let line = TranscriptLine {
        id: format!("p-{}", chrono::Utc::now().timestamp_millis()),
        speaker: speaker.into(),
        text: text.to_string(),
        is_final: false,
        started_at: chrono::Utc::now().timestamp_millis(),
    };
    let _ = app.emit(events::TRANSCRIPT_PARTIAL, &line);
}

fn emit_final(app: &AppHandle, speaker: &str, text: String, summary_context: &mut String) {
    let line = TranscriptLine {
        id: uuid::Uuid::new_v4().to_string(),
        speaker: speaker.into(),
        text: text.clone(),
        is_final: true,
        started_at: chrono::Utc::now().timestamp_millis(),
    };
    // Keep a rolling tail for resume context.
    summary_context.push_str(&format!("\n[{speaker}] {text}"));
    const MAX_CTX: usize = 4000;
    if summary_context.len() > MAX_CTX {
        let skip = summary_context.len() - MAX_CTX;
        *summary_context = summary_context[skip..].to_string();
    }
    let _ = app.emit(events::TRANSCRIPT_FINAL, &line);
}

fn emit_status(app: &AppHandle, status: &str) {
    crate::diagnostics::record("info", "audio.status", None, serde_json::json!({"status": status}));
    let _ = app.emit(events::TRANSCRIPT_STATUS, serde_json::json!({ "status": status }));
}

fn emit_err(app: &AppHandle, msg: &str) {
    crate::diagnostics::record("error", "audio.error", None, serde_json::json!({"kind": crate::diagnostics::failure_kind(msg)}));
    let _ = app.emit(events::ERROR, serde_json::json!({ "message": msg }));
}
