import React, { useEffect, useState, useMemo } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { marked } from "marked";

// Configure marked for inline rendering
marked.setOptions({
  breaks: true,
  gfm: true,
  async: false,
});

function Overlay() {
  const [transcription, setTranscription] = useState("");
  const [response, setResponse] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [status, setStatus] = useState("Loading...");
  const [showResponse, setShowResponse] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const unlistenReady = listen("ready", () => {
      setStatus("");
    });

    const unlistenLoading = listen<string>("loading", (event) => {
      setStatus(event.payload);
    });

    const unlistenError = listen<string>("error", (event) => {
      setError(event.payload);
      setStatus("");
    });

    const unlistenTranscription = listen<string>("transcription", (event) => {
      setTranscription(event.payload);
      setShowResponse(false);
      // Clear transcription after 3 seconds
      setTimeout(() => setTranscription(""), 3000);
    });

    const unlistenResponse = listen<string>("command-response", (event) => {
      setResponse(event.payload);
      setShowResponse(true);
      // Clear response after 5 seconds
      setTimeout(() => {
        setShowResponse(false);
        setResponse("");
      }, 5000);
    });

    const unlistenRecordingStarted = listen<string>("recording-started", () => {
      setIsRecording(true);
    });

    const unlistenRecordingStopped = listen<string>("recording-stopped", () => {
      setIsRecording(false);
    });

    return () => {
      unlistenReady.then((fn) => fn());
      unlistenLoading.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenTranscription.then((fn) => fn());
      unlistenResponse.then((fn) => fn());
      unlistenRecordingStarted.then((fn) => fn());
      unlistenRecordingStopped.then((fn) => fn());
    };
  }, []);

  // Convert markdown to HTML for responses
  const responseHtml = useMemo(() => {
    if (!showResponse || !response) return "";
    return marked.parse(response) as string;
  }, [showResponse, response]);

  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        background: "rgba(30, 30, 30, 0.95)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "0 16px",
        boxSizing: "border-box",
        borderRadius: "12px",
        gap: "12px",
      }}
    >
      {isRecording && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "6px",
            padding: "4px 10px",
            background: "#ff3b30",
            borderRadius: "4px",
            flexShrink: 0,
          }}
        >
          <div
            style={{
              width: "8px",
              height: "8px",
              background: "white",
              borderRadius: "50%",
              animation: "pulse 1s infinite",
            }}
          />
          <span style={{ color: "white", fontSize: "12px", fontWeight: "bold" }}>
            REC
          </span>
        </div>
      )}

      {error ? (
        <p style={{ color: "#ff3b30", fontSize: "14px", margin: 0, whiteSpace: "pre-wrap", textAlign: "left", flex: 1 }}>{error}</p>
      ) : status ? (
        <p style={{ color: "#888", fontSize: "16px", margin: 0 }}>{status}</p>
      ) : showResponse && responseHtml ? (
        <div
          className="markdown-response"
          style={{
            color: "#34c759",
            fontSize: "16px",
            margin: 0,
            textAlign: "left",
            overflow: "auto",
            flex: 1,
            maxHeight: "100%",
          }}
          dangerouslySetInnerHTML={{ __html: responseHtml }}
        />
      ) : transcription ? (
        <p
          style={{
            color: "white",
            fontSize: "18px",
            margin: 0,
            textAlign: "center",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            flex: 1,
          }}
        >
          {transcription}
        </p>
      ) : (
        <p style={{ color: "#666", fontSize: "16px", margin: 0 }}>
          Say "OK Robert" to give a command
        </p>
      )}

      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.5; }
        }
        .markdown-response p {
          margin: 0 0 0.5em 0;
        }
        .markdown-response p:last-child {
          margin-bottom: 0;
        }
        .markdown-response ul, .markdown-response ol {
          margin: 0.5em 0;
          padding-left: 1.5em;
        }
        .markdown-response li {
          margin: 0.2em 0;
        }
        .markdown-response code {
          background: rgba(255, 255, 255, 0.1);
          padding: 0.1em 0.3em;
          border-radius: 3px;
          font-family: monospace;
        }
        .markdown-response strong {
          color: #4cd964;
        }
        .markdown-response a {
          color: #5ac8fa;
          text-decoration: underline;
        }
      `}</style>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>
);
