use anyhow::{anyhow, Result};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Temporarily suppress stderr during a closure execution
fn with_stderr_suppressed<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    unsafe {
        // Save original stderr
        let original_stderr = libc::dup(2);

        // Open /dev/null and redirect stderr to it
        if let Ok(devnull) = File::open("/dev/null") {
            libc::dup2(devnull.as_raw_fd(), 2);
        }

        // Execute the closure
        let result = f();

        // Restore original stderr
        libc::dup2(original_stderr, 2);
        libc::close(original_stderr);

        result
    }
}

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(anyhow!("Model not found: {}", path.display()));
        }

        let path_str = path.to_str().ok_or_else(|| anyhow!("Invalid path"))?.to_string();

        // Suppress whisper.cpp logs during model loading
        let ctx = with_stderr_suppressed(|| {
            let params = WhisperContextParameters::default();
            WhisperContext::new_with_params(&path_str, params)
        })
        .map_err(|e| anyhow!("Failed to load model: {}", e))?;

        Ok(Self { ctx })
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        // Suppress whisper.cpp logs during transcription
        with_stderr_suppressed(|| {
            let mut state = self.ctx.create_state()
                .map_err(|e| anyhow!("State error: {}", e))?;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

            // Auto-detect language for better accuracy
            params.set_language(None);
            params.set_translate(false);
            params.set_no_timestamps(true);
            params.set_single_segment(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            params.set_suppress_non_speech_tokens(true);

            state.full(params, samples)
                .map_err(|e| anyhow!("Transcription error: {}", e))?;

            let num_segments = state.full_n_segments()
                .map_err(|e| anyhow!("Segments error: {}", e))?;

            let mut text = String::new();
            for i in 0..num_segments {
                if let Ok(segment) = state.full_get_segment_text(i) {
                    text.push_str(&segment);
                    text.push(' ');
                }
            }

            Ok(text.trim().to_string())
        })
    }
}
