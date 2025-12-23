use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Configuration for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

/// Tool information from MCP server
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_id: String,
}

/// JSON-RPC request structure
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response structure
#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u32>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i32,
    message: String,
}

/// Simple JSON-RPC client for MCP servers
struct McpClient {
    client: reqwest::Client,
    url: String,
}

impl McpClient {
    fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: method.to_string(),
            params,
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let rpc_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;

        if let Some(error) = rpc_response.error {
            return Err(anyhow!("RPC error: {}", error.message));
        }

        rpc_response
            .result
            .ok_or_else(|| anyhow!("No result in response"))
    }
}

/// Manages connections to MCP servers
pub struct McpManager {
    configs: Vec<McpServerConfig>,
}

impl McpManager {
    pub fn new(configs: Vec<McpServerConfig>) -> Self {
        Self { configs }
    }

    /// Get config for a specific server
    fn get_config(&self, server_id: &str) -> Option<&McpServerConfig> {
        self.configs.iter().find(|c| c.id == server_id)
    }

    /// List tools from all enabled servers
    pub async fn list_all_tools(&self) -> Vec<McpToolInfo> {
        let mut all_tools = Vec::new();

        for config in &self.configs {
            if !config.enabled {
                continue;
            }

            match self.list_tools_from_server(&config.id).await {
                Ok(tools) => all_tools.extend(tools),
                Err(e) => {
                    eprintln!(
                        "[MCP] Warning: Could not list tools from {}: {}",
                        config.name, e
                    );
                }
            }
        }

        all_tools
    }

    /// List tools from a specific server
    pub async fn list_tools_from_server(&self, server_id: &str) -> Result<Vec<McpToolInfo>> {
        let config = self
            .get_config(server_id)
            .ok_or_else(|| anyhow!("Unknown MCP server: {}", server_id))?;

        let client = McpClient::new(&config.url);
        let result = client.call("tools/list", None).await?;

        // Parse tools from response
        let tools_array = result
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| anyhow!("Invalid tools response"))?;

        let tools = tools_array
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name")?.as_str()?;
                let description = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let input_schema = tool.get("inputSchema").cloned().unwrap_or(Value::Null);

                Some(McpToolInfo {
                    name: name.to_string(),
                    description: description.to_string(),
                    input_schema,
                    server_id: server_id.to_string(),
                })
            })
            .collect();

        Ok(tools)
    }

    /// Call a tool on a specific server
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<String> {
        let config = self
            .get_config(server_id)
            .ok_or_else(|| anyhow!("Unknown MCP server: {}", server_id))?;

        let client = McpClient::new(&config.url);
        let params = json!({
            "name": tool_name,
            "arguments": arguments
        });

        let result = client.call("tools/call", Some(params)).await?;

        // Extract content from response
        let content = result.get("content").and_then(|c| c.as_array());

        let text = if let Some(content_array) = content {
            content_array
                .iter()
                .filter_map(|item| {
                    if item.get("type")?.as_str()? == "text" {
                        item.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            // Fallback: try to serialize the whole result
            serde_json::to_string_pretty(&result).unwrap_or_default()
        };

        // Check for error flag
        if result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false) {
            Err(anyhow!("Tool error: {}", text))
        } else {
            Ok(text)
        }
    }
}

/// Test connection to an MCP server
pub async fn test_mcp_server(url: &str) -> Result<Vec<String>> {
    let client = McpClient::new(url);
    let result = client.call("tools/list", None).await?;

    let tools_array = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow!("Invalid tools response"))?;

    let tool_names: Vec<String> = tools_array
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    Ok(tool_names)
}
