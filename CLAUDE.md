# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Robert is a local voice assistant for macOS built with Tauri 2. It continuously listens to the microphone, transcribes speech using Whisper (locally via Metal GPU), and responds to voice commands using the Anthropic Claude API with tool use.

## Build & Development Commands

```bash
# Development (starts Vite + Tauri together)
npm run tauri dev

# Production build
npm run tauri build

# Frontend only
npm run dev        # Vite dev server
npm run build      # TypeScript + Vite build
```

**Important**: Never run `./target/release/robert` directly - it requires the Vite dev server or built frontend assets.

## Architecture

### Backend (Rust - `src-tauri/`)

- **lib.rs**: Application entry point, Tauri setup, tray icon, global shortcuts (Cmd+Shift+R for overlay, Cmd+Shift+E for recording), audio processing loop
- **audio/capture.rs**: Microphone capture using cpal, voice activity detection, audio buffering
- **transcription/whisper.rs**: Whisper.cpp integration via whisper-rs (Metal GPU acceleration)
- **llm/anthropic.rs**: Anthropic API client with SSE streaming, implements agentic loop with tool use
- **tools/**: Tool definitions and executor for voice commands (recordings CRUD, quit, summarize)
- **storage/**: SQLite database for recordings and transcriptions
- **state.rs**: Application state (settings, active recording, conversation history)
- **handlers.rs**: Tauri IPC commands for frontend

### Frontend (React/TypeScript - `src/`)

- **main.tsx**: Hidden main window (app lifecycle only)
- **overlay.tsx**: Always-on-top overlay showing transcriptions and responses
- **settings.tsx**: Settings UI with tabs for configuration and recordings management

### Data Flow

1. AudioCapture buffers microphone samples until speech ends (silence detection)
2. Transcriber (Whisper) converts audio to text
3. If text contains wake word ("ok robert"), command is extracted and sent to AgenticClient
4. AgenticClient runs agentic loop: calls Claude API, executes tools, repeats until done
5. Responses stream to overlay via Tauri events

## Key Dependencies

- **whisper-rs**: Whisper.cpp bindings with Metal support for fast local transcription
- **cpal**: Cross-platform audio capture
- **reqwest**: HTTP client for Anthropic API (with streaming)
- **rusqlite**: SQLite for recordings persistence
- **tauri 2**: Desktop app framework with tray icon and global shortcuts

## Configuration

Settings stored at `~/Library/Application Support/com.robert.Robert/settings.json`:
- `anthropic_api_key`: Required for voice commands
- `mic_device`: Microphone selection
- `speech_threshold`, `silence_duration_ms`: Voice activity detection tuning
- `wake_words`: Trigger phrases (default: "ok robert", "hey robert")

Whisper model expected at `models/ggml-small.bin` (download from HuggingFace whisper.cpp repo).

## MCP Integration

Robert can connect to external MCP (Model Context Protocol) servers to extend its capabilities with additional tools.

### Architecture

- **mcp/manager.rs**: JSON-RPC 2.0 client for MCP servers (simple HTTP POST)
- **tools/provider.rs**: Merges local tools with MCP tools, adds server prefix (e.g., `panorama_tool_tasksFilter`)
- **tools/executor.rs**: Routes tool calls to local handlers or MCP servers based on `ToolSource`

### Configuration

MCP servers are configured in Settings > MCP Servers tab:
- **ID**: Unique identifier used as tool prefix (e.g., `panorama`)
- **Name**: Display name
- **URL**: JSON-RPC endpoint (e.g., `http://localhost:3000/mcp`)

### Tool Routing

When Claude calls a tool:
1. `ToolExecutor` looks up the tool name in the routing table
2. If `ToolSource::Local`, executes via local handler
3. If `ToolSource::Mcp { server_id, original_name }`, sends JSON-RPC request to the MCP server

## Known Issues

See `DIFFICULTIES.md` for documented issues:
- Whisper.cpp logs are suppressed via stderr redirect
- Tauri 2 transparent windows require `macOSPrivateApi` feature (not implemented)
- Local LLM attempts (Candle, llama-cpp-2) abandoned due to performance issues
