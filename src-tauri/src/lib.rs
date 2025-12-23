mod audio;
mod handlers;
mod llm;
mod mcp;
mod state;
mod storage;
mod tools;
mod transcription;

use audio::{AudioCapture, AudioEvent, VadConfig};
use llm::{AgenticClient, user_message};
use state::{AppState, CopilotUIState};
use std::sync::{Arc, Mutex};
use storage::{AudioSource, Database};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    webview::WebviewWindowBuilder,
    Emitter, Manager, WebviewUrl,
};
use tokio::sync::RwLock;
use mcp::McpManager;
use tools::{get_merged_tools, ToolExecutor};
use transcription::{Transcriber, StreamingTranscriber, StreamingConfig};

pub type DbState = Arc<Mutex<Database>>;
pub type CopilotState = Arc<std::sync::RwLock<CopilotUIState>>;

const WHISPER_MODEL: &str = "models/ggml-small.bin";

const WAKE_PATTERNS: &[&str] = &["ok robert", "okay robert", "hey robert", "robert,", "robert "];

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize database before Tauri
    let db: Option<DbState> = match Database::new() {
        Ok(db) => {
            println!("[Robert] Database initialized");
            Some(Arc::new(Mutex::new(db)))
        }
        Err(e) => {
            eprintln!("[Robert] Failed to initialize database: {}", e);
            None
        }
    };

    // Create copilot UI state (using std::sync::RwLock for sync access in callbacks)
    let copilot_state: CopilotState = Arc::new(std::sync::RwLock::new(CopilotUIState::new()));

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Arc::new(RwLock::new(AppState::load())))
        .manage(copilot_state.clone());

    // Only manage database if it was created successfully
    if let Some(db) = db.clone() {
        builder = builder.manage(db);
    }

    builder
        .setup(move |app| {
            create_overlay_window(app)?;
            create_settings_window(app)?;
            create_copilot_window(app)?;

            setup_tray(app)?;
            setup_global_shortcut(app, db.clone())?;

            // Start audio processing in background
            let app_handle = app.handle().clone();
            let state_clone = app.state::<Arc<RwLock<AppState>>>().inner().clone();
            let copilot_clone = copilot_state.clone();
            let db_clone = db.clone();
            std::thread::spawn(move || {
                if let Err(e) = audio_processing_loop(app_handle, state_clone, copilot_clone, db_clone) {
                    eprintln!("Audio processing error: {}", e);
                }
            });

            println!("[Robert] Loading...");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            handlers::get_settings,
            handlers::save_settings,
            handlers::get_models,
            handlers::list_audio_devices,
            handlers::start_recording,
            handlers::stop_recording,
            handlers::list_recordings,
            handlers::get_recording_transcriptions,
            handlers::rename_recording,
            handlers::delete_recording,
            handlers::get_recording_status,
            handlers::get_copilot_state,
            handlers::test_mcp_server,
        ])
        .run(tauri::generate_context!())
        .expect("error running Robert");
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Robert", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

    let icon_data: Vec<u8> = vec![0, 0, 0, 255].repeat(16 * 16);
    let icon = Image::new_owned(icon_data, 16, 16);

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn setup_global_shortcut(app: &tauri::App, db: Option<DbState>) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

    // Cmd+Shift+R: Toggle overlay
    let overlay_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyR);

    // Cmd+Shift+E: Toggle recording
    let recording_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyE);

    app.global_shortcut()
        .on_shortcut(overlay_shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(overlay) = app.get_webview_window("overlay") {
                    if overlay.is_visible().unwrap_or(false) {
                        let _ = overlay.hide();
                    } else {
                        let _ = overlay.show();
                    }
                }
            }
        })?;

    let state = app.state::<Arc<RwLock<AppState>>>().inner().clone();
    let app_handle = app.handle().clone();

    app.global_shortcut()
        .on_shortcut(recording_shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let state = state.clone();
                let app_handle = app_handle.clone();
                let db = db.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        toggle_recording(&app_handle, &state, db.as_ref()).await;
                    });
                });
            }
        })?;

    Ok(())
}

async fn toggle_recording(app: &tauri::AppHandle, state: &Arc<RwLock<AppState>>, db: Option<&DbState>) {
    let db = match db {
        Some(db) => db,
        None => {
            eprintln!("[{}] Database not available", timestamp());
            return;
        }
    };

    let mut state = state.write().await;

    if state.active_recording.is_some() {
        // Stop recording
        if let Some(active) = state.active_recording.take() {
            if let Ok(db) = db.lock() {
                let _ = db.end_recording(active.id);
                let _ = app.emit("recording-stopped", &active.name);
                println!("[{}] Recording stopped: {}", timestamp(), active.name);
            }
        }
    } else {
        // Start recording
        if let Ok(db) = db.lock() {
            let name = format!(
                "Recording {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M")
            );
            if let Ok(recording) = db.create_recording(&name) {
                state.active_recording = Some(state::ActiveRecording {
                    id: recording.id,
                    name: recording.name.clone(),
                });
                let _ = app.emit("recording-started", &recording.name);
                println!("[{}] Recording started: {}", timestamp(), recording.name);
            }
        }
    }
}

fn create_overlay_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let overlay = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("overlay.html".into()),
    )
    .title("")
    .inner_size(800.0, 60.0)
    .position(100.0, 50.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .transparent(true)
    .build()?;

    if let Some(monitor) = overlay.current_monitor()? {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let screen_width = size.width as f64 / scale;
        let overlay_width = screen_width * 0.8;
        let x = (screen_width - overlay_width) / 2.0;

        overlay.set_position(tauri::LogicalPosition::new(x, 50.0))?;
        overlay.set_size(tauri::LogicalSize::new(overlay_width, 60.0))?;
    }

    Ok(())
}

fn create_settings_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("settings.html".into()),
    )
    .title("Robert Settings")
    .inner_size(600.0, 550.0)
    .center()
    .resizable(true)
    .visible(false)
    .build()?;

    Ok(())
}

fn create_copilot_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let copilot = WebviewWindowBuilder::new(
        app,
        "copilot",
        WebviewUrl::App("copilot.html".into()),
    )
    .title("")
    .inner_size(450.0, 600.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;

    // Position in bottom-right corner
    if let Some(monitor) = copilot.current_monitor()? {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let screen_width = size.width as f64 / scale;
        let screen_height = size.height as f64 / scale;
        let window_width = 450.0;
        let window_height = 600.0;
        let margin = 20.0;

        let x = screen_width - window_width - margin;
        let y = screen_height - window_height - margin;

        copilot.set_position(tauri::LogicalPosition::new(x, y))?;
    }

    Ok(())
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = now.as_secs() % 86400;
    let hours = (secs / 3600) % 24;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    let millis = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, millis)
}

fn extract_command(text: &str) -> Option<String> {
    let text_lower = text.to_lowercase();

    for pattern in WAKE_PATTERNS {
        if let Some(pos) = text_lower.find(pattern) {
            let command_start = pos + pattern.len();
            let command_text = text[command_start..]
                .trim()
                .trim_start_matches(|c: char| c.is_ascii_punctuation())
                .to_string();

            if !command_text.is_empty() {
                return Some(command_text);
            }
        }
    }

    None
}

fn audio_processing_loop(
    app: tauri::AppHandle,
    state: Arc<RwLock<AppState>>,
    copilot_state: CopilotState,
    db: Option<DbState>,
) -> anyhow::Result<()> {
    let whisper_path = std::path::Path::new(WHISPER_MODEL);

    if !whisper_path.exists() {
        eprintln!("Whisper model not found at {}", WHISPER_MODEL);
        eprintln!("Download with: curl -L -o models/ggml-small.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin");
        return Ok(());
    }

    println!("[{}] Loading Whisper (streaming mode)...", timestamp());
    let _ = app.emit("loading", "Loading Whisper...");

    // Use streaming transcriber for real-time wake word detection
    let streaming_config = StreamingConfig::default();
    let mut streaming_transcriber = StreamingTranscriber::new(whisper_path, streaming_config)?;

    // Keep regular transcriber for final transcription (better accuracy)
    let mut final_transcriber = Transcriber::new(whisper_path)?;

    println!("[{}] Whisper ready (streaming)", timestamp());

    // Get settings for audio capture
    let (mic_device, vad_config) = {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let state = state.read().await;
            let vad = VadConfig {
                speech_threshold: state.settings.speech_threshold,
                silence_duration_ms: state.settings.silence_duration_ms,
            };
            (state.settings.mic_device.clone(), vad)
        })
    };

    println!("[{}] VAD settings: threshold={}, silence_ms={}",
        timestamp(), vad_config.speech_threshold, vad_config.silence_duration_ms);

    let capture = match &mic_device {
        Some(device_name) => {
            println!("[{}] Using microphone: {}", timestamp(), device_name);
            AudioCapture::new_with_device(device_name, vad_config)?
        }
        None => {
            println!("[{}] Using default microphone", timestamp());
            AudioCapture::new(vad_config)?
        }
    };

    if let Some(name) = capture.device_name() {
        println!("[{}] Audio device: {}", timestamp(), name);
    }

    // Use event receiver for streaming mode
    let event_receiver = capture.event_receiver();
    let _stream = capture.start()?;

    println!("[{}] Ready (streaming mode)", timestamp());
    let _ = app.emit("ready", ());

    let rt = tokio::runtime::Runtime::new()?;

    // Track if wake word was detected in current utterance
    let mut wake_word_detected = false;
    let mut overlay_shown = false;

    loop {
        match event_receiver.recv() {
            Ok(event) => {
                match event {
                    AudioEvent::StreamingChunk(samples) => {
                        // Push audio to streaming transcriber
                        streaming_transcriber.push_audio(&samples);

                        // Transcribe for real-time wake word detection
                        if let Ok(result) = streaming_transcriber.transcribe() {
                            let text = result.text.trim().to_string();

                            if !text.is_empty() && text != "." && text != "..." && text.len() > 1 {
                                // Check for wake word in streaming text
                                if !wake_word_detected && contains_wake_word(&text) {
                                    wake_word_detected = true;
                                    println!("[{}] Wake word detected (streaming): {}", timestamp(), text);

                                    // Show overlay IMMEDIATELY
                                    if !overlay_shown {
                                        overlay_shown = true;

                                        // Update copilot state
                                        {
                                            let mut copilot = copilot_state.write().unwrap();
                                            copilot.visible = true;
                                            copilot.state = "listening".to_string();
                                            copilot.response_text.clear();
                                            copilot.should_close = false;
                                        }

                                        // Show the window
                                        if let Some(copilot) = app.get_webview_window("copilot") {
                                            let _ = copilot.show();
                                            let _ = copilot.set_focus();
                                        }
                                    }
                                }

                                // Emit streaming transcription
                                let _ = app.emit("transcription", &text);
                            }
                        }
                    }

                    AudioEvent::SpeechEnded(samples) => {
                        // Final transcription with full audio (more accurate)
                        if let Ok(text) = final_transcriber.transcribe(&samples) {
                            let text = text.trim().to_string();

                            if !text.is_empty() && text != "." && text != "..." && text.len() > 1 {
                                println!("[{}] Final: {}", timestamp(), text);

                                // Process command if wake word was detected
                                let is_command = if wake_word_detected {
                                    if let Some(command_text) = extract_command(&text) {
                                        println!("[{}] Command: {}", timestamp(), command_text);

                                        let db_ref = db.clone();
                                        let copilot_clone = copilot_state.clone();
                                        rt.block_on(async {
                                            process_command(&app, &state, &copilot_clone, &command_text, db_ref.as_ref()).await;
                                        });
                                        true
                                    } else {
                                        // Wake word detected but no command extracted
                                        // This might happen if transcription changed
                                        false
                                    }
                                } else if let Some(command_text) = extract_command(&text) {
                                    // Fallback: wake word in final transcription but not in streaming
                                    println!("[{}] Command (late detection): {}", timestamp(), command_text);

                                    // Show overlay if not already shown
                                    {
                                        let mut copilot = copilot_state.write().unwrap();
                                        copilot.visible = true;
                                        copilot.state = "listening".to_string();
                                        copilot.response_text.clear();
                                        copilot.should_close = false;
                                    }

                                    if let Some(copilot) = app.get_webview_window("copilot") {
                                        let _ = copilot.show();
                                        let _ = copilot.set_focus();
                                    }

                                    let db_ref = db.clone();
                                    let copilot_clone = copilot_state.clone();
                                    rt.block_on(async {
                                        process_command(&app, &state, &copilot_clone, &command_text, db_ref.as_ref()).await;
                                    });
                                    true
                                } else {
                                    false
                                };

                                // Store transcription if recording is active
                                if let Some(ref db) = db {
                                    rt.block_on(async {
                                        let state = state.read().await;
                                        if let Some(active) = &state.active_recording {
                                            if let Ok(db) = db.lock() {
                                                let _ = db.add_transcription(
                                                    active.id,
                                                    &text,
                                                    AudioSource::Microphone,
                                                );
                                            }
                                        }
                                    });
                                }

                                // Emit final transcription
                                let _ = app.emit("transcription", &text);

                                if !is_command {
                                    println!("[{}] {}", timestamp(), text);
                                }
                            }
                        }

                        // Reset state for next utterance
                        streaming_transcriber.reset();
                        wake_word_detected = false;
                        overlay_shown = false;
                    }
                }
            }
            Err(_) => break,
        }
    }

    Ok(())
}

/// Check if text contains any wake word pattern
fn contains_wake_word(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    WAKE_PATTERNS.iter().any(|pattern| text_lower.contains(pattern))
}

const SYSTEM_PROMPT: &str = "You are Robert, a voice assistant that helps users manage their meeting recordings. \
You can start/stop recordings, list them, summarize them, get their content, rename them, and delete them. \
When the user confirms an action (like 'yes', 'go ahead', 'do it', 'tu peux y aller'), execute the action discussed. \
Always respond in the same language the user speaks.";

const MAX_HISTORY_MESSAGES: usize = 40;

async fn process_command(app: &tauri::AppHandle, state: &Arc<RwLock<AppState>>, copilot_state: &CopilotState, command_text: &str, db: Option<&DbState>) {
    let api_key = {
        let state = state.read().await;
        state.settings.anthropic_api_key.clone()
    };

    let api_key = match api_key {
        Some(key) if !key.is_empty() => {
            let trimmed = key.trim().to_string();
            println!("[{}] API key: {}...{} (len={})",
                timestamp(),
                &trimmed.chars().take(10).collect::<String>(),
                &trimmed.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>(),
                trimmed.len()
            );
            trimmed
        },
        _ => {
            println!("[{}] No Anthropic API key configured", timestamp());
            let _ = app.emit("command-response", "Please configure your Anthropic API key in settings");
            return;
        }
    };

    // Get current history and add user message
    let mut messages = {
        let mut state_guard = state.write().await;

        // Add user message
        state_guard.conversation_history.push(user_message(command_text));

        // Trim history if too long
        if state_guard.conversation_history.len() > MAX_HISTORY_MESSAGES {
            let drain_count = state_guard.conversation_history.len() - MAX_HISTORY_MESSAGES;
            state_guard.conversation_history.drain(0..drain_count);
        }

        state_guard.conversation_history.clone()
    };

    println!("[{}] Starting agentic loop with {} messages in history", timestamp(), messages.len());

    // Update copilot state to thinking
    {
        let mut copilot = copilot_state.write().unwrap();
        copilot.state = "thinking".to_string();
    }

    let client = AgenticClient::new(&api_key);

    // Get MCP server configs and create manager
    let mcp_servers = {
        let state = state.read().await;
        state.settings.mcp_servers.clone()
    };

    let mcp_manager = if mcp_servers.is_empty() {
        None
    } else {
        Some(Arc::new(McpManager::new(mcp_servers)))
    };

    // Get merged tools (local + MCP) and routing table
    let (tools, routing) = get_merged_tools(mcp_manager.as_ref().map(|m| m.as_ref())).await;

    // Create tool executor with MCP support
    let executor = ToolExecutor::new(
        app.clone(),
        state.clone(),
        db.cloned(),
        mcp_manager,
        routing,
    );
    let copilot_for_callback = copilot_state.clone();
    let has_started_responding = std::sync::atomic::AtomicBool::new(false);

    // Run agentic loop
    let result = client.run_agentic_loop(
        &mut messages,
        &tools,
        SYSTEM_PROMPT,
        // Tool execution callback
        |tool_name: &str, tool_input: serde_json::Value| {
            let executor = executor.clone();
            let name = tool_name.to_string();
            Box::pin(async move {
                executor.execute(&name, tool_input).await
            })
        },
        // Text streaming callback
        |text: &str| {
            // Update to responding state on first chunk
            if !has_started_responding.swap(true, std::sync::atomic::Ordering::SeqCst) {
                if let Ok(mut copilot) = copilot_for_callback.write() {
                    copilot.state = "responding".to_string();
                }
            }
            // Accumulate text
            if let Ok(mut copilot) = copilot_for_callback.write() {
                copilot.response_text.push_str(text);
            }
        },
    ).await;

    match result {
        Ok(final_text) => {
            if !final_text.is_empty() {
                // Truncate log to avoid verbose output
                let preview = if final_text.len() > 100 {
                    format!("{}...", &final_text[..100])
                } else {
                    final_text.clone()
                };
                println!("[{}] Response: {}", timestamp(), preview);
            }
            // Signal copilot window to start auto-close countdown
            {
                let mut copilot = copilot_state.write().unwrap();
                copilot.should_close = true;
            }
        }
        Err(e) => {
            eprintln!("[{}] Agentic loop error: {}", timestamp(), e);
            // Set error message and signal close
            {
                let mut copilot = copilot_state.write().unwrap();
                copilot.response_text = "Sorry, I couldn't process that command".to_string();
                copilot.state = "responding".to_string();
                copilot.should_close = true;
            }
        }
    }

    // Save updated history
    {
        let mut state_guard = state.write().await;
        state_guard.conversation_history = messages;
    }
}
