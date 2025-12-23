use crate::llm::Message;
use crate::mcp::McpServerConfig;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Default)]
pub struct AppState {
    pub settings: Settings,
    pub active_recording: Option<ActiveRecording>,
    pub conversation_history: Vec<Message>,
}

impl AppState {
    pub fn load() -> Self {
        Self {
            settings: Settings::load().unwrap_or_default(),
            active_recording: None,
            conversation_history: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActiveRecording {
    pub id: Uuid,
    pub name: String,
}

#[derive(Clone, Serialize, Default)]
pub struct CopilotUIState {
    pub visible: bool,
    pub state: String,
    pub response_text: String,
    pub should_close: bool,
}

impl CopilotUIState {
    pub fn new() -> Self {
        Self {
            visible: false,
            state: "idle".to_string(),
            response_text: String::new(),
            should_close: false,
        }
    }

    pub fn reset(&mut self) {
        self.visible = false;
        self.state = "idle".to_string();
        self.response_text.clear();
        self.should_close = false;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub speech_threshold: f32,
    pub silence_duration_ms: usize,
    pub wake_words: Vec<String>,
    pub whisper_model: String,
    pub mic_device: Option<String>,
    pub system_audio_device: Option<String>,
    pub anthropic_api_key: Option<String>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speech_threshold: 0.006,
            silence_duration_ms: 1000,
            wake_words: vec!["ok robert".into(), "hey robert".into()],
            whisper_model: "ggml-small.bin".into(),
            mic_device: None,
            system_audio_device: None,
            anthropic_api_key: None,
            mcp_servers: Vec::new(),
        }
    }
}

impl Settings {
    fn settings_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "robert", "Robert")
            .map(|dirs| dirs.data_dir().join("settings.json"))
    }

    pub fn load() -> Option<Self> {
        let path = Self::settings_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::settings_path().ok_or("Could not determine settings path")?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())?;

        Ok(())
    }
}
