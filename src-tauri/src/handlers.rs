use crate::audio::{AudioCapture, DeviceInfo};
use crate::state::{ActiveRecording, AppState, CopilotUIState, Settings};
use crate::storage::{Recording, Transcription};
use crate::DbState;
use crate::CopilotState;
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub size_mb: u64,
    pub model_type: String,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<RwLock<AppState>>>) -> Result<Settings, String> {
    let state = state.read().await;
    Ok(state.settings.clone())
}

#[tauri::command]
pub async fn save_settings(
    settings: Settings,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    // Save to disk first
    settings.save()?;

    // Then update in-memory state
    let mut state = state.write().await;
    state.settings = settings;
    Ok(())
}

#[tauri::command]
pub async fn get_models(app: tauri::AppHandle) -> Result<Vec<ModelInfo>, String> {
    use tauri::Manager;

    let mut models = Vec::new();

    // Try dev path first (relative to src-tauri/)
    let dev_models_dir = std::path::PathBuf::from("models");

    // Production path: ~/Library/Application Support/com.robert.Robert/models/
    let prod_models_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");

    // Use whichever exists
    let models_dir = if dev_models_dir.exists() {
        dev_models_dir
    } else {
        prod_models_dir
    };

    if models_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let size = entry.metadata().map(|m| m.len() / (1024 * 1024)).unwrap_or(0);

                    let model_type = if name.ends_with(".bin") {
                        "Whisper"
                    } else {
                        continue;
                    };

                    models.push(ModelInfo {
                        name: name.to_string(),
                        size_mb: size,
                        model_type: model_type.to_string(),
                    });
                }
            }
        }
    }

    Ok(models)
}

#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<DeviceInfo>, String> {
    AudioCapture::list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_recording(
    name: Option<String>,
    state: State<'_, Arc<RwLock<AppState>>>,
    db: State<'_, DbState>,
    app: tauri::AppHandle,
) -> Result<Recording, String> {
    let mut state = state.write().await;

    if state.active_recording.is_some() {
        return Err("A recording is already in progress".to_string());
    }

    let recording_name = name.unwrap_or_else(|| {
        format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M"))
    });

    let recording = {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.create_recording(&recording_name).map_err(|e| e.to_string())?
    };

    state.active_recording = Some(ActiveRecording {
        id: recording.id,
        name: recording.name.clone(),
    });

    let _ = app.emit("recording-started", &recording.name);

    Ok(recording)
}

#[tauri::command]
pub async fn stop_recording(
    state: State<'_, Arc<RwLock<AppState>>>,
    db: State<'_, DbState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut state = state.write().await;

    let active = state
        .active_recording
        .take()
        .ok_or("No recording in progress")?;

    {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.end_recording(active.id).map_err(|e| e.to_string())?;
    }

    let _ = app.emit("recording-stopped", &active.name);

    Ok(())
}

#[tauri::command]
pub async fn list_recordings(
    db: State<'_, DbState>,
) -> Result<Vec<Recording>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.list_recordings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recording_transcriptions(
    recording_id: String,
    db: State<'_, DbState>,
) -> Result<Vec<Transcription>, String> {
    let id = Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_transcriptions(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_recording(
    recording_id: String,
    new_name: String,
    db: State<'_, DbState>,
) -> Result<(), String> {
    let id = Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    db.rename_recording(id, &new_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_recording(
    recording_id: String,
    db: State<'_, DbState>,
) -> Result<(), String> {
    let id = Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    db.delete_recording(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recording_status(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<String>, String> {
    let state = state.read().await;
    Ok(state.active_recording.as_ref().map(|r| r.name.clone()))
}

#[tauri::command]
pub fn get_copilot_state(
    copilot: State<'_, CopilotState>,
) -> Result<CopilotUIState, String> {
    let state = copilot.read().map_err(|e| e.to_string())?;
    Ok(state.clone())
}

#[tauri::command]
pub async fn test_mcp_server(url: String) -> Result<Vec<String>, String> {
    crate::mcp::test_mcp_server(&url)
        .await
        .map_err(|e| e.to_string())
}
