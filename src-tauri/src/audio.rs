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

/// Keep resampling state across audio callbacks; resample only after downmixing.
pub struct AudioConverter {
    channels: usize,
    partial_frame: Vec<f32>,
    mono: Vec<f32>,
    resampler: Option<rubato::Fft<f32>>,
}
impl AudioConverter {
    pub fn new(channels: usize, rate: u32) -> Result<Self, String> {
        if channels == 0 || rate == 0 {
            return Err("Invalid audio format".into());
        }
        let resampler = if rate == TARGET_RATE {
            None
        } else {
            Some(
                rubato::Fft::new(
                    rate as usize,
                    TARGET_RATE as usize,
                    (rate as usize / 100).max(64),
                    1,
                    rubato::FixedSync::Input,
                )
                .map_err(|e| format!("Audio resampler: {e}"))?,
            )
        };
        Ok(Self {
            channels,
            partial_frame: Vec::new(),
            mono: Vec::new(),
            resampler,
        })
    }
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<i16>, String> {
        use rubato::Resampler;
        self.partial_frame.extend_from_slice(samples);
        let count = self.partial_frame.len() / self.channels * self.channels;
        self.mono.extend(
            self.partial_frame[..count]
                .chunks_exact(self.channels)
                .map(|frame| frame.iter().sum::<f32>() / self.channels as f32),
        );
        self.partial_frame.drain(..count);
        let pcm = |sample: f32| (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(self.mono.drain(..).map(pcm).collect());
        };
        let mut result = Vec::new();
        while self.mono.len() >= resampler.input_frames_next() {
            let input = vec![self
                .mono
                .drain(..resampler.input_frames_next())
                .collect::<Vec<_>>()];
            let view =
                audioadapter_buffers::direct::SequentialSliceOfVecs::new(&input, 1, input[0].len())
                    .map_err(|e| e.to_string())?;
            let output = resampler.process(&view, None).map_err(|e| e.to_string())?;
            result.extend(output.take_data().into_iter().map(pcm));
        }
        Ok(result)
    }
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
fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<Vec<i16>>,
    stopped: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;
    let mut converter = AudioConverter::new(config.channels as usize, config.sample_rate.0)?;
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
                match converter.push(&samples) {
                    Ok(pcm) => {
                        for chunk in chunk_mono(&pcm) {
                            if tx.send(chunk).is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => crate::diagnostics::record(
                        "error",
                        "audio.convert_failed",
                        None,
                        serde_json::json!({"kind":"resampler"}),
                    ),
                }
            },
            move |_| {
                stopped.store(true, Ordering::SeqCst);
                crate::diagnostics::record(
                    "error",
                    "audio.device_failed",
                    None,
                    serde_json::json!({"kind":"input_stream"}),
                );
            },
            None,
        )
        .map_err(|e| format!("Audio stream: {e}"))
}

fn spawn_input(
    device: cpal::Device,
    tx: mpsc::Sender<Vec<i16>>,
    name: &str,
) -> Result<CaptureHandle, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Input format: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let started = (|| -> Result<cpal::Stream, String> {
                let stream = match format {
                    cpal::SampleFormat::F32 => {
                        build_input::<f32>(&device, &config, tx, stop2.clone())
                    }
                    cpal::SampleFormat::F64 => {
                        build_input::<f64>(&device, &config, tx, stop2.clone())
                    }
                    cpal::SampleFormat::I16 => {
                        build_input::<i16>(&device, &config, tx, stop2.clone())
                    }
                    cpal::SampleFormat::I32 => {
                        build_input::<i32>(&device, &config, tx, stop2.clone())
                    }
                    cpal::SampleFormat::U16 => {
                        build_input::<u16>(&device, &config, tx, stop2.clone())
                    }
                    _ => Err(format!("Unsupported input format: {format}")),
                }?;
                stream.play().map_err(|e| format!("Start input: {e}"))?;
                Ok(stream)
            })();
            match started {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    while !stop2.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(CaptureHandle {
            stop,
            thread: Some(thread),
        }),
        result => {
            stop.store(true, Ordering::SeqCst);
            Err(match result {
                Ok(Err(e)) => e,
                _ => "Audio device did not start within 5 seconds".into(),
            })
        }
    }
}

pub fn spawn_mic_capture(tx: mpsc::Sender<Vec<i16>>) -> Result<CaptureHandle, String> {
    use cpal::traits::HostTrait;
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No microphone found")?;
    spawn_input(device, tx, "notely-mic")
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
            Err(e) => (
                None,
                Some(format!(
                    "system audio unavailable ({e}); capturing mic only"
                )),
            ),
        };
        let fwd_stop = Arc::new(AtomicBool::new(false));
        let fwd_stop2 = fwd_stop.clone();
        let fwd_thread = std::thread::Builder::new()
            .name("notely-mixer".into())
            .spawn(move || loop {
                let mut forwarded = false;
                for pcm in mic_raw_rx.try_iter().take(20) {
                    forwarded = true;
                    if live_tx.send(("me".into(), pcm)).is_err() {
                        return;
                    }
                }
                for pcm in sys_raw_rx.try_iter().take(20) {
                    forwarded = true;
                    if live_tx.send(("other".into(), pcm)).is_err() {
                        return;
                    }
                }
                if fwd_stop2.load(Ordering::SeqCst) && !forwarded {
                    break;
                }
                if !forwarded {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            })
            .map_err(|e| format!("mixer thread failed: {e}"))?;
        Ok((
            Self {
                mic,
                sys,
                fwd_stop,
                fwd_thread: Some(fwd_thread),
            },
            sys_warning,
        ))
    }

    pub fn stop(mut self) {
        self.mic.stop();
        if let Some(sys) = self.sys.take() {
            sys.stop();
        }
        self.fwd_stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.fwd_thread.take() {
            let _ = t.join();
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
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("notely-loopback".into())
            .spawn(move || {
                if let Err(error) = run_loop(tx, stop2, &ready_tx) {
                    let _ = ready_tx.try_send(Err(error));
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(CaptureHandle {
                stop,
                thread: Some(thread),
            }),
            result => {
                stop.store(true, Ordering::SeqCst);
                Err(match result {
                    Ok(Err(e)) => e,
                    _ => "System audio did not start within 5 seconds".into(),
                })
            }
        }
    }

    /// Direction::Render on the default render endpoint = loopback capture.
    fn run_loop(
        tx: mpsc::Sender<Vec<i16>>,
        stop: Arc<AtomicBool>,
        ready: &mpsc::SyncSender<Result<(), String>>,
    ) -> Result<(), String> {
        initialize_mta()
            .ok()
            .map_err(|e| format!("WASAPI init failed: {e:?}"))?;
        let enumerator =
            DeviceEnumerator::new().map_err(|e| format!("device enum failed: {e:?}"))?;
        let device = enumerator
            .get_default_device(Direction::Render)
            .map_err(|e| format!("no render device: {e:?}"))?;
        let mut client = device
            .get_iaudioclient()
            .map_err(|e| format!("audio client: {e:?}"))?;
        let desired = WaveFormat::new(32, 32, SampleType::Float, 44100, 2, None);
        let (_, period) = client
            .get_device_period()
            .map_err(|e| format!("device period: {e}"))?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: period,
        };
        client
            .initialize_client(&desired, Direction::Capture, mode, None)
            .map_err(|e| format!("init loopback client: {e:?}"))?;
        let event = client
            .set_get_eventhandle()
            .map_err(|e| format!("event handle: {e:?}"))?;
        let capture = client
            .get_audiocaptureclient()
            .map_err(|e| format!("capture client: {e:?}"))?;
        client
            .start_stream()
            .map_err(|e| format!("start stream: {e:?}"))?;
        let _ = ready.send(Ok(()));
        let mut converter = AudioConverter::new(2, 44100)?;
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
                leftover.extend(
                    bytes
                        .chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])),
                );
            }
            while leftover.len() >= 8820 {
                let block: Vec<f32> = leftover.drain(..8820).collect();
                if let Ok(mono) = converter.push(&block) {
                    for chunk in chunk_mono(&mono) {
                        if tx.send(chunk).is_err() {
                            client.stop_stream().ok();
                            return Ok(());
                        }
                    }
                }
            }
            let _ = event.wait_for_event(200);
        }
        client.stop_stream().ok();
        Ok(())
    }
}

#[cfg(not(windows))]
mod loopback_device {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait};

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
                "No loopback input found. On macOS, route meeting audio through a loopback input such as BlackHole to capture other participants."
                    .to_string()
            })?;
        spawn_input(device, tx, "notely-sysaudio")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_resampling_is_callback_boundary_independent_and_downmixes_stereo() {
        for rate in [16000, 44100, 48000] {
            let samples: Vec<f32> = (0..rate)
                .flat_map(|i| {
                    let wave =
                        (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / rate as f32).sin() * 0.5;
                    [wave, wave]
                })
                .collect();
            let whole = AudioConverter::new(2, rate)
                .unwrap()
                .push(&samples)
                .unwrap();
            let mut converter = AudioConverter::new(2, rate).unwrap();
            let mut split = Vec::new();
            for part in samples.chunks(257) {
                split.extend(converter.push(part).unwrap());
            }
            assert_eq!(whole, split, "callback fragmentation at {rate}");
            assert!(
                (15000..=16100).contains(&whole.len()),
                "wrong output duration at {rate}: {}",
                whole.len()
            );
            let peak = whole.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!((12000..20000).contains(&peak));
        }
        let stereo = [1.0, -1.0, 0.25, 0.75];
        assert_eq!(
            AudioConverter::new(2, 16000)
                .unwrap()
                .push(&stereo)
                .unwrap(),
            vec![0, 16384]
        );
    }
}
