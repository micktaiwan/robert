# Robert - Swift Rewrite Specification

> Document de spécification pour la réécriture native macOS en Swift/SwiftUI

---

## Résumé Exécutif (10 lignes)

Robert est un assistant vocal local pour macOS qui :
1. **Capture audio en continu** via le microphone avec Voice Activity Detection (VAD)
2. **Transcrit localement** via Whisper.cpp (GPU Metal) en mode streaming et batch
3. **Détecte un wake word** ("OK Robert") en temps réel grâce à une fenêtre glissante
4. **Extrait la commande** vocale après le wake word
5. **Envoie au LLM** (Claude API) avec streaming SSE dans une boucle agentique
6. **Exécute des outils** locaux (gestion d'enregistrements) et distants (MCP JSON-RPC)
7. **Affiche les réponses** en temps réel dans une fenêtre overlay animée
8. **Persiste les données** (settings JSON, recordings SQLite)
9. **Fournit des raccourcis globaux** (Cmd+Shift+R overlay, Cmd+Shift+E recording)
10. **Gère l'historique conversationnel** (40 messages max) pour des interactions multi-turn

---

## A) Feature Backlog (P0/P1/P2)

### P0 - MVP (End-to-End Minimal)

| ID | Feature | Description | Fichiers sources |
|----|---------|-------------|------------------|
| P0.1 | Audio Capture + VAD | Capture micro continue, détection parole/silence par RMS | `src-tauri/src/audio/capture.rs:128-237` |
| P0.2 | Whisper Transcription | Transcription batch (finale) via whisper.cpp Metal | `src-tauri/src/transcription/whisper.rs` |
| P0.3 | Wake Word Detection | Détection "OK Robert" dans le texte transcrit | `src-tauri/src/lib.rs:335-353, 573-577` |
| P0.4 | Command Extraction | Extraction du texte après wake word | `src-tauri/src/lib.rs:335-353` |
| P0.5 | Claude API Client | Appel Claude avec streaming SSE | `src-tauri/src/llm/anthropic.rs:121-337` |
| P0.6 | Basic Tool System | Au moins 1 outil local (list_recordings) | `src-tauri/src/tools/definitions.rs` |
| P0.7 | Copilot Window | Fenêtre overlay pour afficher réponses | `src/copilot.tsx`, `src-tauri/src/lib.rs:290-322` |
| P0.8 | Settings Storage | Persistance settings JSON | `src-tauri/src/state.rs:88-111` |
| P0.9 | Tray Icon | Menu bar avec Settings/Quit | `src-tauri/src/lib.rs:130-155` |

### P1 - Important mais Non Bloquant

| ID | Feature | Description | Fichiers sources |
|----|---------|-------------|------------------|
| P1.1 | Streaming Transcription | Transcription temps réel pour wake word rapide | `src-tauri/src/transcription/streaming.rs` |
| P1.2 | Recording System | CRUD complet recordings + transcriptions SQLite | `src-tauri/src/storage/database.rs` |
| P1.3 | All Local Tools | 8 outils: quit, list, summarize, start/stop, get_content, rename, delete | `src-tauri/src/tools/executor.rs` |
| P1.4 | Agentic Loop | Boucle multi-itération avec tool use parallèle | `src-tauri/src/llm/anthropic.rs:290-327` |
| P1.5 | Conversation History | Historique 40 messages pour multi-turn | `src-tauri/src/lib.rs:584, 611-624` |
| P1.6 | Overlay Window | Transcription en direct + indicateur recording | `src/overlay.tsx` |
| P1.7 | Settings UI | Interface Settings avec tabs | `src/settings.tsx` |
| P1.8 | Global Shortcuts | Cmd+Shift+R (overlay), Cmd+Shift+E (recording) | `src-tauri/src/lib.rs:157-198` |
| P1.9 | Window Auto-hide | Fade out après inactivité (5s) | `src/overlay.tsx:25-46`, `src/copilot.tsx:180-206` |
| P1.10 | Microphone Selection | Choix du device audio | `src-tauri/src/handlers.rs` (list_audio_devices) |

### P2 - Nice-to-Have

| ID | Feature | Description | Fichiers sources |
|----|---------|-------------|------------------|
| P2.1 | MCP Integration | Support serveurs MCP externes via JSON-RPC | `src-tauri/src/mcp/manager.rs` |
| P2.2 | Tool Routing | Merge local+MCP tools avec prefixage | `src-tauri/src/tools/provider.rs` |
| P2.3 | Native Mouse Tracking | Polling NSEvent pour hover overlay | `src-tauri/src/macos_tracking.rs` |
| P2.4 | Wave Animation | Animation cercles pulsants dans copilot | `src/copilot.tsx:27-92` |
| P2.5 | Markdown Rendering | Rendu markdown dans réponses | `src/copilot.tsx:329-335` |
| P2.6 | System Audio Device | Option BlackHole (non implémenté) | `src-tauri/src/state.rs:67` |
| P2.7 | Whisper Model Selection | Choix du modèle Whisper | `src/settings.tsx:354-370` |
| P2.8 | Local Agreement | Stabilisation texte streaming | `src-tauri/src/transcription/streaming.rs:172-198` |
| P2.9 | Window Transparency | Alpha variable sur copilot | `src-tauri/src/handlers.rs:210-237` |
| P2.10 | Escape Key Close | Fermer copilot avec Escape | `src/copilot.tsx:234-251` |

---

## B) Functional Specification

### B.1 Audio (Capture & VAD)

**Responsabilité**: Capture micro continue avec détection voix/silence

**Fichiers**: `src-tauri/src/audio/capture.rs`

#### Paramètres VAD (configurables)

| Paramètre | Valeur par défaut | Description |
|-----------|-------------------|-------------|
| `speech_threshold` | 0.006 | Seuil RMS pour détecter la parole |
| `silence_duration_ms` | 1000 | Silence requis pour fin d'utterance |
| `MIN_SPEECH_DURATION_MS` | 400 | Durée min pour traiter |
| `MAX_SPEECH_DURATION_MS` | 10000 | Durée max avant traitement forcé |
| `STREAMING_CHUNK_MS` | 600 | Intervalle envoi chunks streaming |
| `TARGET_SAMPLE_RATE` | 16000 | Whisper attend 16kHz |

#### Pipeline Audio

```
Microphone → cpal Stream
    ↓
Multi-channel → Mono (moyenne)
    ↓
Calcul RMS amplitude
    ↓
[Si RMS > threshold] → speech_started = true
    ↓
[Tous les 600ms si speech] → StreamingChunk(samples)
    ↓
[Silence > 1000ms OU durée > 10s] → SpeechEnded(samples)
    ↓
Reset buffer
```

#### Resampling

- Entrée: sample rate natif (44.1kHz, 48kHz selon device)
- Sortie: 16kHz (requis par Whisper)
- Méthode: Interpolation linéaire (`capture.rs:271-286`)

#### Events émis

```rust
enum AudioEvent {
    StreamingChunk(Vec<f32>),  // Pendant speech, chaque 600ms
    SpeechEnded(Vec<f32>),     // Fin speech (silence ou timeout)
}
```

---

### B.2 STT (Speech-to-Text)

**Responsabilité**: Transcription locale via Whisper.cpp

**Fichiers**:
- `src-tauri/src/transcription/whisper.rs` (batch)
- `src-tauri/src/transcription/streaming.rs` (streaming)

#### Dual Transcription Strategy

| Mode | Utilisation | Latence | Précision |
|------|-------------|---------|-----------|
| Streaming | Wake word detection | ~600ms | Moyenne |
| Batch/Final | Commande finale | ~1-2s | Haute |

#### Streaming Transcriber

- **Fenêtre glissante**: 5 secondes d'audio (`length_ms: 5000`)
- **Buffer max**: 15 secondes
- **Local Agreement**: Stabilisation texte par comparaison consecutive

```rust
struct StreamingTranscriber {
    audio_buffer: VecDeque<f32>,      // Ring buffer
    max_buffer_samples: usize,         // 16000 * 15 = 240000
    confirmed_text: String,            // Texte confirmé
    previous_text: String,             // Pour comparaison
    agreement_count: usize,            // Compteur stabilité
}
```

#### Batch Transcriber

- Modèle: `ggml-small.bin` (par défaut)
- Chemins: `models/ggml-small.bin` (dev) ou `~/Library/Application Support/com.robert.Robert/models/` (prod)
- Paramètres: Greedy sampling, auto-detect language, no timestamps

#### Suppression logs Whisper

```rust
// Redirect stderr to /dev/null during whisper operations
fn suppress_stderr<F, T>(f: F) -> T { ... }
```

---

### B.3 Wake Word Detection

**Responsabilité**: Détecter "OK Robert" et variantes

**Fichier**: `src-tauri/src/lib.rs:36, 573-577`

#### Patterns reconnus

```rust
const WAKE_PATTERNS: &[&str] = &[
    "ok robert",
    "okay robert",
    "hey robert",
    "robert,",
    "robert "
];
```

#### Algorithme

```rust
fn contains_wake_word(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    WAKE_PATTERNS.iter().any(|pattern| text_lower.contains(pattern))
}

fn extract_command(text: &str) -> Option<String> {
    // Trouve le pattern, extrait tout après, trim punctuation
    for pattern in WAKE_PATTERNS {
        if let Some(pos) = text_lower.find(pattern) {
            return Some(text[pos + pattern.len()..].trim());
        }
    }
    None
}
```

#### Comportement

1. **Streaming check**: Vérifie wake word toutes les 600ms
2. **Si détecté**: Affiche fenêtre copilot immédiatement
3. **Final check**: Re-vérifie dans transcription finale (fallback)
4. **Reset**: Après SpeechEnded, reset état wake_word_detected

---

### B.4 LLM (Anthropic Claude API)

**Responsabilité**: Appels Claude API avec streaming et tool use

**Fichier**: `src-tauri/src/llm/anthropic.rs`

#### Configuration

| Paramètre | Valeur |
|-----------|--------|
| `ANTHROPIC_API_URL` | `https://api.anthropic.com/v1/messages` |
| `MODEL` | `claude-sonnet-4-20250514` |
| `MAX_TOKENS` | 4096 |
| `MAX_ITERATIONS` | 30 |

#### System Prompt

```
You are Robert, a voice assistant that helps users manage their meeting recordings.
You can start/stop recordings, list them, summarize them, get their content, rename them, and delete them.
When the user confirms an action (like 'yes', 'go ahead', 'do it', 'tu peux y aller'), execute the action discussed.
Always respond in the same language the user speaks.
```

#### Agentic Loop

```
1. Prépare request (messages, tools, system)
2. POST streaming → parse SSE events
3. Accumule text_content et tool_uses
4. Si stop_reason == "end_turn" OU pas de tool_use → return
5. Exécute tous les tools EN PARALLÈLE
6. Ajoute tool_results comme user message
7. Goto 1 (max 30 itérations)
```

#### SSE Event Types

| Event | Action |
|-------|--------|
| `content_block_start` (tool_use) | Store tool id/name |
| `content_block_delta` (text_delta) | Stream to callback |
| `content_block_delta` (input_json_delta) | Accumulate JSON |
| `content_block_stop` | Parse tool input JSON |
| `message_delta` | Get stop_reason |

#### Message Format

```rust
struct Message {
    role: String,           // "user" | "assistant"
    content: Vec<ContentBlock>,
}

enum ContentBlock {
    Text { text: String },
    ToolUse { id, name, input: Value },
    ToolResult { tool_use_id, content: String },
}
```

---

### B.5 Tools / MCP

**Responsabilité**: Définition et exécution d'outils

**Fichiers**:
- `src-tauri/src/tools/definitions.rs`
- `src-tauri/src/tools/executor.rs`
- `src-tauri/src/tools/provider.rs`
- `src-tauri/src/mcp/manager.rs`

#### Local Tools (8)

| Tool | Description | Paramètres |
|------|-------------|------------|
| `quit` | Ferme l'application | - |
| `list_recordings` | Liste tous les enregistrements | - |
| `summarize_recording` | Génère un résumé LLM | recording_name OR recording_index |
| `start_recording` | Démarre un enregistrement | name (optional) |
| `stop_recording` | Arrête l'enregistrement actif | - |
| `get_recording_content` | Récupère transcription complète | recording_name OR recording_index |
| `rename_recording` | Renomme un enregistrement | recording_name OR recording_index + new_name |
| `delete_recording` | Supprime un enregistrement | recording_name OR recording_index |

#### Recording Index Resolution

- `1` = premier (plus récent)
- `2` = deuxième
- `-1` = dernier (plus ancien)
- `-2` = avant-dernier

#### MCP Integration

**Protocole**: JSON-RPC 2.0 sur HTTP POST

```rust
struct McpServerConfig {
    id: String,      // Préfixe pour tools (ex: "panorama")
    name: String,    // Nom affichage
    url: String,     // Endpoint JSON-RPC
    enabled: bool,
}
```

**Tool Routing**:
1. Local tools → `ToolSource::Local`
2. MCP tools → `ToolSource::Mcp { server_id, original_name }`
3. Nom prefixé: `{server_id}_{original_name}` (ex: `panorama_tasksFilter`)

**MCP JSON-RPC Methods**:
- `tools/list` → Liste des outils disponibles
- `tools/call` → Exécution d'un outil

---

### B.6 UI Overlay (Copilot Window)

**Responsabilité**: Affichage réponses avec animations

**Fichiers**: `src/copilot.tsx`, `src-tauri/src/lib.rs:290-322`

#### Spécifications Fenêtre

| Propriété | Valeur |
|-----------|--------|
| Dimensions | 450x600 px |
| Position | Bottom-right (20px margin) |
| Decorations | Non |
| Always on top | Oui |
| Skip taskbar | Oui |
| Initial visible | Non |

#### États

```typescript
type CopilotStateType = "idle" | "listening" | "thinking" | "responding";
```

| État | Couleur animation | Description |
|------|-------------------|-------------|
| idle | - | Fenêtre cachée |
| listening | Bleu (#007aff) | Écoute active |
| thinking | Orange (#ff9500) | Claude traite (rotate) |
| responding | Vert (#34c759) | Réponse en cours |

#### Wave Animation

- 5 cercles concentriques + point central
- Animation: `pulse-wave` (scale 1→1.15) ou `pulse-rotate` (thinking)
- Durée: 2s (wave), 1.5s (rotate)

#### Text Animation

- 5 caractères par frame pour vitesse
- Curseur clignotant `|`
- Auto-scroll vers le bas

#### Auto-close Behavior

1. Response complete → `should_close = true`
2. Attendre 4.5s
3. Fade opacity 1→0 sur 500ms (steps de 0.05)
4. Hide window + reset state

---

### B.7 UI Overlay (Transcription Window)

**Responsabilité**: Affichage transcription temps réel

**Fichiers**: `src/overlay.tsx`, `src-tauri/src/lib.rs:239-272`

#### Spécifications Fenêtre

| Propriété | Valeur |
|-----------|--------|
| Dimensions | 80% screen width x 60px |
| Position | Top-center (y=50px) |
| Background | rgba(20, 20, 25, 0.85) |
| Border radius | 12px |
| Transparent | Oui |
| Always on top | Oui |

#### Affichages

| Condition | Couleur | Contenu |
|-----------|---------|---------|
| Loading | Gris (#888) | "Loading..." |
| Error | Rouge (#ff3b30) | Message erreur |
| Streaming | Orange (#ff9500) | Transcription en cours |
| Final | Blanc | Transcription finale (3s) |
| Response | Vert (#34c759) | Réponse commande (5s) |
| Idle | Gris (#666) | "Say 'OK Robert'..." |

#### Recording Indicator

- Badge rouge "REC" avec point pulsant
- Affiché quand `isRecording = true`

#### Auto-hide

- Timeout: 5s sans activité
- Cancel si mouse hover (via native tracking)
- Opacity: 1 → 0.01 (transition 0.5s)

---

### B.8 Storage

**Responsabilité**: Persistance données

**Fichiers**:
- `src-tauri/src/storage/database.rs`
- `src-tauri/src/state.rs`

#### SQLite Schema

```sql
CREATE TABLE recordings (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    ended_at TEXT,
    is_active INTEGER DEFAULT 1
);

CREATE TABLE transcriptions (
    id TEXT PRIMARY KEY,
    recording_id TEXT NOT NULL,
    text TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    source TEXT NOT NULL,  -- "microphone" | "system"
    FOREIGN KEY (recording_id) REFERENCES recordings(id) ON DELETE CASCADE
);

CREATE INDEX idx_transcriptions_recording ON transcriptions(recording_id);
```

#### Chemins

| Type | Chemin |
|------|--------|
| Database | `~/Library/Application Support/com.robert.Robert/robert.db` |
| Settings | `~/Library/Application Support/com.robert.Robert/settings.json` |
| Models (dev) | `./models/` |
| Models (prod) | `~/Library/Application Support/com.robert.Robert/models/` |

#### Settings Structure

```json
{
    "speech_threshold": 0.006,
    "silence_duration_ms": 1000,
    "wake_words": ["ok robert", "hey robert"],
    "whisper_model": "ggml-small.bin",
    "mic_device": null,
    "system_audio_device": null,
    "anthropic_api_key": null,
    "mcp_servers": []
}
```

---

### B.9 Settings UI

**Responsabilité**: Configuration utilisateur

**Fichier**: `src/settings.tsx`

#### Tabs

1. **Settings**: API key, devices, VAD params, wake words, Whisper model
2. **Recordings**: Liste, rename, delete
3. **MCP Servers**: Add, remove, enable/disable, test connection

#### Contrôles

| Setting | Type | Range/Values |
|---------|------|--------------|
| Speech threshold | Slider | 0.001 - 0.02 (step 0.001) |
| Silence duration | Number | 500 - 3000 ms |
| Wake words | Text | Comma-separated |
| API key | Password | sk-ant-... |
| Mic device | Select | List from cpal |

---

### B.10 Telemetry / Logs

**Responsabilité**: Debug et observabilité

#### Format Log

```
[HH:MM:SS.mmm] Message
```

#### Logs émis

| Event | Message |
|-------|---------|
| Startup | `[Robert] Loading...` |
| Whisper loaded | `[...] Whisper ready (streaming)` |
| VAD config | `[VAD] Using speech_threshold=..., silence_duration_ms=...` |
| Wake word | `[...] Wake word detected (streaming): ...` |
| Command | `[...] Command: ...` |
| Tool start | `[Tool Start] tool_name` |
| Tool OK | `[Tool OK] tool_name: preview...` |
| Tool Error | `[Tool Error] tool_name: error` |
| Response | `[...] Response: preview...` |

---

### B.11 Packaging / Install

#### Entitlements (macOS)

- `com.apple.security.device.audio-input` (microphone)
- Network client capability (pour Claude API)

#### Build

- **Dev**: `npm run tauri dev` (Vite + Tauri)
- **Prod**: `npm run tauri build` → `.app` + `.dmg`

#### Distribution

- Targets: `app`, `dmg`
- macOS minimum: 11.0

---

## C) Acceptance Tests (Given/When/Then)

### C.1 - Audio Capture

```gherkin
Scenario: AC-1.1 Basic audio capture starts
  Given Robert is launched
  And microphone permission is granted
  When Whisper model finishes loading
  Then audio capture should start
  And event "ready" should be emitted

Scenario: AC-1.2 Voice activity detection
  Given audio capture is running
  When user speaks with RMS > 0.006
  Then speech_started flag should be true
  And audio should be buffered

Scenario: AC-1.3 Speech end detection
  Given user has been speaking
  When silence (RMS < 0.006) persists for 1000ms
  And speech duration > 400ms
  Then SpeechEnded event should be emitted
  And buffer should reset

Scenario: AC-1.4 Max duration cutoff
  Given user is speaking continuously
  When speech duration reaches 10000ms
  Then SpeechEnded event should be emitted
  And new utterance capture should begin
```

### C.2 - Transcription

```gherkin
Scenario: AC-2.1 Streaming transcription
  Given audio capture is running
  When StreamingChunk event is received
  Then streaming transcriber should process chunk
  And partial text should be available

Scenario: AC-2.2 Final transcription
  Given SpeechEnded event with audio samples
  When batch transcriber processes audio
  Then accurate transcription should be returned
  And text should be emitted to overlay

Scenario: AC-2.3 Whisper model not found
  Given models/ggml-small.bin does not exist
  When Robert tries to initialize Whisper
  Then error event should be emitted
  And error message should include download instructions
```

### C.3 - Wake Word Detection

```gherkin
Scenario: AC-3.1 Wake word triggers copilot
  Given audio is being transcribed
  When transcription contains "ok robert"
  Then copilot window should show
  And copilot state should be "listening"

Scenario: AC-3.2 Wake word variants
  Given audio is being transcribed
  When transcription contains "hey robert" or "okay robert"
  Then wake word should be detected

Scenario: AC-3.3 Command extraction
  Given wake word "ok robert" detected
  And full transcription is "ok robert what time is it"
  When command is extracted
  Then command should be "what time is it"

Scenario: AC-3.4 No command after wake word
  Given transcription is just "ok robert"
  When extract_command is called
  Then None should be returned
  And no agentic loop should start
```

### C.4 - LLM Integration

```gherkin
Scenario: AC-4.1 Missing API key
  Given anthropic_api_key is null in settings
  When command is processed
  Then error message should show "Please configure your Anthropic API key"

Scenario: AC-4.2 Successful text response
  Given valid API key is configured
  And command is "list my recordings"
  When agentic loop completes
  Then response text should be displayed
  And copilot state should be "responding"

Scenario: AC-4.3 Tool use response
  Given command requires tool (e.g., "list recordings")
  When Claude responds with tool_use
  Then tool should be executed
  And tool result should be sent back to Claude

Scenario: AC-4.4 Parallel tool execution
  Given Claude responds with multiple tool_uses
  When tools are executed
  Then all tools should run in parallel (join_all)
  And all results should be collected

Scenario: AC-4.5 API error handling
  Given API returns non-200 status
  When agentic loop processes response
  Then error should be logged
  And copilot should show error message
```

### C.5 - Tool Execution

```gherkin
Scenario: AC-5.1 List recordings
  Given 3 recordings exist in database
  When list_recordings tool is called
  Then response should contain all 3 recording names with dates

Scenario: AC-5.2 Start recording
  Given no active recording
  When start_recording tool is called
  Then new recording should be created
  And recording-started event should be emitted
  And state.active_recording should be set

Scenario: AC-5.3 Start recording while active
  Given an active recording exists
  When start_recording tool is called
  Then error "A recording is already in progress" should be returned

Scenario: AC-5.4 Stop recording
  Given an active recording exists
  When stop_recording tool is called
  Then recording should be marked ended
  And recording-stopped event should be emitted

Scenario: AC-5.5 Delete active recording
  Given recording "Meeting" is active
  When delete_recording tool is called for "Meeting"
  Then error should indicate cannot delete active recording

Scenario: AC-5.6 Recording index resolution
  Given 3 recordings exist
  When recording_index = 1 is used
  Then first (most recent) recording should be selected

Scenario: AC-5.7 Negative index resolution
  Given 3 recordings exist
  When recording_index = -1 is used
  Then last (oldest) recording should be selected

Scenario: AC-5.8 Quit tool
  Given application is running
  When quit tool is executed
  Then app.exit(0) should be called
```

### C.6 - MCP Integration

```gherkin
Scenario: AC-6.1 MCP server connection test
  Given MCP server is configured with valid URL
  When test_mcp_server is called
  Then list of available tools should be returned

Scenario: AC-6.2 MCP tool execution
  Given MCP server "panorama" has tool "tasksFilter"
  When tool "panorama_tasksFilter" is called
  Then request should be routed to panorama server
  And original_name "tasksFilter" should be used

Scenario: AC-6.3 MCP server unreachable
  Given MCP server URL is invalid
  When list_all_tools is called
  Then warning should be logged
  And local tools should still be available
```

### C.7 - UI Windows

```gherkin
Scenario: AC-7.1 Overlay auto-hide
  Given overlay is visible
  And no activity for 5 seconds
  When fade timeout expires
  And mouse is not over overlay
  Then overlay opacity should fade to 0.01

Scenario: AC-7.2 Overlay mouse hover
  Given overlay is visible
  And fade timeout is counting
  When mouse enters overlay
  Then fade should be cancelled
  And overlay should stay visible

Scenario: AC-7.3 Copilot auto-close
  Given copilot shows complete response
  When 4.5 seconds pass
  Then opacity should fade 1→0 over 500ms
  Then window should hide
  And state should reset to idle

Scenario: AC-7.4 Copilot escape key
  Given copilot window is visible
  When user presses Escape
  Then window should hide immediately
  And state should reset
```

### C.8 - Global Shortcuts

```gherkin
Scenario: AC-8.1 Toggle overlay
  Given overlay is visible
  When Cmd+Shift+R is pressed
  Then overlay should hide

Scenario: AC-8.2 Toggle recording
  Given no active recording
  When Cmd+Shift+E is pressed
  Then recording should start
  And "REC" indicator should show

Scenario: AC-8.3 Toggle recording off
  Given active recording exists
  When Cmd+Shift+E is pressed
  Then recording should stop
  And "REC" indicator should hide
```

### C.9 - Settings

```gherkin
Scenario: AC-9.1 Save settings
  Given user changes speech_threshold to 0.008
  When Save button is clicked
  Then settings.json should be updated
  And "Saved!" confirmation should show

Scenario: AC-9.2 Mic device selection
  Given multiple microphones available
  When user selects "MacBook Pro Microphone"
  And saves settings
  Then next audio capture should use selected device
```

### C.10 - Edge Cases

```gherkin
Scenario: AC-10.1 Database initialization failure
  Given database path is not writable
  When Robert starts
  Then warning should be logged
  And app should continue without database
  And recording features should be disabled

Scenario: AC-10.2 Conversation history limit
  Given 45 messages in conversation history
  When new message is added
  Then oldest 5 messages should be trimmed
  And history should have 40 messages

Scenario: AC-10.3 Empty transcription
  Given very short audio captured
  When transcription returns empty or "."
  Then no event should be emitted
  And no command should be processed
```

---

## D) Swift Rewrite Plan (High-Level)

### D.1 Architecture Cible

```
┌─────────────────────────────────────────────────────────┐
│                    RobertApp (Main)                     │
│  - NSApplication lifecycle                              │
│  - Menu bar (NSStatusItem)                              │
│  - Global shortcuts (MASShortcut ou Carbon)             │
└─────────────────────────────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
┌───────────────┐  ┌───────────────┐  ┌───────────────┐
│  CopilotView  │  │  OverlayView  │  │  SettingsView │
│  (SwiftUI)    │  │  (SwiftUI)    │  │  (SwiftUI)    │
│  NSPanel      │  │  NSPanel      │  │  NSWindow     │
│  always-on-top│  │  always-on-top│  │  standard     │
└───────────────┘  └───────────────┘  └───────────────┘
        │                  │                  │
        └──────────────────┼──────────────────┘
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   RobertEngine                          │
│  @MainActor ObservableObject                            │
│  - audioCapture: AudioCaptureService                    │
│  - transcriber: WhisperService                          │
│  - llmClient: AnthropicClient                           │
│  - toolExecutor: ToolExecutor                           │
│  - storage: StorageService                              │
│  - settings: SettingsService                            │
└─────────────────────────────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
┌───────────────┐  ┌───────────────┐  ┌───────────────┐
│AudioCapture   │  │WhisperService │  │AnthropicClient│
│Service        │  │               │  │               │
│AVAudioEngine  │  │whisper.cpp    │  │URLSession     │
│VAD logic      │  │Metal GPU      │  │SSE streaming  │
└───────────────┘  └───────────────┘  └───────────────┘
```

### D.2 Stack Technologique

| Composant | Technologie Swift |
|-----------|-------------------|
| UI Windows | SwiftUI + NSPanel (floating) |
| Audio Capture | AVAudioEngine + AVAudioInputNode |
| Whisper | whisper.cpp via Swift Package (C interop) |
| HTTP/SSE | URLSession + async/await |
| Storage | SwiftData ou GRDB.swift |
| Settings | @AppStorage + UserDefaults |
| Global Shortcuts | Carbon API (RegisterEventHotKey) ou MASShortcut |
| Menu Bar | NSStatusItem + NSMenu |
| Concurrency | Swift Concurrency (async/await, actors) |

### D.3 Vertical Slices (Milestones)

#### Milestone 1: Foundation + Audio
**Objectif**: Capture audio avec VAD fonctionnel

**Composants**:
- Structure projet Xcode
- AudioCaptureService (AVAudioEngine, mono 16kHz)
- VAD basic (RMS threshold)
- AudioEvent (StreamingChunk, SpeechEnded)
- Unit tests VAD

**Critères de réussite**:
- [ ] App démarre sans crash
- [ ] Audio capture fonctionne avec microphone permission
- [ ] VAD détecte parole/silence
- [ ] Events envoyés via AsyncSequence ou Combine

---

#### Milestone 2: Whisper Integration
**Objectif**: Transcription locale fonctionnelle

**Composants**:
- WhisperService (wrapper C)
- StreamingTranscriber (fenêtre glissante)
- BatchTranscriber (transcription finale)
- Model loading + Metal GPU
- Logs suppression

**Critères de réussite**:
- [ ] Modèle ggml-small.bin se charge
- [ ] Streaming transcription toutes les 600ms
- [ ] Batch transcription haute précision
- [ ] GPU Metal utilisé (vérifier logs)

---

#### Milestone 3: Wake Word + Command
**Objectif**: Détection wake word et extraction commande

**Composants**:
- WakeWordDetector (patterns matching)
- CommandExtractor
- État machine basic (idle → listening)
- Debug overlay (texte brut)

**Critères de réussite**:
- [ ] "OK Robert" détecté en streaming
- [ ] Commande extraite correctement
- [ ] État passe à "listening" sur wake word

---

#### Milestone 4: Claude API + Agentic Loop
**Objectif**: Communication LLM avec tool use

**Composants**:
- AnthropicClient (URLSession streaming)
- SSE parser
- Agentic loop (async)
- ToolDefinition protocol
- Basic tools (list_recordings mock)

**Critères de réussite**:
- [ ] API call réussit avec streaming
- [ ] Text chunks reçus en temps réel
- [ ] Tool use parsed correctement
- [ ] Loop s'arrête sur "end_turn"

---

#### Milestone 5: UI Windows
**Objectif**: Windows overlay fonctionnelles

**Composants**:
- CopilotWindow (NSPanel, always-on-top)
- OverlayWindow (NSPanel, transparent)
- Wave animation (SwiftUI)
- Auto-fade logic
- Markdown rendering (AttributedString)

**Critères de réussite**:
- [ ] Fenêtres s'affichent correctement
- [ ] Animation smooth 60fps
- [ ] Auto-hide après 5s
- [ ] Positionnement correct (bottom-right, top-center)

---

#### Milestone 6: Storage + Tools + Polish
**Objectif**: Persistence et outils complets

**Composants**:
- StorageService (SwiftData/SQLite)
- Tous les 8 local tools
- SettingsService (@AppStorage)
- Settings UI
- Global shortcuts (Cmd+Shift+R/E)
- Menu bar (NSStatusItem)
- MCP integration (optionnel P2)

**Critères de réussite**:
- [ ] Recordings persistent en base
- [ ] Tous les tools fonctionnent
- [ ] Settings sauvegardés
- [ ] Shortcuts globaux actifs
- [ ] Tray icon avec menu

---

### D.4 Considérations Techniques

#### Concurrency Model

```swift
// Main engine as actor
@MainActor
final class RobertEngine: ObservableObject {
    @Published var copilotState: CopilotState = .idle
    @Published var currentTranscription: String = ""

    private let audioService: AudioCaptureService
    private let whisperService: WhisperService

    func startListening() async {
        for await event in audioService.events {
            switch event {
            case .streamingChunk(let samples):
                await processStreamingChunk(samples)
            case .speechEnded(let samples):
                await processUtterance(samples)
            }
        }
    }
}
```

#### Window Management

```swift
// Floating panel for overlay
class OverlayPanel: NSPanel {
    init() {
        super.init(
            contentRect: .zero,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isOpaque = false
        backgroundColor = .clear
        ignoresMouseEvents = false // Pour hover detection
    }
}
```

#### Whisper C Interop

```swift
// Via Swift Package avec bridging header
import whisper_cpp

class WhisperService {
    private var ctx: OpaquePointer?

    func transcribe(_ samples: [Float]) async throws -> String {
        // Appel whisper_full() sur thread background
        return try await Task.detached(priority: .userInitiated) {
            // whisper_full(ctx, params, samples, count)
        }.value
    }
}
```

---

### D.5 Questions / Unknowns

1. **Whisper.cpp Swift bindings**: Existe-t-il un package SPM mature ou faut-il créer un wrapper C?

2. **Metal GPU verification**: Comment vérifier que whisper.cpp utilise bien Metal sur le Mac cible?

3. **Global shortcuts conflicts**: Comment gérer les conflits si l'utilisateur a déjà Cmd+Shift+R assigné?

4. **App Sandbox**: L'app doit-elle être sandboxed pour le Mac App Store? Impact sur file access?

5. **Audio permission timing**: Quand exactement demander la permission microphone? Au premier launch ou plus tard?

6. **Menu bar only mode**: Faut-il supporter un mode sans dock icon (LSUIElement)?

7. **Window level priority**: Quel NSWindow.Level pour que l'overlay reste visible même sur fullscreen apps?

8. **Escape key in overlay**: Comment capturer Escape quand la fenêtre n'a pas le focus (panel non-activating)?

9. **System audio capture**: Est-ce que ScreenCaptureKit serait mieux que BlackHole pour l'audio système (P2)?

10. **Conversation persistence**: Faut-il persister l'historique conversationnel entre sessions?

---

## Annexes

### A. File Map (Tauri → Swift)

| Tauri Source | Swift Target |
|--------------|--------------|
| `lib.rs` (entry) | `RobertApp.swift` |
| `audio/capture.rs` | `Services/AudioCaptureService.swift` |
| `transcription/whisper.rs` | `Services/WhisperService.swift` |
| `transcription/streaming.rs` | `Services/StreamingTranscriber.swift` |
| `llm/anthropic.rs` | `Services/AnthropicClient.swift` |
| `tools/definitions.rs` | `Tools/ToolDefinitions.swift` |
| `tools/executor.rs` | `Tools/ToolExecutor.swift` |
| `tools/provider.rs` | `Tools/ToolProvider.swift` |
| `mcp/manager.rs` | `Services/MCPManager.swift` |
| `storage/database.rs` | `Services/StorageService.swift` |
| `state.rs` | `Models/AppState.swift` |
| `handlers.rs` | N/A (direct SwiftUI bindings) |
| `macos_tracking.rs` | NSTrackingArea ou NSEvent.addGlobalMonitor |
| `overlay.tsx` | `Views/OverlayView.swift` |
| `copilot.tsx` | `Views/CopilotView.swift` |
| `settings.tsx` | `Views/SettingsView.swift` |

### B. Dépendances Externes (Swift)

| Fonction | Package/Framework |
|----------|-------------------|
| Audio | AVFoundation (natif) |
| Whisper | whisper.cpp (C, via SPM ou manual) |
| HTTP | URLSession (natif) |
| JSON | Codable (natif) |
| Database | SwiftData (iOS 17+) ou GRDB.swift |
| Markdown | AttributedString (natif) ou swift-markdown |
| Shortcuts | Carbon (natif) ou MASShortcut (pod) |
