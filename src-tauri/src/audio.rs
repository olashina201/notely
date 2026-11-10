//! Live audio capture: microphone (cpal) + system loopback (WASAPI on Windows,
//! loopback-driver input on macOS). All audio resampled to 16 kHz mono PCM for Gemini Live.
//!
//! macOS note: cpal cannot tap system output directly. Without a loopback driver
//! (e.g. BlackHole) system capture reports an error and the UI shows setup guidance.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

pub const TARGET_RATE: u32 = 16_000;
/// ~100 ms chunks at 16 kHz mono i16.
pub const CHUNK_FRAMES: usize = 1600;

/// Downmix + resample arbitrary f32 frames to 16 kHz mono i16 (rubato 5, FFT resampler).
pub fn resample_to_16k(frames: &[f32], channels: usize, from_rate: u32) -> Result<Vec<i16>, String> {
    if frames.is_empty() || channels == 0 {
        return Ok(Vec::new());
    }
    let in_frames = frames.len() / channels;
    let mut per_ch: Vec<Vec<f32>> = vec![Vec::with_capacity(in_frames); channels];
    for (i, s) in frames.iter().enumerate() {
        per_ch[i % channels].push(*s);
    }
    let downmix = |data: &[f32], ch: usize| -> Vec<i16> {
        let n = data.len() / ch;
        (0..n)
            .map(|i| {
                let sum: f32 = (0..ch).map(|c| data[i * ch + c]).sum();
                ((sum / ch as f32).clamp(-1.0, 1.0) * 32767.0) as i16
            })
            .collect()
    };
    if from_rate == TARGET_RATE {
        let interleaved: Vec<f32> = {
            let mut v = vec![0.0f32; in_frames * channels];
            for (c, ch) in per_ch.iter().enumerate() {
                for (i, s) in ch.iter().enumerate() {
                    v[i * channels + c] = *s;
                }
            }
            v
        };
        return Ok(downmix(&interleaved, channels));
    }
    resample_fft(&per_ch, channels, from_rate, downmix)
}

fn resample_fft(
    per_ch: &[Vec<f32>],
    channels: usize,
    from_rate: u32,
    downmix: impl Fn(&[f32], usize) -> Vec<i16>,
) -> Result<Vec<i16>, String> {
    use audioadapter_buffers::direct::SequentialSliceOfVecs;
    use rubato::{Fft, FixedSync, Resampler};
    let in_frames = per_ch[0].len();
    let mut resampler = Fft::<f32>::new(
        from_rate as usize,
        TARGET_RATE as usize,
        in_frames.max(64),
        channels,
        FixedSync::Input,
    )
    .map_err(|e| format!("resampler init failed: {e:?}"))?;
    let input = SequentialSliceOfVecs::new(per_ch, channels, in_frames)
        .map_err(|e| format!("buffer wrap failed: {e:?}"))?;
    let out = resampler.process(&input, None).map_err(|e| format!("resample failed: {e:?}"))?;
    let data = out.take_data();
    Ok(downmix(&data, channels))
}

/// Split mono i16 into ~100 ms chunks.
pub fn chunk_mono(pcm: &[i16]) -> Vec<Vec<i16>> {
    pcm.chunks(CHUNK_FRAMES).map(|c| c.to_vec()).collect()
}

/// Handle for a running capture thread.
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
pub fn spawn_mic_capture(tx: mpsc::Sender<Vec<i16>>) -> Result<CaptureHandle, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| "no microphone found".to_string())?;
    let supported = device.default_input_config().map_err(|e| format!("mic config error: {e}"))?;
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.into();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let thread = std::thread::Builder::new()
        .name("notely-mic".into())
        .spawn(move || {
            let tx2 = tx.clone();
            let build: Result<cpal::Stream, String> = (|| {
                let cb = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mono) = resample_to_16k(data, channels, sample_rate) {
                        for chunk in chunk_mono(&mono) {
                            if tx2.send(chunk).is_err() {
                                break;
                            }
                        }
                    }
                };
                device
                    .build_input_stream(&config, cb, |e| eprintln!("[notely] mic stream error: {e}"), None)
                    .map_err(|e| format!("mic stream error: {e}"))
            })();
            match build {
                Ok(stream) => {
                    if stream.play().is_err() {
                        return;
                    }
                    let _stream = stream;
                    while !stop2.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(e) => eprintln!("[notely] {e}"),
            }
        })
        .map_err(|e| format!("mic thread spawn failed: {e}"))?;
    Ok(CaptureHandle { stop, thread: Some(thread) })
}

pub fn spawn_system_capture(tx: mpsc::Sender<Vec<i16>>) -> Result<CaptureHandle, String> {
    #[cfg(windows)]
    {
        self::windows_loopback::spawn(tx)
    }
    #[cfg(not(windows))]
    {
        self::loopback_device::spawn(tx)
    }
}
/// Mixer: tags mic chunks "me" and system chunks "other", forwards to the Live session.
/// System capture is best-effort: if no loopback device exists (stock macOS),
/// the meeting still runs mic-only and the caller surfaces a warning.
pub struct Mixer {
    mic: CaptureHandle,
    sys: Option<CaptureHandle>,
    fwd_stop: Arc<AtomicBool>,
    fwd_thread: Option<JoinHandle<()>>,
}

impl Mixer {
    pub fn start(
        live_tx: tokio::sync::mpsc::UnboundedSender<(String, Vec<i16>)>,
    ) -> Result<(Self, Option<String>), String> {
        let (mic_raw_tx, mic_raw_rx) = mpsc::channel::<Vec<i16>>();
        let (sys_raw_tx, sys_raw_rx) = mpsc::channel::<Vec<i16>>();
        let mic = spawn_mic_capture(mic_raw_tx)?;
        let (sys, sys_warning) = match spawn_system_capture(sys_raw_tx) {
            Ok(h) => (Some(h), None),
            Err(e) => (None, Some(format!("system audio unavailable ({e}); capturing mic only"))),
        };
        let fwd_stop = Arc::new(AtomicBool::new(false));
        let fwd_stop2 = fwd_stop.clone();
        let fwd_thread = std::thread::Builder::new()
            .name("notely-mixer".into())
            .spawn(move || {
                while !fwd_stop2.load(Ordering::SeqCst) {
                    match mic_raw_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(m) => {
                            if live_tx.send(("me".into(), m)).is_err() {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        _ => {}
                    }
                    if let Ok(x) = sys_raw_rx.try_recv() {
                        if live_tx.send(("other".into(), x)).is_err() {
                            break;
                        }
                    }
                }
            })
            .map_err(|e| format!("mixer thread failed: {e}"))?;
        Ok((Self { mic, sys, fwd_stop, fwd_thread: Some(fwd_thread) }, sys_warning))
    }

    pub fn stop(mut self) {
        self.fwd_stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.fwd_thread.take() {
            let _ = t.join();
        }
        self.mic.stop();
        if let Some(sys) = self.sys.take() {
            sys.stop();
        }
    }
}
#[cfg(windows)]
mod windows_loopback {
    use super::*;
    use wasapi::*;

    pub fn spawn(tx: mpsc::Sender<Vec<i16>>) -> Result<CaptureHandle, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("notely-loopback".into())
            .spawn(move || {
                if let Err(e) = run_loop(tx, stop2) {
                    eprintln!("[notely] system audio error: {e}");
                }
            })
            .map_err(|e| format!("loopback thread failed: {e}"))?;
        Ok(CaptureHandle { stop, thread: Some(thread) })
    }

    /// Direction::Render on the default render endpoint = loopback capture.
    fn run_loop(tx: mpsc::Sender<Vec<i16>>, stop: Arc<AtomicBool>) -> Result<(), String> {
        initialize_mta().map_err(|e| format!("WASAPI init failed: {e:?}"))?;
        let enumerator = DeviceEnumerator::new().map_err(|e| format!("device enum failed: {e:?}"))?;
        let device = enumerator
            .get_default_device(Direction::Render)
            .map_err(|e| format!("no render device: {e:?}"))?;
        let mut client = device.get_iaudioclient().map_err(|e| format!("audio client: {e:?}"))?;
        let desired = WaveFormat::new(32, 32, SampleType::Float, 44100, 2, None);
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };
        client
            .initialize_client(&desired, Direction::Render, mode, None)
            .map_err(|e| format!("init loopback client: {e:?}"))?;
        let event = client.set_get_eventhandle().map_err(|e| format!("event handle: {e:?}"))?;
        let capture = client.get_audiocaptureclient().map_err(|e| format!("capture client: {e:?}"))?;
        client.start_stream().map_err(|e| format!("start stream: {e:?}"))?;
        let blockalign = desired.get_blockalign() as usize;
        let mut queue: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
        let mut leftover: Vec<f32> = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            capture
                .read_from_device_to_deque(&mut queue)
                .map_err(|e| format!("read failed: {e:?}"))?;
            while queue.len() >= blockalign {
                let mut bytes = vec![0u8; blockalign];
                for b in bytes.iter_mut() {
                    *b = queue.pop_front().unwrap_or(0);
                }
                leftover.extend(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])));
            }
            while leftover.len() >= 44100 {
                let block: Vec<f32> = leftover.drain(..44100).collect();
                if let Ok(mono) = resample_to_16k(&block, 2, 44100) {
                    for chunk in chunk_mono(&mono) {
                        if tx.send(chunk).is_err() {
                            client.stop_stream().ok();
                            return Ok(());
                        }
                    }
                }
            }
            let _ = event.wait_for_event(200_000);
        }
        client.stop_stream().ok();
        Ok(())
    }
}

#[cfg(not(windows))]
mod loopback_device {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    /// macOS: capture from a loopback driver (BlackHole etc.) exposed as an input device.
    pub fn spawn(tx: mpsc::Sender<Vec<i16>>) -> Result<CaptureHandle, String> {
        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| format!("input devices error: {e}"))?
            .find(|d| {
                d.name()
                    .map(|n| {
                        let n = n.to_lowercase();
                        n.contains("blackhole") || n.contains("loopback") || n.contains("soundflower")
                    })
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                "System audio unavailable: install a loopback driver (e.g. BlackHole)                  and select it as input, or use mic-only mode. See Settings > Audio."
                    .to_string()
            })?;
        let supported = device.default_input_config().map_err(|e| format!("loopback config error: {e}"))?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("notely-sysaudio".into())
            .spawn(move || {
                let tx2 = tx.clone();
                let cb = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mono) = resample_to_16k(data, channels, sample_rate) {
                        for chunk in chunk_mono(&mono) {
                            if tx2.send(chunk).is_err() {
                                break;
                            }
                        }
                    }
                };
                match device.build_input_stream(&config, cb, |e| eprintln!("[notely] sys stream error: {e}"), None) {
                    Ok(stream) => {
                        if stream.play().is_err() {
                            return;
                        }
                        let _stream = stream;
                        while !stop2.load(Ordering::SeqCst) {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                    Err(e) => eprintln!("[notely] loopback stream failed: {e}"),
                }
            })
            .map_err(|e| format!("system audio thread failed: {e}"))?;
        Ok(CaptureHandle { stop, thread: Some(thread) })
    }
}
