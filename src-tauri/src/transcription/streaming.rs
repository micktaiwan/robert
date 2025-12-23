use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Configuration for streaming transcription
#[derive(Clone, Copy)]
pub struct StreamingConfig {
    /// Total audio window length for each transcription (ms)
    pub length_ms: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            length_ms: 5000,   // Use 5 seconds of audio context
        }
    }
}

/// Result from streaming transcription
#[derive(Debug, Clone)]
pub struct StreamingResult {
    /// The transcribed text for this window
    pub text: String,
}

/// Streaming transcriber using sliding window approach
pub struct StreamingTranscriber {
    ctx: WhisperContext,
    config: StreamingConfig,
    /// Ring buffer holding audio samples (at 16kHz)
    audio_buffer: VecDeque<f32>,
    /// Maximum samples to keep in buffer
    max_buffer_samples: usize,
    /// Text confirmed by multiple consecutive transcriptions
    confirmed_text: String,
    /// Previous transcription for comparison (Local Agreement)
    previous_text: String,
    /// Number of consecutive agreements on current text
    agreement_count: usize,
    /// Initial prompt for context continuity
    initial_prompt: String,
}

impl StreamingTranscriber {
    pub fn new<P: AsRef<Path>>(model_path: P, config: StreamingConfig) -> Result<Self> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(anyhow!("Model not found: {}", path.display()));
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid path"))?
            .to_string();

        // Suppress whisper.cpp logs during model loading
        let ctx = suppress_stderr(|| {
            let params = WhisperContextParameters::default();
            WhisperContext::new_with_params(&path_str, params)
        })
        .map_err(|e| anyhow!("Failed to load model: {}", e))?;

        // Calculate max buffer size (keep ~15 seconds max)
        let max_buffer_samples = 16000 * 15; // 15 seconds at 16kHz

        Ok(Self {
            ctx,
            config,
            audio_buffer: VecDeque::with_capacity(max_buffer_samples),
            max_buffer_samples,
            confirmed_text: String::new(),
            previous_text: String::new(),
            agreement_count: 0,
            initial_prompt: String::new(),
        })
    }

    /// Add new audio samples to the buffer
    pub fn push_audio(&mut self, samples: &[f32]) {
        // Add new samples
        self.audio_buffer.extend(samples.iter().copied());

        // Trim if exceeding max size
        while self.audio_buffer.len() > self.max_buffer_samples {
            self.audio_buffer.pop_front();
        }
    }

    /// Transcribe current audio window
    pub fn transcribe(&mut self) -> Result<StreamingResult> {
        let length_samples = (16000 * self.config.length_ms) / 1000;

        // Get audio window (last length_ms of audio)
        let buffer_len = self.audio_buffer.len();
        let start = buffer_len.saturating_sub(length_samples);
        let samples: Vec<f32> = self.audio_buffer.range(start..).copied().collect();

        if samples.is_empty() {
            return Ok(StreamingResult {
                text: String::new(),
            });
        }

        // Transcribe with context
        let text = suppress_stderr(|| self.transcribe_samples(&samples))?;
        let text = text.trim().to_string();

        // Local Agreement: confirm text if it matches previous transcription
        let _ = self.apply_local_agreement(&text);

        Ok(StreamingResult {
            text,
        })
    }

    /// Reset state for a new utterance
    pub fn reset(&mut self) {
        self.audio_buffer.clear();
        self.confirmed_text.clear();
        self.previous_text.clear();
        self.agreement_count = 0;
        self.initial_prompt.clear();
    }

    fn transcribe_samples(&self, samples: &[f32]) -> Result<String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("State error: {}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Set language to auto-detect
        params.set_language(None);
        params.set_translate(false);
        params.set_no_timestamps(true);
        params.set_single_segment(false); // Allow multiple segments for longer audio
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_non_speech_tokens(true);

        // Set initial prompt for context continuity
        if !self.initial_prompt.is_empty() {
            params.set_initial_prompt(&self.initial_prompt);
        }

        state
            .full(params, samples)
            .map_err(|e| anyhow!("Transcription error: {}", e))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| anyhow!("Segments error: {}", e))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
                text.push(' ');
            }
        }

        Ok(text.trim().to_string())
    }

    /// Apply Local Agreement policy to stabilize text
    fn apply_local_agreement(&mut self, current_text: &str) -> String {
        // Find common prefix between previous and current transcription
        let common_prefix = find_common_word_prefix(&self.previous_text, current_text);

        if !common_prefix.is_empty() && common_prefix == self.previous_text {
            // Previous text fully matches current prefix - increase agreement
            self.agreement_count += 1;

            // After 2 agreements, confirm the text
            if self.agreement_count >= 2 && !self.confirmed_text.contains(&common_prefix) {
                // Only add new words that aren't already confirmed
                let new_words = get_new_words(&self.confirmed_text, &common_prefix);
                if !new_words.is_empty() {
                    if !self.confirmed_text.is_empty() {
                        self.confirmed_text.push(' ');
                    }
                    self.confirmed_text.push_str(&new_words);
                }
            }
        } else {
            // Text changed, reset agreement counter
            self.agreement_count = 0;
        }

        self.previous_text = current_text.to_string();
        current_text.to_string()
    }
}

/// Find common word-aligned prefix between two strings
fn find_common_word_prefix(a: &str, b: &str) -> String {
    let words_a: Vec<&str> = a.split_whitespace().collect();
    let words_b: Vec<&str> = b.split_whitespace().collect();

    let mut common = Vec::new();
    for (wa, wb) in words_a.iter().zip(words_b.iter()) {
        if wa.to_lowercase() == wb.to_lowercase() {
            common.push(*wa);
        } else {
            break;
        }
    }

    common.join(" ")
}

/// Get words from new_text that aren't in confirmed_text
fn get_new_words(confirmed: &str, new_text: &str) -> String {
    let confirmed_words: Vec<&str> = confirmed.split_whitespace().collect();
    let new_words: Vec<&str> = new_text.split_whitespace().collect();

    if new_words.len() > confirmed_words.len() {
        new_words[confirmed_words.len()..].join(" ")
    } else {
        String::new()
    }
}

/// Temporarily suppress stderr during a closure execution
fn suppress_stderr<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    unsafe {
        let original_stderr = libc::dup(2);

        if let Ok(devnull) = File::open("/dev/null") {
            libc::dup2(devnull.as_raw_fd(), 2);
        }

        let result = f();

        libc::dup2(original_stderr, 2);
        libc::close(original_stderr);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_word_prefix() {
        assert_eq!(
            find_common_word_prefix("ok robert hello", "ok robert hello world"),
            "ok robert hello"
        );
        assert_eq!(
            find_common_word_prefix("ok robert", "ok robert"),
            "ok robert"
        );
        assert_eq!(find_common_word_prefix("hello", "world"), "");
        assert_eq!(
            find_common_word_prefix("OK Robert", "ok robert test"),
            "OK Robert"
        );
    }

    #[test]
    fn test_get_new_words() {
        assert_eq!(get_new_words("ok robert", "ok robert hello world"), "hello world");
        assert_eq!(get_new_words("", "hello world"), "hello world");
        assert_eq!(get_new_words("hello world", "hello"), "");
    }
}
