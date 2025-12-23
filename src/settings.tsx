import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface McpServerConfig {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
}

interface Settings {
  speech_threshold: number;
  silence_duration_ms: number;
  wake_words: string[];
  whisper_model: string;
  mic_device: string | null;
  system_audio_device: string | null;
  anthropic_api_key: string | null;
  mcp_servers: McpServerConfig[];
}

interface ModelInfo {
  name: string;
  size_mb: number;
  model_type: string;
}

interface DeviceInfo {
  name: string;
  is_default: boolean;
}

interface Recording {
  id: string;
  name: string;
  created_at: string;
  ended_at: string | null;
  is_active: boolean;
}

function SettingsPage() {
  const [settings, setSettings] = useState<Settings>({
    speech_threshold: 0.006,
    silence_duration_ms: 1000,
    wake_words: ["ok robert", "hey robert"],
    whisper_model: "ggml-small.bin",
    mic_device: null,
    system_audio_device: null,
    anthropic_api_key: null,
    mcp_servers: [],
  });
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [wakeWordsText, setWakeWordsText] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [saved, setSaved] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [activeTab, setActiveTab] = useState<"settings" | "recordings" | "mcp">("settings");

  // MCP server state
  const [newMcpServer, setNewMcpServer] = useState({ id: "", name: "", url: "" });
  const [testingServer, setTestingServer] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<{ serverId: string; tools: string[]; error?: string } | null>(null);

  useEffect(() => {
    loadData();

    const unlisten = listen("recording-stopped", () => {
      loadRecordings();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function loadData() {
    try {
      const s = await invoke<Settings>("get_settings");
      setSettings(s);
      setWakeWordsText(s.wake_words.join(", "));
      setApiKey(s.anthropic_api_key || "");
      const m = await invoke<ModelInfo[]>("get_models");
      setModels(m);
      const d = await invoke<DeviceInfo[]>("list_audio_devices");
      setDevices(d);
      await loadRecordings();
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async function loadRecordings() {
    try {
      const r = await invoke<Recording[]>("list_recordings");
      setRecordings(r);
    } catch (e) {
      console.error("Failed to load recordings:", e);
    }
  }

  async function saveSettings() {
    try {
      const newSettings: Settings = {
        ...settings,
        wake_words: wakeWordsText.split(",").map((w) => w.trim().toLowerCase()),
        anthropic_api_key: apiKey || null,
      };
      await invoke("save_settings", { settings: newSettings });
      setSettings(newSettings);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  }

  async function renameRecording(id: string, newName: string) {
    try {
      await invoke("rename_recording", { recordingId: id, newName });
      setEditingId(null);
      await loadRecordings();
    } catch (e) {
      console.error("Failed to rename recording:", e);
    }
  }

  async function deleteRecording(id: string) {
    if (!confirm("Are you sure you want to delete this recording?")) return;
    try {
      await invoke("delete_recording", { recordingId: id });
      await loadRecordings();
    } catch (e) {
      console.error("Failed to delete recording:", e);
    }
  }

  // MCP Server functions
  async function addMcpServer() {
    if (!newMcpServer.id || !newMcpServer.name || !newMcpServer.url) {
      alert("Please fill in all fields");
      return;
    }

    const newServer: McpServerConfig = {
      ...newMcpServer,
      enabled: true,
    };

    const newSettings = {
      ...settings,
      mcp_servers: [...settings.mcp_servers, newServer],
    };

    try {
      await invoke("save_settings", { settings: newSettings });
      setSettings(newSettings);
      setNewMcpServer({ id: "", name: "", url: "" });
    } catch (e) {
      console.error("Failed to add MCP server:", e);
    }
  }

  async function removeMcpServer(id: string) {
    const newSettings = {
      ...settings,
      mcp_servers: settings.mcp_servers.filter((s) => s.id !== id),
    };

    try {
      await invoke("save_settings", { settings: newSettings });
      setSettings(newSettings);
    } catch (e) {
      console.error("Failed to remove MCP server:", e);
    }
  }

  async function toggleMcpServer(id: string) {
    const newSettings = {
      ...settings,
      mcp_servers: settings.mcp_servers.map((s) =>
        s.id === id ? { ...s, enabled: !s.enabled } : s
      ),
    };

    try {
      await invoke("save_settings", { settings: newSettings });
      setSettings(newSettings);
    } catch (e) {
      console.error("Failed to toggle MCP server:", e);
    }
  }

  async function testMcpServer(url: string, id: string) {
    setTestingServer(id);
    setTestResult(null);
    try {
      const tools = await invoke<string[]>("test_mcp_server", { url });
      setTestResult({ serverId: id, tools });
    } catch (e) {
      setTestResult({ serverId: id, tools: [], error: String(e) });
    } finally {
      setTestingServer(null);
    }
  }

  const whisperModels = models.filter((m) => m.model_type === "Whisper");

  const inputStyle = {
    width: "100%",
    padding: "8px",
    borderRadius: "4px",
    border: "1px solid #ccc",
    boxSizing: "border-box" as const,
  };

  const tabStyle = (active: boolean) => ({
    padding: "10px 20px",
    background: active ? "#007aff" : "#e0e0e0",
    color: active ? "white" : "#333",
    border: "none",
    borderRadius: "6px 6px 0 0",
    cursor: "pointer",
    fontSize: "14px",
    marginRight: "4px",
  });

  return (
    <div style={{ padding: "24px", maxWidth: "600px", margin: "0 auto" }}>
      <h1 style={{ fontSize: "24px", marginBottom: "16px" }}>Robert Settings</h1>

      <div style={{ marginBottom: "16px" }}>
        <button style={tabStyle(activeTab === "settings")} onClick={() => setActiveTab("settings")}>
          Settings
        </button>
        <button style={tabStyle(activeTab === "recordings")} onClick={() => setActiveTab("recordings")}>
          Recordings ({recordings.length})
        </button>
        <button style={tabStyle(activeTab === "mcp")} onClick={() => setActiveTab("mcp")}>
          MCP Servers ({settings.mcp_servers.filter((s) => s.enabled).length})
        </button>
      </div>

      {activeTab === "settings" && (
        <>
          <section style={{ marginBottom: "24px" }}>
            <h2 style={{ fontSize: "18px", marginBottom: "12px" }}>Anthropic API</h2>
            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>API Key</span>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="sk-ant-..."
                style={inputStyle}
              />
              <small style={{ color: "#666", display: "block", marginTop: "4px" }}>
                Get your API key from console.anthropic.com
              </small>
            </label>
          </section>

          <section style={{ marginBottom: "24px" }}>
            <h2 style={{ fontSize: "18px", marginBottom: "12px" }}>Audio Devices</h2>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>Microphone</span>
              <select
                value={settings.mic_device || ""}
                onChange={(e) => setSettings({ ...settings, mic_device: e.target.value || null })}
                style={inputStyle}
              >
                <option value="">Default</option>
                {devices.map((d) => (
                  <option key={d.name} value={d.name}>
                    {d.name} {d.is_default ? "(Default)" : ""}
                  </option>
                ))}
              </select>
            </label>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>System Audio (BlackHole)</span>
              <select
                value={settings.system_audio_device || ""}
                onChange={(e) => setSettings({ ...settings, system_audio_device: e.target.value || null })}
                style={inputStyle}
              >
                <option value="">None</option>
                {devices.map((d) => (
                  <option key={d.name} value={d.name}>
                    {d.name} {d.is_default ? "(Default)" : ""}
                  </option>
                ))}
              </select>
              <small style={{ color: "#666", display: "block", marginTop: "4px" }}>
                Install BlackHole to capture system audio: brew install blackhole-2ch
              </small>
            </label>
          </section>

          <section style={{ marginBottom: "24px" }}>
            <h2 style={{ fontSize: "18px", marginBottom: "12px" }}>Voice Detection</h2>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>
                Speech Threshold: {settings.speech_threshold.toFixed(3)}
              </span>
              <input
                type="range"
                min="0.001"
                max="0.02"
                step="0.001"
                value={settings.speech_threshold}
                onChange={(e) =>
                  setSettings({ ...settings, speech_threshold: parseFloat(e.target.value) })
                }
                style={{ width: "100%" }}
              />
            </label>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>Silence Duration (ms)</span>
              <input
                type="number"
                min="500"
                max="3000"
                value={settings.silence_duration_ms}
                onChange={(e) =>
                  setSettings({ ...settings, silence_duration_ms: parseInt(e.target.value) })
                }
                style={inputStyle}
              />
            </label>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <span style={{ display: "block", marginBottom: "4px" }}>
                Wake Words (comma-separated)
              </span>
              <input
                type="text"
                value={wakeWordsText}
                onChange={(e) => setWakeWordsText(e.target.value)}
                placeholder="ok robert, hey robert"
                style={inputStyle}
              />
            </label>
          </section>

          <section style={{ marginBottom: "24px" }}>
            <h2 style={{ fontSize: "18px", marginBottom: "12px" }}>Whisper Model</h2>

            <label style={{ display: "block", marginBottom: "16px" }}>
              <select
                value={settings.whisper_model}
                onChange={(e) => setSettings({ ...settings, whisper_model: e.target.value })}
                style={inputStyle}
              >
                {whisperModels.map((m) => (
                  <option key={m.name} value={m.name}>
                    {m.name} ({m.size_mb} MB)
                  </option>
                ))}
              </select>
            </label>
          </section>

          <button
            onClick={saveSettings}
            style={{
              padding: "12px 24px",
              background: saved ? "#34c759" : "#007aff",
              color: "white",
              border: "none",
              borderRadius: "6px",
              cursor: "pointer",
              fontSize: "16px",
            }}
          >
            {saved ? "Saved!" : "Save Settings"}
          </button>
        </>
      )}

      {activeTab === "recordings" && (
        <section>
          <div style={{ marginBottom: "16px", color: "#666" }}>
            <small>Press Cmd+Shift+E to start/stop recording</small>
          </div>

          {recordings.length === 0 ? (
            <p style={{ color: "#666" }}>No recordings yet.</p>
          ) : (
            <ul style={{ listStyle: "none", padding: 0 }}>
              {recordings.map((r) => (
                <li
                  key={r.id}
                  style={{
                    padding: "12px",
                    background: "white",
                    borderRadius: "6px",
                    marginBottom: "8px",
                    border: r.is_active ? "2px solid #ff3b30" : "1px solid #e0e0e0",
                  }}
                >
                  {editingId === r.id ? (
                    <div style={{ display: "flex", gap: "8px" }}>
                      <input
                        type="text"
                        value={editingName}
                        onChange={(e) => setEditingName(e.target.value)}
                        style={{ flex: 1, padding: "4px 8px", borderRadius: "4px", border: "1px solid #ccc" }}
                        autoFocus
                        onKeyDown={(e) => {
                          if (e.key === "Enter") renameRecording(r.id, editingName);
                          if (e.key === "Escape") setEditingId(null);
                        }}
                      />
                      <button
                        onClick={() => renameRecording(r.id, editingName)}
                        style={{ padding: "4px 12px", background: "#007aff", color: "white", border: "none", borderRadius: "4px", cursor: "pointer" }}
                      >
                        Save
                      </button>
                      <button
                        onClick={() => setEditingId(null)}
                        style={{ padding: "4px 12px", background: "#e0e0e0", border: "none", borderRadius: "4px", cursor: "pointer" }}
                      >
                        Cancel
                      </button>
                    </div>
                  ) : (
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <div>
                        <div style={{ fontWeight: "500" }}>
                          {r.name}
                          {r.is_active && (
                            <span style={{ marginLeft: "8px", color: "#ff3b30", fontSize: "12px" }}>
                              RECORDING
                            </span>
                          )}
                        </div>
                        <div style={{ fontSize: "12px", color: "#666" }}>
                          {new Date(r.created_at).toLocaleString()}
                        </div>
                      </div>
                      <div style={{ display: "flex", gap: "8px" }}>
                        <button
                          onClick={() => {
                            setEditingId(r.id);
                            setEditingName(r.name);
                          }}
                          style={{ padding: "4px 8px", background: "#e0e0e0", border: "none", borderRadius: "4px", cursor: "pointer", fontSize: "12px" }}
                        >
                          Rename
                        </button>
                        <button
                          onClick={() => deleteRecording(r.id)}
                          style={{ padding: "4px 8px", background: "#ff3b30", color: "white", border: "none", borderRadius: "4px", cursor: "pointer", fontSize: "12px" }}
                          disabled={r.is_active}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {activeTab === "mcp" && (
        <section>
          <p style={{ color: "#666", marginBottom: "16px" }}>
            Connect to MCP servers to extend Robert with external tools.
          </p>

          {/* Add new server form */}
          <div
            style={{
              padding: "16px",
              background: "white",
              borderRadius: "6px",
              marginBottom: "16px",
              border: "1px solid #e0e0e0",
            }}
          >
            <h3 style={{ fontSize: "14px", marginBottom: "12px" }}>Add MCP Server</h3>
            <div style={{ display: "flex", gap: "8px", marginBottom: "8px" }}>
              <input
                type="text"
                placeholder="ID (e.g., panorama)"
                value={newMcpServer.id}
                onChange={(e) => setNewMcpServer({ ...newMcpServer, id: e.target.value })}
                style={{ ...inputStyle, flex: 1 }}
              />
              <input
                type="text"
                placeholder="Name (e.g., Panorama Tasks)"
                value={newMcpServer.name}
                onChange={(e) => setNewMcpServer({ ...newMcpServer, name: e.target.value })}
                style={{ ...inputStyle, flex: 1 }}
              />
            </div>
            <div style={{ display: "flex", gap: "8px" }}>
              <input
                type="text"
                placeholder="URL (e.g., http://localhost:3000/mcp)"
                value={newMcpServer.url}
                onChange={(e) => setNewMcpServer({ ...newMcpServer, url: e.target.value })}
                style={{ ...inputStyle, flex: 1 }}
              />
              <button
                onClick={addMcpServer}
                style={{
                  padding: "8px 16px",
                  background: "#007aff",
                  color: "white",
                  border: "none",
                  borderRadius: "4px",
                  cursor: "pointer",
                }}
              >
                Add
              </button>
            </div>
          </div>

          {/* Server list */}
          {settings.mcp_servers.length === 0 ? (
            <p style={{ color: "#666" }}>No MCP servers configured.</p>
          ) : (
            <ul style={{ listStyle: "none", padding: 0 }}>
              {settings.mcp_servers.map((server) => (
                <li
                  key={server.id}
                  style={{
                    padding: "12px",
                    background: "white",
                    borderRadius: "6px",
                    marginBottom: "8px",
                    border: server.enabled ? "1px solid #34c759" : "1px solid #e0e0e0",
                    opacity: server.enabled ? 1 : 0.6,
                  }}
                >
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div>
                      <div style={{ fontWeight: "500" }}>
                        {server.name}
                        <span style={{ marginLeft: "8px", color: "#666", fontSize: "12px" }}>
                          ({server.id})
                        </span>
                        {server.enabled && (
                          <span style={{ marginLeft: "8px", color: "#34c759", fontSize: "12px" }}>
                            ENABLED
                          </span>
                        )}
                      </div>
                      <div style={{ fontSize: "12px", color: "#666" }}>{server.url}</div>
                    </div>
                    <div style={{ display: "flex", gap: "8px" }}>
                      <button
                        onClick={() => testMcpServer(server.url, server.id)}
                        disabled={testingServer === server.id}
                        style={{
                          padding: "4px 8px",
                          background: "#007aff",
                          color: "white",
                          border: "none",
                          borderRadius: "4px",
                          cursor: testingServer === server.id ? "wait" : "pointer",
                          fontSize: "12px",
                        }}
                      >
                        {testingServer === server.id ? "Testing..." : "Test"}
                      </button>
                      <button
                        onClick={() => toggleMcpServer(server.id)}
                        style={{
                          padding: "4px 8px",
                          background: server.enabled ? "#ff9500" : "#34c759",
                          color: "white",
                          border: "none",
                          borderRadius: "4px",
                          cursor: "pointer",
                          fontSize: "12px",
                        }}
                      >
                        {server.enabled ? "Disable" : "Enable"}
                      </button>
                      <button
                        onClick={() => removeMcpServer(server.id)}
                        style={{
                          padding: "4px 8px",
                          background: "#ff3b30",
                          color: "white",
                          border: "none",
                          borderRadius: "4px",
                          cursor: "pointer",
                          fontSize: "12px",
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  </div>

                  {/* Test results */}
                  {testResult && testResult.serverId === server.id && (
                    <div
                      style={{
                        marginTop: "12px",
                        padding: "8px",
                        background: testResult.error ? "#ffebee" : "#e8f5e9",
                        borderRadius: "4px",
                        fontSize: "12px",
                        border: testResult.error ? "1px solid #ef5350" : "1px solid #66bb6a",
                      }}
                    >
                      {testResult.error ? (
                        <div style={{ color: "#c62828" }}>
                          <div style={{ fontWeight: "500", marginBottom: "4px" }}>Connection failed:</div>
                          <div>{testResult.error}</div>
                        </div>
                      ) : (
                        <>
                          <div style={{ fontWeight: "500", marginBottom: "4px", color: "#2e7d32" }}>
                            Connected! Available tools ({testResult.tools.length}):
                          </div>
                          <div style={{ maxHeight: "100px", overflowY: "auto" }}>
                            {testResult.tools.map((tool) => (
                              <div key={tool} style={{ color: "#666" }}>
                                {tool}
                              </div>
                            ))}
                          </div>
                        </>
                      )}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SettingsPage />
  </React.StrictMode>
);
