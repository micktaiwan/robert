use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const TARGET_SAMPLE_RATE: u32 = 16000; // Whisper expects 16kHz

// VAD parameters (defaults, can be overridden via VadConfig)
const DEFAULT_SPEECH_THRESHOLD: f32 = 0.006; // Amplitude threshold for speech detection (lower = more sensitive)
const DEFAULT_SILENCE_DURATION_MS: usize = 1000; // How long silence before we consider speech ended
const MIN_SPEECH_DURATION_MS: usize = 400; // Minimum speech duration to process
const MAX_SPEECH_DURATION_MS: usize = 10000; // Max duration before forced processing

// Streaming mode parameters
const STREAMING_CHUNK_MS: usize = 600; // Send chunks every 600ms for streaming transcription

#[derive(Clone, Copy)]
pub struct VadConfig {
    pub speech_threshold: f32,
    pub silence_duration_ms: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            speech_threshold: DEFAULT_SPEECH_THRESHOLD,
            silence_duration_ms: DEFAULT_SILENCE_DURATION_MS,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Audio events for streaming mode
#[derive(Clone, Debug)]
pub enum AudioEvent {
    /// Streaming chunk during speech (for real-time transcription)
    StreamingChunk(Vec<f32>),
    /// Complete utterance after silence detected
    SpeechEnded(Vec<f32>),
}

pub struct AudioCapture {
    device: Device,
    config: StreamConfig,
    native_sample_rate: u32,
    is_recording: Arc<AtomicBool>,
    audio_sender: Sender<Vec<f32>>,
    // Streaming mode channels
    event_sender: Sender<AudioEvent>,
    event_receiver: Receiver<AudioEvent>,
    vad_config: VadConfig,
}

impl AudioCapture {
    pub fn list_input_devices() -> Result<Vec<DeviceInfo>> {
        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        let devices: Vec<DeviceInfo> = host
            .input_devices()?
            .filter_map(|d| {
                d.name().ok().map(|name| DeviceInfo {
                    is_default: Some(&name) == default_name.as_ref(),
                    name,
                })
            })
            .collect();

        Ok(devices)
    }

    pub fn new(vad_config: VadConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;
        Self::from_device(device, vad_config)
    }

    pub fn new_with_device(device_name: &str, vad_config: VadConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(device_name))
            .ok_or_else(|| anyhow!("Device not found: {}", device_name))?;
        Self::from_device(device, vad_config)
    }

    fn from_device(device: Device, vad_config: VadConfig) -> Result<Self> {
        let default_config = device.default_input_config()?;

        let stream_config = StreamConfig {
            channels: default_config.channels(),
            sample_rate: default_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let native_sample_rate = default_config.sample_rate().0;

        let (sender, _receiver) = bounded(100);
        let (event_sender, event_receiver) = bounded(100);

        Ok(Self {
            device,
            config: stream_config,
            native_sample_rate,
            is_recording: Arc::new(AtomicBool::new(false)),
            audio_sender: sender,
            event_sender,
            event_receiver,
            vad_config,
        })
    }

    pub fn device_name(&self) -> Option<String> {
        self.device.name().ok()
    }

    pub fn start(&self) -> Result<Stream> {
        let sender = self.audio_sender.clone();
        let event_sender = self.event_sender.clone();
        let is_recording = self.is_recording.clone();
        let channels = self.config.channels as usize;
        let native_rate = self.native_sample_rate;
        let resample_ratio = native_rate as f64 / TARGET_SAMPLE_RATE as f64;

        // Get VAD config values
        let speech_threshold = self.vad_config.speech_threshold;
        let silence_duration_ms = self.vad_config.silence_duration_ms;

        // Calculate sample counts for VAD
        let silence_samples = (native_rate as usize * silence_duration_ms) / 1000;
        let min_speech_samples = (native_rate as usize * MIN_SPEECH_DURATION_MS) / 1000;
        let max_speech_samples = (native_rate as usize * MAX_SPEECH_DURATION_MS) / 1000;

        // Streaming mode: send chunks every STREAMING_CHUNK_MS
        let streaming_chunk_samples = (native_rate as usize * STREAMING_CHUNK_MS) / 1000;

        println!("[VAD] Using speech_threshold={}, silence_duration_ms={}", speech_threshold, silence_duration_ms);

        is_recording.store(true, Ordering::SeqCst);

        let stream = self.device.build_input_stream(
            &self.config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                thread_local! {
                    static STATE: std::cell::RefCell<VadState> = std::cell::RefCell::new(VadState::new());
                }

                if !is_recording.load(Ordering::SeqCst) {
                    return;
                }

                STATE.with(|state| {
                    let mut state = state.borrow_mut();

                    // Convert to mono
                    let mono_samples: Vec<f32> = if channels >= 2 {
                        data.chunks(channels)
                            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                            .collect()
                    } else {
                        data.to_vec()
                    };

                    // Calculate RMS amplitude for this chunk
                    let rms = (mono_samples.iter().map(|s| s * s).sum::<f32>() / mono_samples.len() as f32).sqrt();
                    let is_speech = rms > speech_threshold;

                    // Add samples to buffer
                    let samples_added = mono_samples.len();
                    state.buffer.extend(mono_samples);
                    state.samples_since_last_chunk += samples_added;

                    if is_speech {
                        state.silence_counter = 0;
                        if !state.speech_started {
                            state.speech_started = true;
                        }
                    } else if state.speech_started {
                        state.silence_counter += data.len() / channels;
                    }

                    // STREAMING: Send chunks during speech for real-time transcription
                    if state.speech_started && state.samples_since_last_chunk >= streaming_chunk_samples {
                        // Send streaming chunk with all audio so far (resampled to 16kHz)
                        let resampled = resample(&state.buffer, resample_ratio);
                        let _ = event_sender.try_send(AudioEvent::StreamingChunk(resampled));
                        state.samples_since_last_chunk = 0;
                    }

                    // Check if we should send the final buffer (speech ended)
                    let should_send = state.speech_started && (
                        // Speech ended (enough silence)
                        (state.silence_counter >= silence_samples && state.buffer.len() >= min_speech_samples) ||
                        // Max duration reached
                        state.buffer.len() >= max_speech_samples
                    );

                    if should_send {
                        // Trim trailing silence (keep a bit for natural ending)
                        let trim_samples = state.silence_counter.saturating_sub(native_rate as usize / 10);
                        let end = state.buffer.len().saturating_sub(trim_samples);
                        let audio_to_send: Vec<f32> = state.buffer[..end].to_vec();

                        if audio_to_send.len() >= min_speech_samples {
                            let resampled = resample(&audio_to_send, resample_ratio);
                            // Send to both channels for compatibility
                            let _ = sender.try_send(resampled.clone());
                            let _ = event_sender.try_send(AudioEvent::SpeechEnded(resampled));
                        }

                        state.reset();
                    }

                    // Prevent buffer from growing too large when no speech
                    if !state.speech_started && state.buffer.len() > native_rate as usize {
                        state.buffer.clear();
                    }
                });
            },
            move |_err| {},
            None,
        )?;

        stream.play()?;
        Ok(stream)
    }

    /// Get receiver for streaming audio events (chunks during speech + final utterance)
    pub fn event_receiver(&self) -> Receiver<AudioEvent> {
        self.event_receiver.clone()
    }
}

struct VadState {
    buffer: Vec<f32>,
    speech_started: bool,
    silence_counter: usize,
    // For streaming mode: track samples since last streaming chunk
    samples_since_last_chunk: usize,
}

impl VadState {
    fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(48000 * 10),
            speech_started: false,
            silence_counter: 0,
            samples_since_last_chunk: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.speech_started = false;
        self.silence_counter = 0;
        self.samples_since_last_chunk = 0;
    }
}

fn resample(input: &[f32], ratio: f64) -> Vec<f32> {
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let src_floor = src_idx.floor() as usize;
        let src_ceil = (src_floor + 1).min(input.len() - 1);
        let frac = src_idx - src_floor as f64;

        let sample = input[src_floor] * (1.0 - frac as f32) + input[src_ceil] * frac as f32;
        output.push(sample);
    }

    output
}
