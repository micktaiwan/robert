# Robert

A local voice assistant for macOS built with Tauri 2. Robert continuously listens to your microphone, transcribes speech locally using Whisper (with Metal GPU acceleration), and responds to voice commands using the Anthropic Claude API with tool use.

## Features

- **Local Speech Recognition**: Uses Whisper.cpp with Metal GPU acceleration for fast, private transcription
- **Wake Word Activation**: Responds to "Ok Robert" or "Hey Robert"
- **Claude-Powered Responses**: Leverages Anthropic's Claude API for intelligent voice command processing
- **Tool Use**: Extensible tool system for voice commands (recordings management, system controls, etc.)
- **MCP Integration**: Connect to external MCP servers to extend capabilities with additional tools
- **Always-On Overlay**: Floating window showing transcriptions and responses
- **Global Shortcuts**: Cmd+Shift+R for overlay, Cmd+Shift+E for recording

## Requirements

- macOS (Metal GPU required for Whisper acceleration)
- Node.js 18+
- Rust toolchain
- Anthropic API key

## Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/YOUR_USERNAME/robert.git
   cd robert
   ```

2. Install dependencies:
   ```bash
   npm install
   ```

3. Download the Whisper model:
   ```bash
   # For development:
   mkdir -p src-tauri/models
   curl -L -o src-tauri/models/ggml-small.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin

   # For production (after build):
   mkdir -p ~/Library/Application\ Support/com.robert.voiceassistant/models
   curl -L -o ~/Library/Application\ Support/com.robert.voiceassistant/models/ggml-small.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
   ```

4. Run in development mode:
   ```bash
   npm run tauri dev
   ```

5. Configure your Anthropic API key in Settings (accessible via tray icon)

## Build

```bash
npm run tauri build
```

## Architecture

### Backend (Rust)

- **Audio Capture**: Microphone input via cpal with voice activity detection
- **Transcription**: Whisper.cpp integration via whisper-rs
- **LLM Client**: Anthropic API with SSE streaming and agentic tool loop
- **Storage**: SQLite database for recordings and transcriptions

### Frontend (React/TypeScript)

- **Overlay Window**: Always-on-top floating display
- **Settings Window**: Configuration and recordings management

## Configuration

Settings are stored at `~/Library/Application Support/com.robert.Robert/settings.json`:

- `anthropic_api_key`: Your Anthropic API key
- `mic_device`: Selected microphone
- `speech_threshold`, `silence_duration_ms`: Voice activity detection tuning
- `wake_words`: Trigger phrases (default: "ok robert", "hey robert")

## MCP Servers

Robert can connect to MCP (Model Context Protocol) servers to extend its capabilities. Configure servers in Settings > MCP Servers tab with:

- **ID**: Unique identifier (used as tool prefix)
- **Name**: Display name
- **URL**: JSON-RPC endpoint

## License

MIT
