use crate::llm::summarize;
use crate::mcp::McpManager;
use crate::state::AppState;
use crate::tools::ToolSource;
use crate::DbState;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

pub enum ToolResult {
    Success(String),
    Error(String),
    Exit,
}

#[derive(Deserialize)]
struct RecordingSelector {
    recording_name: Option<String>,
    recording_index: Option<i32>,
}

#[derive(Deserialize)]
struct StartRecordingInput {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RenameInput {
    recording_name: Option<String>,
    recording_index: Option<i32>,
    new_name: String,
}

#[derive(Clone)]
pub struct ToolExecutor {
    app_handle: AppHandle,
    state: Arc<RwLock<AppState>>,
    db: Option<DbState>,
    mcp_manager: Option<Arc<McpManager>>,
    routing: Arc<HashMap<String, ToolSource>>,
}

use crate::storage::Recording;

impl ToolExecutor {
    pub fn new(
        app_handle: AppHandle,
        state: Arc<RwLock<AppState>>,
        db: Option<DbState>,
        mcp_manager: Option<Arc<McpManager>>,
        routing: HashMap<String, ToolSource>,
    ) -> Self {
        Self {
            app_handle,
            state,
            db,
            mcp_manager,
            routing: Arc::new(routing),
        }
    }

    /// Resolve a recording from either name or index
    fn resolve_recording(
        db: &crate::storage::Database,
        name: Option<&str>,
        index: Option<i32>,
    ) -> Result<Recording, String> {
        if let Some(name) = name {
            match db.get_recording_by_name(name) {
                Ok(Some(r)) => return Ok(r),
                Ok(None) => return Err(format!("Recording '{}' not found", name)),
                Err(e) => return Err(format!("Database error: {}", e)),
            }
        }

        if let Some(idx) = index {
            let recordings = db.list_recordings().map_err(|e| e.to_string())?;
            if recordings.is_empty() {
                return Err("No recordings found".to_string());
            }

            // Handle negative indices (-1 = last, -2 = second to last)
            let actual_idx = if idx < 0 {
                (recordings.len() as i32 + idx) as usize
            } else if idx == 0 {
                return Err("Index must be non-zero (1 = first, -1 = last)".to_string());
            } else {
                (idx - 1) as usize // Convert 1-based to 0-based
            };

            if actual_idx >= recordings.len() {
                return Err(format!(
                    "Index {} out of range. You have {} recording(s).",
                    idx,
                    recordings.len()
                ));
            }

            return Ok(recordings[actual_idx].clone());
        }

        Err("Please specify either a recording name or index (e.g., 'first', 'second', 'last')".to_string())
    }

    pub async fn execute(&self, tool_name: &str, input: serde_json::Value) -> ToolResult {
        // Look up routing to determine where to execute the tool
        let source = match self.routing.get(tool_name) {
            Some(s) => s.clone(),
            None => return ToolResult::Error(format!("Unknown tool: {}", tool_name)),
        };

        match source {
            ToolSource::Local => self.execute_local(tool_name, input).await,
            ToolSource::Mcp {
                server_id,
                original_name,
            } => self.execute_mcp(&server_id, &original_name, input).await,
        }
    }

    /// Execute a local tool
    async fn execute_local(&self, tool_name: &str, input: serde_json::Value) -> ToolResult {
        match tool_name {
            "quit" => self.execute_quit(),
            "list_recordings" => self.execute_list_recordings().await,
            "summarize_recording" => self.execute_summarize(input).await,
            "start_recording" => self.execute_start_recording(input).await,
            "stop_recording" => self.execute_stop_recording().await,
            "get_recording_content" => self.execute_get_content(input).await,
            "rename_recording" => self.execute_rename(input).await,
            "delete_recording" => self.execute_delete(input).await,
            _ => ToolResult::Error(format!("Unknown local tool: {}", tool_name)),
        }
    }

    /// Execute a tool on an MCP server
    async fn execute_mcp(
        &self,
        server_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> ToolResult {
        let manager = match &self.mcp_manager {
            Some(m) => m,
            None => return ToolResult::Error("MCP not configured".to_string()),
        };

        match manager.call_tool(server_id, tool_name, input).await {
            Ok(result) => ToolResult::Success(result),
            Err(e) => ToolResult::Error(format!("MCP tool error: {}", e)),
        }
    }

    fn execute_quit(&self) -> ToolResult {
        self.app_handle.exit(0);
        ToolResult::Exit
    }

    async fn execute_list_recordings(&self) -> ToolResult {
        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        match db.list_recordings() {
            Ok(recordings) => {
                if recordings.is_empty() {
                    return ToolResult::Success("No recordings found.".to_string());
                }
                let summary = recordings
                    .iter()
                    .map(|r| {
                        let status = if r.is_active { " (active)" } else { "" };
                        format!(
                            "- {} ({}){}",
                            r.name,
                            r.created_at.format("%Y-%m-%d %H:%M"),
                            status
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                ToolResult::Success(format!("Recordings:\n{}", summary))
            }
            Err(e) => ToolResult::Error(format!("Failed to list recordings: {}", e)),
        }
    }

    async fn execute_summarize(&self, input: serde_json::Value) -> ToolResult {
        let input: RecordingSelector = match serde_json::from_value(input) {
            Ok(i) => i,
            Err(e) => return ToolResult::Error(format!("Invalid input: {}", e)),
        };

        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let api_key = {
            let state = self.state.read().await;
            match &state.settings.anthropic_api_key {
                Some(key) => key.clone(),
                None => return ToolResult::Error("Anthropic API key not configured".to_string()),
            }
        };

        let (recording, text) = {
            let db = match db.lock() {
                Ok(db) => db,
                Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
            };

            // Find recording by name or index
            let recording = match Self::resolve_recording(
                &db,
                input.recording_name.as_deref(),
                input.recording_index,
            ) {
                Ok(r) => r,
                Err(e) => return ToolResult::Error(e),
            };

            // Get full transcription text
            let text = match db.get_full_transcription_text(recording.id) {
                Ok(t) if t.is_empty() => {
                    return ToolResult::Error(format!(
                        "Recording '{}' has no transcriptions yet",
                        recording.name
                    ))
                }
                Ok(t) => t,
                Err(e) => return ToolResult::Error(format!("Failed to get transcription: {}", e)),
            };

            (recording, text)
        };

        // Call Anthropic to summarize
        match summarize(&api_key, &text).await {
            Ok(summary) => ToolResult::Success(format!("Summary of '{}':\n\n{}", recording.name, summary)),
            Err(e) => ToolResult::Error(format!("Failed to summarize: {}", e)),
        }
    }

    async fn execute_start_recording(&self, input: serde_json::Value) -> ToolResult {
        let input: StartRecordingInput = serde_json::from_value(input).unwrap_or(StartRecordingInput { name: None });

        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let mut state = self.state.write().await;

        // Check if already recording
        if state.active_recording.is_some() {
            return ToolResult::Error("A recording is already in progress".to_string());
        }

        let name = input
            .name
            .unwrap_or_else(|| format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M")));

        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        match db.create_recording(&name) {
            Ok(recording) => {
                let id = recording.id;
                state.active_recording = Some(crate::state::ActiveRecording {
                    id,
                    name: recording.name.clone(),
                });

                // Emit event
                let _ = self.app_handle.emit("recording-started", &recording.name);

                ToolResult::Success(format!("Started recording: {}", recording.name))
            }
            Err(e) => ToolResult::Error(format!("Failed to start recording: {}", e)),
        }
    }

    async fn execute_stop_recording(&self) -> ToolResult {
        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let mut state = self.state.write().await;

        let active = match state.active_recording.take() {
            Some(a) => a,
            None => return ToolResult::Error("No recording in progress".to_string()),
        };

        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        match db.end_recording(active.id) {
            Ok(_) => {
                // Emit event
                let _ = self.app_handle.emit("recording-stopped", &active.name);

                ToolResult::Success(format!("Stopped recording: {}", active.name))
            }
            Err(e) => ToolResult::Error(format!("Failed to stop recording: {}", e)),
        }
    }

    async fn execute_get_content(&self, input: serde_json::Value) -> ToolResult {
        let input: RecordingSelector = match serde_json::from_value(input) {
            Ok(i) => i,
            Err(e) => return ToolResult::Error(format!("Invalid input: {}", e)),
        };

        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        // Find recording by name or index
        let recording = match Self::resolve_recording(
            &db,
            input.recording_name.as_deref(),
            input.recording_index,
        ) {
            Ok(r) => r,
            Err(e) => return ToolResult::Error(e),
        };

        // Get full transcription text
        match db.get_full_transcription_text(recording.id) {
            Ok(text) if text.is_empty() => {
                ToolResult::Success(format!("Recording '{}' has no transcriptions yet.", recording.name))
            }
            Ok(text) => {
                ToolResult::Success(format!("Content of '{}':\n\n{}", recording.name, text))
            }
            Err(e) => ToolResult::Error(format!("Failed to get transcription: {}", e)),
        }
    }

    async fn execute_rename(&self, input: serde_json::Value) -> ToolResult {
        let input: RenameInput = match serde_json::from_value(input) {
            Ok(i) => i,
            Err(e) => return ToolResult::Error(format!("Invalid input: {}", e)),
        };

        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        // Find recording by name or index
        let recording = match Self::resolve_recording(
            &db,
            input.recording_name.as_deref(),
            input.recording_index,
        ) {
            Ok(r) => r,
            Err(e) => return ToolResult::Error(e),
        };

        let old_name = recording.name.clone();

        // Rename the recording
        match db.rename_recording(recording.id, &input.new_name) {
            Ok(_) => {
                ToolResult::Success(format!(
                    "Renamed '{}' to '{}'",
                    old_name, input.new_name
                ))
            }
            Err(e) => ToolResult::Error(format!("Failed to rename recording: {}", e)),
        }
    }

    async fn execute_delete(&self, input: serde_json::Value) -> ToolResult {
        let input: RecordingSelector = match serde_json::from_value(input) {
            Ok(i) => i,
            Err(e) => return ToolResult::Error(format!("Invalid input: {}", e)),
        };

        let db = match &self.db {
            Some(db) => db,
            None => return ToolResult::Error("Database not initialized".to_string()),
        };

        // Find recording (release lock before await)
        let recording = {
            let db = match db.lock() {
                Ok(db) => db,
                Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
            };

            match Self::resolve_recording(
                &db,
                input.recording_name.as_deref(),
                input.recording_index,
            ) {
                Ok(r) => r,
                Err(e) => return ToolResult::Error(e),
            }
        }; // db lock released here

        // Check if recording is active (now safe to await)
        {
            let state = self.state.read().await;
            if let Some(active) = &state.active_recording {
                if active.id == recording.id {
                    return ToolResult::Error(format!(
                        "Cannot delete '{}' - recording is currently active. Stop it first.",
                        recording.name
                    ));
                }
            }
        }

        let name = recording.name.clone();

        // Re-acquire lock for deletion
        let db = match db.lock() {
            Ok(db) => db,
            Err(e) => return ToolResult::Error(format!("Database lock error: {}", e)),
        };

        // Delete the recording
        match db.delete_recording(recording.id) {
            Ok(_) => {
                ToolResult::Success(format!("Deleted recording '{}'", name))
            }
            Err(e) => ToolResult::Error(format!("Failed to delete recording: {}", e)),
        }
    }
}
