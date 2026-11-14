//! Input-audio transcription only. Mic and loopback use separate Live sessions;
//! generated model responses are never treated as something a person said.
use crate::state::{events, TranscriptLine};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

const LIVE_URL: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

pub struct LiveHandle {
    stop: Arc<AtomicBool>,
    audio_tx: UnboundedSender<(String, Vec<i16>)>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl LiveHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.audio_tx.send(("__stop__".into(), Vec::new()));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub struct LiveConfig {
    pub provider: crate::state::TranscriptionProvider,
    pub api_key: String,
    pub model: String,
    pub language: String,
    pub system_audio: bool,
}

pub async fn test_connection(cfg: &LiveConfig) -> Result<(), String> {
    if cfg.provider == crate::state::TranscriptionProvider::Openai {
        return crate::openai_live::test_connection(cfg).await;
    }
    tokio::time::timeout(Duration::from_secs(12), async {
        let (mut socket, _) = tokio_tungstenite::connect_async(format!("{LIVE_URL}?key={}", cfg.api_key)).await.map_err(|_| "Could not connect to Gemini transcription")?;
        socket.send(Message::Text(setup_message(cfg, None).to_string().into())).await.map_err(|_| "Could not configure Gemini transcription")?;
        while let Some(message) = socket.next().await {
            match message.map_err(|_| "Gemini transcription setup disconnected")? {
                Message::Text(text) => {
                    let event: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Invalid Gemini setup reply")?;
                    if event.get("setupComplete").is_some() { let _ = socket.close(None).await; return Ok(()); }
                    if event.get("error").is_some() { return Err("Gemini rejected transcription setup. Check the Live model and API key in Settings.".into()); }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Err("Gemini closed before transcription setup completed".into())
    }).await.map_err(|_| "Gemini transcription setup timed out")?
}

pub fn spawn_live_session(
    app: AppHandle,
    cfg: LiveConfig,
    mut audio_rx: UnboundedReceiver<(String, Vec<i16>)>,
    audio_tx: UnboundedSender<(String, Vec<i16>)>,
) -> Result<LiveHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let worker=std::thread::Builder::new().name("notely-live".into()).spawn(move || {
        let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build(){Ok(rt)=>rt,Err(_)=>{emit_err(&app,"Could not start the meeting transcription runtime.");return;}};
        runtime.block_on(async {
            let (mic_tx,mic_rx)=tokio::sync::mpsc::channel(256);
            let (sys_tx,sys_rx)=tokio::sync::mpsc::channel(256);
            let cfg=Arc::new(cfg);
            let states=Arc::new(Mutex::new(std::collections::HashMap::from([("me","connecting"),("other",if cfg.system_audio {"connecting"}else{"disabled"})])));
            let router=async {
                let mut reported=false;
                while let Some((source,pcm))=audio_rx.recv().await {
                    if source=="__stop__"{break;}
                    let result=if source=="me" {mic_tx.try_send(pcm)}else if cfg.system_audio {sys_tx.try_send(pcm)}else{continue};
                    if let Err(error)=result {
                        if matches!(error,tokio::sync::mpsc::error::TrySendError::Closed(_)){break;}
                        if !reported{emit_err(&app,"Some meeting audio could not be queued while transcription was disconnected. The transcript may have a gap.");reported=true;}
                    }
                }
                drop(mic_tx);drop(sys_tx);
            };
            let mic=source_loop(&app,&cfg,"me",mic_rx,&stopping,states.clone());
            let sys=async {if cfg.system_audio {source_loop(&app,&cfg,"other",sys_rx,&stopping,states.clone()).await;}};
            tokio::select! {
                _=async{tokio::join!(router,mic,sys);}=>{},
                _=async{until_stopped(&stopping).await;tokio::time::sleep(Duration::from_secs(8)).await;}=>emit_err(&app,"Meeting shutdown reached its limit. Recognized transcript was saved; the final audio may be incomplete."),
            }
        });
    }).map_err(|e|format!("Live worker: {e}"))?;
    Ok(LiveHandle {
        stop,
        audio_tx,
        worker: Some(worker),
    })
}

async fn until_stopped(stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}
type SourceStates = Arc<Mutex<std::collections::HashMap<&'static str, &'static str>>>;
fn source_status(
    app: &AppHandle,
    source: &'static str,
    status: &'static str,
    states: &SourceStates,
) {
    let overall = {
        let mut states = states.lock().unwrap();
        states.insert(source, status);
        if states.values().any(|s| *s == "reconnecting") {
            "reconnecting"
        } else if states
            .values()
            .all(|s| *s == "connected" || *s == "disabled")
        {
            "connected"
        } else {
            "connecting"
        }
    };
    crate::diagnostics::record(
        "info",
        "audio.status",
        None,
        serde_json::json!({"status":overall,"kind":source}),
    );
    let _ = app.emit(
        events::TRANSCRIPT_STATUS,
        serde_json::json!({"status":overall,"source":source}),
    );
}

fn setup_message(cfg: &LiveConfig, resume: Option<&str>) -> serde_json::Value {
    let mut setup = serde_json::json!({"setup":{
        "model":format!("models/{}",cfg.model.trim_start_matches("models/")),
        "generationConfig":{"responseModalities":["AUDIO"]},
        "systemInstruction":{"parts":[{"text":"You provide input audio transcription for a meeting recorder. Do not answer questions or add commentary. Transcribe only speech actually heard. Output audio will not be played."}]},
        "realtimeInputConfig":{"automaticActivityDetection":{}},
        "inputAudioTranscription":{"languageCodes":[cfg.language]},
        "contextWindowCompression":{"slidingWindow":{}},
        "sessionResumption":{}
    }});
    if let Some(handle) = resume {
        setup["setup"]["sessionResumption"]["handle"] = serde_json::json!(handle);
    }
    setup
}
fn audio_message(pcm: &[i16]) -> Message {
    let bytes: Vec<u8> = pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    Message::Text(serde_json::json!({"realtimeInput":{"audio":{"mimeType":"audio/pcm;rate=16000","data":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,bytes)}}}).to_string().into())
}

#[derive(Default)]
struct TranscriptAccumulator {
    current: Option<TranscriptLine>,
}
impl TranscriptAccumulator {
    fn consume(&mut self, value: &serde_json::Value, source: &str) -> Vec<TranscriptLine> {
        let mut output = Vec::new();
        if let Some(text) = value
            .pointer("/serverContent/inputTranscription/text")
            .and_then(|v| v.as_str())
        {
            let line = self.current.get_or_insert_with(|| TranscriptLine {
                id: uuid::Uuid::new_v4().to_string(),
                speaker: source.into(),
                text: String::new(),
                is_final: false,
                started_at: chrono::Utc::now().timestamp_millis(),
            });
            line.text.push_str(text);
            output.push(line.clone());
        }
        if value
            .pointer("/serverContent/inputTranscription/finished")
            .and_then(|v| v.as_bool())
            == Some(true)
            || value
                .pointer("/serverContent/turnComplete")
                .and_then(|v| v.as_bool())
                == Some(true)
        {
            if let Some(line) = self.finish() {
                output.push(line);
            }
        }
        output
    }
    fn finish(&mut self) -> Option<TranscriptLine> {
        self.current
            .take()
            .filter(|line| !line.text.trim().is_empty())
            .map(|mut line| {
                line.is_final = true;
                line
            })
    }
}
fn deliver(app: &AppHandle, line: TranscriptLine) {
    if let Err(error) = crate::commands::persist_transcript_line(app, &line) {
        emit_err(app, &format!("Transcript could not be saved: {error}"));
    }
    let _ = app.emit(
        if line.is_final {
            events::TRANSCRIPT_FINAL
        } else {
            events::TRANSCRIPT_PARTIAL
        },
        line,
    );
}

async fn source_loop(
    app: &AppHandle,
    cfg: &LiveConfig,
    source: &'static str,
    mut rx: tokio::sync::mpsc::Receiver<Vec<i16>>,
    stop: &AtomicBool,
    states: SourceStates,
) {
    let mut resume: Option<String> = None;
    let mut backoff = 1;
    let mut transcript = TranscriptAccumulator::default();
    while !stop.load(Ordering::SeqCst) {
        crate::diagnostics::record("info", "audio.connection_started", None, serde_json::json!({"provider":cfg.provider.credentials().id(),"model":cfg.model,"kind":source}));
        let result = if cfg.provider == crate::state::TranscriptionProvider::Openai {
            crate::openai_live::one_connection(cfg, source, &mut rx, stop, |line| deliver(app, line), || source_status(app, source, "connected", &states)).await
        } else { one_connection(
            cfg,
            source,
            &mut rx,
            stop,
            &mut resume,
            &mut transcript,
            |line| deliver(app, line),
            || source_status(app, source, "connected", &states),
        )
        .await };
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if let Some(line) = transcript.finish() {
            deliver(app, line);
        }
        source_status(app, source, "reconnecting", &states);
        if let Err(error) = result {
            emit_err(
                app,
                &format!(
                    "{} transcription: {}",
                    if source == "me" {
                        "Microphone"
                    } else {
                        "System audio"
                    },
                    error.replace(&cfg.api_key, "[redacted]")
                ),
            );
        }
        tokio::select! {_=until_stopped(stop)=>break,_=tokio::time::sleep(Duration::from_secs(backoff))=>{}}
        backoff = (backoff * 2).min(10);
    }
    if let Some(line) = transcript.finish() {
        deliver(app, line);
    }
}

async fn one_connection(
    cfg: &LiveConfig,
    source: &str,
    rx: &mut tokio::sync::mpsc::Receiver<Vec<i16>>,
    stop: &AtomicBool,
    resume: &mut Option<String>,
    transcript: &mut TranscriptAccumulator,
    mut emit: impl FnMut(TranscriptLine),
    mut connected: impl FnMut(),
) -> Result<(), String> {
    let url = format!("{LIVE_URL}?key={}", cfg.api_key);
    let (mut socket, _) = tokio::select! {
        _=until_stopped(stop)=>return Ok(()),
        result=tokio::time::timeout(Duration::from_secs(10),tokio_tungstenite::connect_async(url))=>result.map_err(|_|"Live connection timed out")?.map_err(|e|format!("Live connection failed: {e}"))?
    };
    socket
        .send(Message::Text(
            setup_message(cfg, resume.as_deref()).to_string().into(),
        ))
        .await
        .map_err(|_| "Live setup send failed")?;
    let acknowledgement = tokio::select! {
        _=until_stopped(stop)=>return Ok(()),
        result=tokio::time::timeout(Duration::from_secs(10),socket.next())=>result.map_err(|_|"Live setup acknowledgement timed out")?
    };
    let acknowledgement = acknowledgement
        .ok_or("Live closed before setup")?
        .map_err(|e| format!("Live setup failed: {e}"))?;
    let Message::Text(text) = acknowledgement else {
        return Err(format!("Live setup rejected: {acknowledgement:?}"));
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Invalid Live setup reply")?;
    if value.get("setupComplete").is_none() {
        return Err(format!("Live setup rejected for model {}", cfg.model));
    }
    connected();
    let mut draining = false;
    let mut deadline = tokio::time::Instant::now() + Duration::from_secs(86400);
    loop {
        tokio::select! {
            _=tokio::time::sleep_until(deadline),if draining=>break,
            item=rx.recv(),if !draining=>{
                match item {
                    Some(pcm)=>tokio::time::timeout(Duration::from_secs(5),socket.send(audio_message(&pcm))).await.map_err(|_|"Audio upload timed out")?.map_err(|_|"Audio upload disconnected")?,
                    None=>{socket.send(Message::Text("{\"realtimeInput\":{\"audioStreamEnd\":true}}".into())).await.map_err(|_|"Could not finish audio stream")?;draining=true;deadline=tokio::time::Instant::now()+Duration::from_secs(3);}
                }
            }
            _=until_stopped(stop),if !draining=>{
                // Audio capture is stopped and the router has queued its final packets.
                while let Ok(pcm)=rx.try_recv(){socket.send(audio_message(&pcm)).await.map_err(|_|"Final audio upload disconnected")?;}
                socket.send(Message::Text("{\"realtimeInput\":{\"audioStreamEnd\":true}}".into())).await.map_err(|_|"Could not finish audio stream")?;
                draining=true;deadline=tokio::time::Instant::now()+Duration::from_secs(3);
            }
            message=socket.next()=>{
                let Some(message)=message else {break;};
                let message=message.map_err(|e|format!("Live read failed: {e}"))?;
                let text=match message {Message::Text(text)=>text,Message::Close(frame)=>return Err(format!("Live connection closed: {frame:?}")),_=>continue};
                let value:serde_json::Value=serde_json::from_str(&text).map_err(|_|"Invalid Live message")?;
                if let Some(error)=value.get("error"){return Err(format!("Live server error: {error}"));}
                if let Some(update)=value.get("sessionResumptionUpdate") {
                    if update.get("resumable").and_then(|v|v.as_bool())==Some(true){*resume=update.get("newHandle").and_then(|v|v.as_str()).map(str::to_owned);}
                }
                for line in transcript.consume(&value,source){emit(line);}
                if value.get("goAway").is_some()&&!draining{return Err("Live session requested reconnection".into());}
            }
        }
    }
    if let Some(line) = transcript.finish() {
        emit(line);
    }
    let _ = socket.close(None).await;
    Ok(())
}
fn emit_err(app: &AppHandle, message: &str) {
    crate::diagnostics::record(
        "error",
        "audio.error",
        None,
        serde_json::json!({"kind":crate::diagnostics::failure_kind(message)}),
    );
    let _ = app.emit(events::ERROR, serde_json::json!({"message":message}));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcription_uses_stable_ids_ignores_model_text_and_keeps_speaker_sources() {
        let mut accumulator = TranscriptAccumulator::default();
        let a = accumulator.consume(
            &serde_json::json!({"serverContent":{"inputTranscription":{"text":"Café "}}}),
            "me",
        );
        let b=accumulator.consume(&serde_json::json!({"serverContent":{"inputTranscription":{"text":"launch."},"modelTurn":{"parts":[{"text":"Invented decision"}]},"outputTranscription":{"text":"Do not record me"},"interrupted":true}}),"me");
        assert_eq!(a[0].id, b[0].id);
        assert_eq!(b[0].text, "Café launch.");
        assert_eq!(b[0].speaker, "me");
        let final_lines = accumulator.consume(
            &serde_json::json!({"serverContent":{"inputTranscription":{"finished":true}}}),
            "me",
        );
        assert!(final_lines[0].is_final);
        assert_eq!(final_lines[0].id, a[0].id);
        assert!(accumulator
            .consume(
                &serde_json::json!({"serverContent":{"turnComplete":true}}),
                "me"
            )
            .is_empty());
        let other=accumulator.consume(&serde_json::json!({"serverContent":{"inputTranscription":{"text":"Agreed.","finished":true}}}),"other");
        assert_eq!(other[1].speaker, "other");
        assert!(other[1].is_final);
    }
}

#[cfg(test)]
mod live_fixture_tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Uses saved Gemini credentials and a synthetic WAV file; makes paid Live and notes requests. Set NOTELY_LIVE_FIXTURE."]
    async fn live_fixture_transcribes_and_persists_meeting_notes() {
        let file = std::env::var("NOTELY_LIVE_FIXTURE")
            .expect("Set NOTELY_LIVE_FIXTURE to a synthetic 16kHz mono PCM16 WAV");
        let wav = std::fs::read(file).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        let mut offset = 12;
        let mut pcm = Vec::new();
        let mut format_ok = false;
        while offset + 8 <= wav.len() {
            let length =
                u32::from_le_bytes(wav[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let end = offset + 8 + length;
            assert!(end <= wav.len());
            match &wav[offset..offset + 4] {
                b"fmt " => {
                    assert!(length >= 16);
                    format_ok =
                        u16::from_le_bytes(wav[offset + 8..offset + 10].try_into().unwrap()) == 1
                            && u16::from_le_bytes(
                                wav[offset + 10..offset + 12].try_into().unwrap(),
                            ) == 1
                            && u32::from_le_bytes(
                                wav[offset + 12..offset + 16].try_into().unwrap(),
                            ) == 16000
                            && u16::from_le_bytes(
                                wav[offset + 22..offset + 24].try_into().unwrap(),
                            ) == 16;
                }
                b"data" => pcm.extend(
                    wav[offset + 8..end]
                        .chunks_exact(2)
                        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]])),
                ),
                _ => {}
            }
            offset = end + (length % 2);
        }
        assert!(
            format_ok && !pcm.is_empty(),
            "Fixture must be 16 kHz mono PCM16"
        );
        // A timed-out OS credential call must not keep Tokio's blocking pool alive.
        let (key_tx, key_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = key_tx.send(crate::settings::key_get(crate::providers::Provider::Gemini));
        });
        let key=key_rx.recv_timeout(Duration::from_secs(10)).expect("Keychain timed out; approve normal test-runner access in macOS or validate through the signed app").expect("Save a Gemini API key in Notely first");
        let cfg = LiveConfig {
            provider: crate::state::TranscriptionProvider::Gemini,
            api_key: key,
            model: std::env::var("NOTELY_LIVE_MODEL")
                .unwrap_or_else(|_| crate::gemini::FALLBACK_LIVE_MODEL.into()),
            language: "en-US".into(),
            system_audio: false,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(256);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let mut ready = Some(ready_tx);
        let stop = AtomicBool::new(false);
        let mut resume = None;
        let mut accumulator = TranscriptAccumulator::default();
        let mut lines = Vec::new();
        let send = async move {
            ready_rx.await.expect("Live did not acknowledge setup");
            for chunk in pcm.chunks(1600) {
                if tx.send(chunk.to_vec()).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            for _ in 0..15 {
                if tx.send(vec![0; 1600]).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            drop(tx);
        };
        let receive = one_connection(
            &cfg,
            "me",
            &mut rx,
            &stop,
            &mut resume,
            &mut accumulator,
            |line| {
                if line.is_final {
                    lines.push(line);
                }
            },
            || {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(());
                }
            },
        );
        let (_, result) = tokio::join!(send, receive);
        result
            .map_err(|e| e.replace(&cfg.api_key, "[redacted]"))
            .unwrap();
        let captured = lines
            .iter()
            .map(|line| line.text.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            captured.contains("cedar")
                && captured.contains("friday")
                && captured.contains("prototype")
                && captured.contains("budget"),
            "Synthetic fixture was not fully recognized"
        );
        let path =
            std::env::temp_dir().join(format!("notely-live-smoke-{}.db", uuid::Uuid::new_v4()));
        let db = crate::db::open(&path).unwrap();
        let id =
            crate::db::insert_meeting(&db, "Synthetic acceptance meeting", "meeting", 1).unwrap();
        let json = serde_json::to_string(&lines).unwrap();
        crate::db::checkpoint_meeting(&db, id, &json).unwrap();
        crate::db::finish_meeting(&db, id, 2, &json, None).unwrap();
        let notes = crate::notes::generate_notes(
            crate::providers::Provider::Gemini,
            &cfg.api_key,
            crate::gemini::FALLBACK_SMART_MODEL,
            &lines,
            "meeting",
        )
        .await
        .map_err(|e| e.replace(&cfg.api_key, "[redacted]"))
        .unwrap();
        assert!(
            notes["actionItems"]
                .as_array()
                .is_some_and(|items| !items.is_empty()),
            "Fixture action item missing from notes"
        );
        crate::db::save_meeting_notes(&db, id, &notes.to_string()).unwrap();
        drop(db);
        let db = crate::db::open(&path).unwrap();
        let saved = crate::db::get_meeting(&db, id).unwrap().unwrap();
        assert_eq!(saved.transcript_json, json);
        assert!(saved.notes_json.is_some());
        println!("PASS: Live setup acknowledged; {} final transcript segments; fixture facts recognized; notes generated; transcript and notes survived SQLite reopen.",lines.len());
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
