use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    pub source: AudioSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AudioSource {
    Microphone,
    System,
}

impl AudioSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            AudioSource::Microphone => "microphone",
            AudioSource::System => "system",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "system" => AudioSource::System,
            _ => AudioSource::Microphone,
        }
    }
}
