//! OpenAI transcription-only WebSocket. No assistant response or output audio.
use crate::{live::LiveConfig, state::TranscriptLine};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
const URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";

fn setup(cfg: &LiveConfig) -> Value {
    let mut transcription = json!({"model":cfg.model,"delay":"low"});
    // This model uses plural languages with language codes rather than locales.
    let language = cfg
        .language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    if !language.is_empty() && language != "auto" {
        transcription["languages"] = json!([language]);
    }
    json!({"type":"session.update","session":{"type":"transcription","audio":{"input":{
        "format":{"type":"audio/pcm","rate":24000},"transcription":transcription,"turn_detection":null
    }}}})
}
fn server_error(value: &Value) -> String {
    let code = value
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("transcription_error");
    // Do not copy server payloads into logs or surface audio content through errors.
    match code {
        "invalid_api_key" => "OpenAI transcription: the API key was rejected. Update it in Settings.".into(),
        "insufficient_quota" | "rate_limit_exceeded" => "OpenAI transcription: quota or rate limit reached. Check your API billing and limits.".into(),
        _ => format!("OpenAI transcription rejected the request ({code}). Check the transcription model and API access in Settings."),
    }
}
async fn connect(cfg: &LiveConfig) -> Result<Socket, String> {
    connect_at(cfg, URL).await
}
async fn connect_at(cfg: &LiveConfig, url: &str) -> Result<Socket, String> {
    let mut request = url
        .into_client_request()
        .map_err(|_| "Invalid transcription endpoint")?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", cfg.api_key)
            .parse()
            .map_err(|_| "Invalid OpenAI API key format")?,
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.map_err(|error| {
        if let tokio_tungstenite::tungstenite::Error::Http(response) = error {
            format!("OpenAI transcription connection rejected (HTTP {}). Check the API key and access.", response.status().as_u16())
        } else { "Could not connect to OpenAI transcription. Check your connection.".into() }
    })?;
    socket
        .send(Message::Text(setup(cfg).to_string().into()))
        .await
        .map_err(|_| "Could not configure OpenAI transcription")?;
    while let Some(message) = socket.next().await {
        match message.map_err(|_| "OpenAI transcription setup disconnected")? {
            Message::Text(text) => {
                let event: Value = serde_json::from_str(&text)
                    .map_err(|_| "Invalid OpenAI transcription setup reply")?;
                match event["type"].as_str() {
                    Some("session.updated")
                        if event.pointer("/session/type").and_then(Value::as_str)
                            == Some("transcription") =>
                    {
                        return Ok(socket)
                    }
                    Some("error") => return Err(server_error(&event)),
                    _ => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Err("OpenAI closed before transcription setup completed".into())
}
pub async fn test_connection(cfg: &LiveConfig) -> Result<(), String> {
    let mut socket = tokio::time::timeout(Duration::from_secs(12), connect(cfg))
        .await
        .map_err(|_| "OpenAI transcription setup timed out")??;
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
    Ok(())
}

/// Continuous linear upsampling from 16kHz mono PCM16 to the API's 24kHz format.
/// Keep the fractional position and boundary sample across capture callbacks.
#[derive(Default)]
struct Upsampler {
    samples: Vec<i16>,
    position: usize,
}
impl Upsampler {
    fn push(&mut self, input: &[i16]) -> Vec<i16> {
        self.samples.extend_from_slice(input);
        let mut out = Vec::new();
        while self.position / 3 + 1 < self.samples.len() {
            let index = self.position / 3;
            let fraction = self.position % 3;
            out.push(
                ((self.samples[index] as i32 * (3 - fraction) as i32
                    + self.samples[index + 1] as i32 * fraction as i32)
                    / 3) as i16,
            );
            self.position += 2;
        }
        let consumed = (self.position / 3).min(self.samples.len().saturating_sub(1));
        self.samples.drain(..consumed);
        self.position -= consumed * 3;
        out
    }
}
fn append(samples: &[i16]) -> Value {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    json!({"type":"input_audio_buffer.append","audio":STANDARD.encode(bytes)})
}
fn commit() -> Value {
    json!({"type":"input_audio_buffer.commit"})
}

/// Client turn detection: 20ms frames, 200ms pre-roll, 600ms trailing silence.
/// Cap continuous speech at 15s so long speakers still produce finalized turns.
#[derive(Default)]
struct Turns {
    input: Vec<i16>,
    preroll: VecDeque<i16>,
    active: bool,
    frames: usize,
    silent: usize,
    upsampler: Upsampler,
}
impl Turns {
    fn push(&mut self, pcm: &[i16]) -> Vec<Value> {
        self.input.extend_from_slice(pcm);
        let count = self.input.len() / 320 * 320;
        let frames: Vec<i16> = self.input.drain(..count).collect();
        let mut messages = Vec::new();
        for frame in frames.chunks_exact(320) {
            let speech =
                frame.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / 320.0 >= 120.0_f64.powi(2);
            if !self.active {
                self.preroll.extend(frame);
                while self.preroll.len() > 3200 {
                    self.preroll.pop_front();
                }
                if !speech {
                    continue;
                }
                self.active = true;
                let start: Vec<i16> = self.preroll.drain(..).collect();
                messages.push(append(&self.upsampler.push(&start)));
                self.frames = start.len() / 320;
            } else {
                messages.push(append(&self.upsampler.push(frame)));
                self.frames += 1;
            }
            self.silent = if speech { 0 } else { self.silent + 1 };
            if self.silent >= 30 || self.frames >= 750 {
                messages.push(commit());
                self.active = false;
                self.frames = 0;
                self.silent = 0;
            }
        }
        messages
    }
    fn finish(&mut self) -> Vec<Value> {
        let mut messages = Vec::new();
        if self.active {
            if !self.input.is_empty() {
                messages.push(append(&self.upsampler.push(&self.input)));
            }
            if self.frames < 6 {
                messages.push(append(
                    &self.upsampler.push(&vec![0; (6 - self.frames) * 320]),
                ));
            }
            messages.push(commit());
            self.active = false;
        }
        self.input.clear();
        messages
    }
}

struct Accumulator {
    session: String,
    pending: HashMap<String, TranscriptLine>,
    completed: HashSet<String>,
    order: VecDeque<String>,
}
impl Default for Accumulator {
    fn default() -> Self {
        Self {
            session: uuid::Uuid::new_v4().to_string(),
            pending: HashMap::new(),
            completed: HashSet::new(),
            order: VecDeque::new(),
        }
    }
}
impl Accumulator {
    fn consume(&mut self, value: &Value, source: &str) -> Option<TranscriptLine> {
        let kind = value["type"].as_str()?;
        if !matches!(
            kind,
            "input_audio_buffer.committed"
                | "conversation.item.input_audio_transcription.delta"
                | "conversation.item.input_audio_transcription.completed"
        ) {
            return None;
        }
        let id = value["item_id"].as_str()?;
        if self.completed.contains(id) {
            return None;
        }
        let line = self
            .pending
            .entry(id.into())
            .or_insert_with(|| TranscriptLine {
                id: format!("{}:{source}:{id}", self.session),
                speaker: source.into(),
                text: String::new(),
                is_final: false,
                started_at: chrono::Utc::now().timestamp_millis(),
            });
        if kind == "input_audio_buffer.committed" {
            return None;
        }
        if kind.ends_with(".delta") {
            line.text.push_str(value["delta"].as_str()?);
            return Some(line.clone());
        }
        line.text = value["transcript"].as_str()?.into();
        line.is_final = true;
        let line = self.pending.remove(id);
        self.completed.insert(id.into());
        self.order.push_back(id.into());
        if self.order.len() > 512 {
            if let Some(old) = self.order.pop_front() {
                self.completed.remove(&old);
            }
        }
        line
    }
    fn finish(&mut self) -> Vec<TranscriptLine> {
        let mut lines: Vec<_> = self
            .pending
            .drain()
            .map(|(_, mut line)| {
                line.is_final = true;
                line
            })
            .filter(|line| !line.text.trim().is_empty())
            .collect();
        lines.sort_by_key(|line| line.started_at);
        lines
    }
}

pub async fn one_connection(
    cfg: &LiveConfig,
    source: &str,
    rx: &mut tokio::sync::mpsc::Receiver<Vec<i16>>,
    stop: &AtomicBool,
    emit: impl FnMut(TranscriptLine),
    connected: impl FnMut(),
) -> Result<(), String> {
    let socket = tokio::select! {
        _=until_stopped(stop)=>return Ok(()),
        result=tokio::time::timeout(Duration::from_secs(12),connect(cfg))=>result.map_err(|_|"OpenAI transcription setup timed out")??
    };
    stream_connection(socket, source, rx, stop, emit, connected).await
}

async fn stream_connection(
    mut socket: Socket,
    source: &str,
    rx: &mut tokio::sync::mpsc::Receiver<Vec<i16>>,
    stop: &AtomicBool,
    mut emit: impl FnMut(TranscriptLine),
    mut connected: impl FnMut(),
) -> Result<(), String> {
    connected();
    let mut turns = Turns::default();
    let mut transcript = Accumulator::default();
    let mut draining = false;
    let mut pending = 0usize;
    let mut deadline = tokio::time::Instant::now() + Duration::from_secs(86400);
    let result = async {
        loop {
            if draining && pending == 0 { break; }
            let messages = tokio::select! {
                _=tokio::time::sleep_until(deadline),if draining=>return Err("OpenAI final transcription timed out; the last speech may be incomplete".into()),
                _=until_stopped(stop),if !draining=>{
                    let mut messages=Vec::new();
                    while let Ok(pcm)=rx.try_recv(){messages.extend(turns.push(&pcm));}
                    messages.extend(turns.finish());draining=true;deadline=tokio::time::Instant::now()+Duration::from_secs(5);messages
                }
                pcm=rx.recv(),if !draining=>match pcm {Some(pcm)=>turns.push(&pcm),None=>{draining=true;deadline=tokio::time::Instant::now()+Duration::from_secs(5);turns.finish()}},
                message=socket.next()=>{
                    let Some(message)=message else {return Err("OpenAI transcription disconnected; some speech may be missing".into())};
                    match message.map_err(|_|"OpenAI transcription read failed")? {
                        Message::Text(text)=>{
                            let value:Value=serde_json::from_str(&text).map_err(|_|"Invalid OpenAI transcription event")?;
                            match value["type"].as_str() {
                                Some("error" | "conversation.item.input_audio_transcription.failed")=>return Err(server_error(&value)),
                                Some("conversation.item.input_audio_transcription.completed")=>{if let Some(line)=transcript.consume(&value,source){pending=pending.saturating_sub(1);emit(line);}},
                                _=>{if let Some(line)=transcript.consume(&value,source){emit(line);}}
                            }
                        }
                        Message::Close(_)=>return Err("OpenAI transcription connection closed".into()),
                        _=>{}
                    }
                    Vec::new()
                }
            };
            for message in messages {
                if message["type"]=="input_audio_buffer.commit" { pending+=1; }
                if pending > 8 { return Err("OpenAI is not finalizing transcript turns. Reconnecting; some speech may be incomplete.".into()); }
                tokio::time::timeout(Duration::from_secs(3),socket.send(Message::Text(message.to_string().into()))).await.map_err(|_|"OpenAI audio upload timed out")?.map_err(|_|"OpenAI audio upload disconnected")?;
            }
        }
        Ok(())
    }.await;
    for line in transcript.finish() {
        emit(line);
    }
    let _ = tokio::time::timeout(Duration::from_millis(200), socket.close(None)).await;
    result
}
async fn until_stopped(stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn websocket_setup_audio_and_shutdown_keep_final_transcript() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["Authorization"], "Bearer fixture-key");
                    assert_eq!(request.uri().query(), Some("intent=transcription"));
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.send(Message::Text(
                json!({"type":"session.created"}).to_string().into(),
            ))
            .await
            .unwrap();
            let Message::Text(text) = ws.next().await.unwrap().unwrap() else {
                panic!("Expected setup")
            };
            let config: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(config["session"]["audio"]["input"]["format"]["rate"], 24000);
            assert_eq!(
                config["session"]["audio"]["input"]["transcription"]["model"],
                "gpt-live-transcribe"
            );
            assert_eq!(
                config["session"]["audio"]["input"]["transcription"]["languages"],
                json!(["en"])
            );
            assert!(config["session"]["audio"]["input"]["turn_detection"].is_null());
            ws.send(Message::Text(
                json!({"type":"session.updated","session":{"type":"transcription"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let mut bytes = 0;
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let event: Value = serde_json::from_str(&text).unwrap();
                if event["type"] == "input_audio_buffer.append" {
                    bytes += STANDARD
                        .decode(event["audio"].as_str().unwrap())
                        .unwrap()
                        .len();
                }
                if event["type"] == "input_audio_buffer.commit" {
                    assert!(bytes >= 24000 * 2 - 4);
                    for value in [
                        json!({"type":"input_audio_buffer.committed","item_id":"turn1"}),
                        json!({"type":"conversation.item.input_audio_transcription.delta","item_id":"turn1","delta":"Ship on"}),
                        json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"turn1","transcript":"Ship on Friday."}),
                    ] {
                        ws.send(Message::Text(value.to_string().into()))
                            .await
                            .unwrap();
                    }
                    return;
                }
            }
            panic!("No committed audio received");
        });
        let cfg = LiveConfig {
            provider: crate::state::TranscriptionProvider::Openai,
            api_key: "fixture-key".into(),
            model: "gpt-live-transcribe".into(),
            language: "en-US".into(),
            system_audio: false,
        };
        let socket = connect_at(
            &cfg,
            &format!("ws://{address}/v1/realtime?intent=transcription"),
        )
        .await
        .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        tx.send(vec![1200; 16000]).await.unwrap();
        drop(tx);
        let mut lines = Vec::new();
        let mut connected = false;
        tokio::time::timeout(
            Duration::from_secs(3),
            stream_connection(
                socket,
                "me",
                &mut rx,
                &AtomicBool::new(false),
                |line| lines.push(line),
                || connected = true,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert!(connected);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].id, lines[1].id);
        assert_eq!(lines[1].text, "Ship on Friday.");
        assert!(lines[1].is_final);
    }

    #[test]
    fn upsampling_keeps_time_and_boundaries() {
        let input: Vec<i16> = (0..16000)
            .map(|i| ((i as f64 / 13.0).sin() * 10000.0) as i16)
            .collect();
        let expected = Upsampler::default().push(&input);
        let mut resampler = Upsampler::default();
        let actual: Vec<_> = input.chunks(113).flat_map(|p| resampler.push(p)).collect();
        assert_eq!(actual, expected);
        assert!((actual.len() as isize - 24000).abs() <= 2);
    }
    #[test]
    fn silence_is_not_committed_and_speech_is_flushed_once() {
        let mut turns = Turns::default();
        assert!(turns.push(&vec![0; 16000]).is_empty());
        let mut messages = turns.push(&vec![2000; 16000]);
        messages.extend(turns.finish());
        assert_eq!(
            messages
                .iter()
                .filter(|m| m["type"] == "input_audio_buffer.commit")
                .count(),
            1
        );
        assert!(turns.finish().is_empty());
        let encoded = messages[0]["audio"].as_str().unwrap();
        assert!(STANDARD.decode(encoded).unwrap().len() >= 3200 * 2);
    }
    #[test]
    fn finals_replace_deltas_and_out_of_order_turns_do_not_mix() {
        let mut a = Accumulator::default();
        let first=a.consume(&json!({"type":"conversation.item.input_audio_transcription.delta","item_id":"a","delta":"Partial"}),"me").unwrap();
        let second=a.consume(&json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"b","transcript":"Other turn"}),"me").unwrap();
        let final_a = json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"Correct final"});
        let final_line = a.consume(&final_a, "me").unwrap();
        assert_eq!(first.id, final_line.id);
        assert_ne!(second.id, final_line.id);
        assert_eq!(final_line.text, "Correct final");
        assert!(final_line.is_final);
        assert!(a.consume(&final_a, "me").is_none());
        assert!(a.consume(&json!({"type":"response.text.delta","item_id":"x","delta":"Do not transcribe AI output"}),"me").is_none());
    }
}
