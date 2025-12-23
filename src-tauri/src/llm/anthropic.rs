use anyhow::{anyhow, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::tools::ToolResult;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-sonnet-4-20250514";
const MAX_TOKENS: u32 = 4096;
const MAX_ITERATIONS: usize = 30;

// ============================================================================
// Types for Anthropic API
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Serialize)]
struct StreamRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    content_block: Option<ContentBlockEvent>,
    #[serde(default)]
    delta: Option<DeltaEvent>,
}

#[derive(Debug, Deserialize)]
struct ContentBlockEvent {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeltaEvent {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
}


// ============================================================================
// Tool Use Block for internal tracking
// ============================================================================

#[derive(Debug, Clone)]
struct ToolUseBlock {
    id: String,
    name: String,
    input: serde_json::Value,
}

// ============================================================================
// Agentic Client
// ============================================================================

pub struct AgenticClient {
    client: Client,
    api_key: String,
}

impl AgenticClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    /// Run the agentic loop - pattern identical to JS @anthropic-ai/sdk
    ///
    /// Loop until stop_reason == "end_turn" OR no tool_use blocks
    pub async fn run_agentic_loop<F, G>(
        &self,
        messages: &mut Vec<Message>,
        tools: &[ToolDefinition],
        system: &str,
        execute_tool: F,
        on_text: G,
    ) -> Result<String>
    where
        F: Fn(&str, serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send>> + Send + Sync,
        G: Fn(&str) + Send + Sync,
    {
        let mut final_text = String::new();

        for _iteration in 0..MAX_ITERATIONS {
            // Debug: println!("[Iteration {}] Calling Claude API with {} messages...", iteration, messages.len());

            // 1. Create streaming request
            let request = StreamRequest {
                model: MODEL.to_string(),
                max_tokens: MAX_TOKENS,
                system: system.to_string(),
                messages: messages.clone(),
                tools: tools.to_vec(),
                stream: true,
            };

            let response = self.client
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow!("Anthropic API error: {}", error_text));
            }

            // 2. Process SSE stream
            let mut text_content = String::new();
            let mut tool_uses: Vec<ToolUseBlock> = vec![];
            let mut current_tool_input = String::new();
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut stop_reason = String::new();

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);

                // Process complete SSE lines
                while let Some(pos) = buffer.find("\n\n") {
                    let line = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Parse SSE event - find the data: line within the block
                    let data_line = line.lines()
                        .find(|l| l.starts_with("data: "))
                        .and_then(|l| l.strip_prefix("data: "));

                    if let Some(data) = data_line {
                        if data == "[DONE]" {
                            continue;
                        }

                        if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                            match event.event_type.as_str() {
                                // content_block_start: beginning of text or tool_use
                                "content_block_start" => {
                                    if let Some(cb) = event.content_block {
                                        if cb.block_type == "tool_use" {
                                            current_tool_id = cb.id.unwrap_or_default();
                                            current_tool_name = cb.name.unwrap_or_default();
                                            current_tool_input.clear();
                                            println!("[Tool Start] {}", current_tool_name);
                                        }
                                    }
                                }

                                // content_block_delta: text chunks or partial JSON
                                "content_block_delta" => {
                                    if let Some(delta) = event.delta {
                                        match delta.delta_type.as_deref() {
                                            Some("text_delta") => {
                                                if let Some(text) = delta.text {
                                                    on_text(&text);
                                                    text_content.push_str(&text);
                                                }
                                            }
                                            Some("input_json_delta") => {
                                                if let Some(json) = delta.partial_json {
                                                    current_tool_input.push_str(&json);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                // content_block_stop: end of a block
                                "content_block_stop" => {
                                    if !current_tool_id.is_empty() {
                                        let input: serde_json::Value = serde_json::from_str(&current_tool_input)
                                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                                        tool_uses.push(ToolUseBlock {
                                            id: current_tool_id.clone(),
                                            name: current_tool_name.clone(),
                                            input,
                                        });

                                        current_tool_id.clear();
                                        current_tool_name.clear();
                                        current_tool_input.clear();
                                    }
                                }

                                // message_delta: contains stop_reason
                                "message_delta" => {
                                    if let Some(delta) = event.delta {
                                        if let Some(reason) = delta.stop_reason {
                                            stop_reason = reason;
                                        }
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                }
            }

            // Debug: println!("[Iteration {}] Stop reason: {}, Tool uses: {}", iteration, stop_reason, tool_uses.len());

            // 3. Build assistant content for history
            let mut assistant_content: Vec<ContentBlock> = vec![];
            if !text_content.is_empty() {
                assistant_content.push(ContentBlock::Text { text: text_content.clone() });
            }
            for tu in &tool_uses {
                assistant_content.push(ContentBlock::ToolUse {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    input: tu.input.clone(),
                });
            }

            // 4. Add assistant response to history (only if non-empty)
            if !assistant_content.is_empty() {
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: assistant_content,
                });
            }

            // 5. Check stop condition
            if stop_reason == "end_turn" || tool_uses.is_empty() {
                final_text = text_content;
                break;
            }

            // 6. Execute tools IN PARALLEL
            let tool_futures: Vec<_> = tool_uses.iter()
                .map(|tu| execute_tool(&tu.name, tu.input.clone()))
                .collect();

            let results = futures::future::join_all(tool_futures).await;

            // 7. Build tool_results
            let tool_results: Vec<ContentBlock> = tool_uses.iter()
                .zip(results)
                .map(|(tu, result)| {
                    let content = match result {
                        ToolResult::Success(msg) => {
                            // Truncate log to avoid repeating full content
                            let preview = if msg.len() > 80 {
                                format!("{}...", &msg[..80])
                            } else {
                                msg.clone()
                            };
                            println!("[Tool OK] {}: {}", tu.name, preview);
                            msg
                        }
                        ToolResult::Error(err) => {
                            println!("[Tool Error] {}: {}", tu.name, err);
                            format!("Error: {}", err)
                        }
                        ToolResult::Exit => {
                            println!("[Tool Exit] {}", tu.name);
                            "Exiting application".to_string()
                        }
                    };

                    ContentBlock::ToolResult {
                        tool_use_id: tu.id.clone(),
                        content,
                    }
                })
                .collect();

            // 8. Add user message with tool_results
            messages.push(Message {
                role: "user".to_string(),
                content: tool_results,
            });
        }

        Ok(final_text)
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Create a user message with text content
pub fn user_message(text: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text { text: text.to_string() }],
    }
}

/// Summarize text using Claude (non-streaming, simple completion)
pub async fn summarize(api_key: &str, text: &str) -> Result<String> {
    let client = Client::new();

    #[derive(Serialize)]
    struct Request {
        model: String,
        max_tokens: u32,
        messages: Vec<Message>,
    }

    #[derive(Deserialize)]
    struct Response {
        content: Vec<ContentBlockResponse>,
    }

    #[derive(Deserialize)]
    struct ContentBlockResponse {
        #[serde(rename = "type")]
        block_type: String,
        #[serde(default)]
        text: Option<String>,
    }

    let request = Request {
        model: MODEL.to_string(),
        max_tokens: 2048,
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Summarize the following audio transcription. Adapt your summary to the content type:\n\
                    - For formal meetings: key points, decisions, action items\n\
                    - For informal conversations: main topics discussed, people mentioned, any plans or intentions\n\
                    - For any content: always provide a useful summary, never refuse\n\n\
                    Keep it concise. Respond in the same language as the transcription.\n\n{}",
                    text
                ),
            }],
        }],
    };

    let response = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(anyhow!("Anthropic API error: {}", error_text));
    }

    let response: Response = response.json().await?;

    for block in response.content {
        if block.block_type == "text" {
            if let Some(text) = block.text {
                return Ok(text);
            }
        }
    }

    Err(anyhow!("No text response from Anthropic"))
}
