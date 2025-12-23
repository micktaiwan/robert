use crate::llm::ToolDefinition;
use serde_json::json;

/// Get tool definitions for Anthropic API
/// Format: { name, description, input_schema: JSON Schema }
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "quit".to_string(),
            description: "Close and quit the Robert application".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "list_recordings".to_string(),
            description: "List all saved recordings/meetings with their names and dates. Note: 'recording' and 'meeting' refer to the same thing.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "summarize_recording".to_string(),
            description: "Generate a summary of a specific recording's transcription. Use either recording_name OR recording_index.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "recording_name": {
                        "type": "string",
                        "description": "The exact name of the recording"
                    },
                    "recording_index": {
                        "type": "integer",
                        "description": "The position of the recording (1 = first/most recent, 2 = second, -1 = last/oldest)"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "start_recording".to_string(),
            description: "Start a new meeting recording session".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Optional name for the recording"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "stop_recording".to_string(),
            description: "Stop the current recording session".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_recording_content".to_string(),
            description: "Get the full transcription content of a specific recording. Use either recording_name OR recording_index.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "recording_name": {
                        "type": "string",
                        "description": "The exact name of the recording"
                    },
                    "recording_index": {
                        "type": "integer",
                        "description": "The position of the recording (1 = first/most recent, 2 = second, -1 = last/oldest)"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "rename_recording".to_string(),
            description: "Rename an existing recording/meeting. Use either recording_name OR recording_index to identify it.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "recording_name": {
                        "type": "string",
                        "description": "The current name of the recording"
                    },
                    "recording_index": {
                        "type": "integer",
                        "description": "The position of the recording (1 = first/most recent, 2 = second, -1 = last/oldest)"
                    },
                    "new_name": {
                        "type": "string",
                        "description": "The new name for the recording"
                    }
                },
                "required": ["new_name"]
            }),
        },
        ToolDefinition {
            name: "delete_recording".to_string(),
            description: "Delete a recording/meeting permanently. Use either recording_name OR recording_index to identify it. Ask for confirmation before deleting.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "recording_name": {
                        "type": "string",
                        "description": "The name of the recording to delete"
                    },
                    "recording_index": {
                        "type": "integer",
                        "description": "The position of the recording (1 = first/most recent, 2 = second, -1 = last/oldest)"
                    }
                },
                "required": []
            }),
        },
    ]
}
