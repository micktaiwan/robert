use crate::llm::ToolDefinition;
use crate::mcp::McpManager;
use std::collections::HashMap;

/// Indicates where a tool should be routed for execution
#[derive(Debug, Clone)]
pub enum ToolSource {
    /// A local tool built into Robert
    Local,
    /// An MCP tool from an external server
    Mcp {
        server_id: String,
        original_name: String,
    },
}

/// Get merged tool definitions from local tools and all enabled MCP servers
/// Returns the tool definitions for Claude API and a routing table
pub async fn get_merged_tools(
    mcp_manager: Option<&McpManager>,
) -> (Vec<ToolDefinition>, HashMap<String, ToolSource>) {
    let mut tools = Vec::new();
    let mut routing = HashMap::new();

    // 1. Add local tools
    for tool in super::get_tool_definitions() {
        routing.insert(tool.name.clone(), ToolSource::Local);
        tools.push(tool);
    }

    // 2. Add MCP tools from all enabled servers
    if let Some(manager) = mcp_manager {
        let mcp_tools = manager.list_all_tools().await;

        for mcp_tool in mcp_tools {
            // Prefix tool name with server_id to avoid conflicts
            // e.g., "panorama_tool_tasksFilter"
            let prefixed_name = format!("{}_{}", mcp_tool.server_id, mcp_tool.name);

            routing.insert(
                prefixed_name.clone(),
                ToolSource::Mcp {
                    server_id: mcp_tool.server_id.clone(),
                    original_name: mcp_tool.name.clone(),
                },
            );

            tools.push(ToolDefinition {
                name: prefixed_name,
                description: mcp_tool.description,
                input_schema: mcp_tool.input_schema,
            });
        }
    }

    (tools, routing)
}
