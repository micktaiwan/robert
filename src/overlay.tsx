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
  const [isStreaming, setIsStreaming] = useState(false);
  const [response, setResponse] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [status, setStatus] = useState("Loading...");
  const [showResponse, setShowResponse] = useState(false);
  const [error, setError] = useState("");
  const [isVisible, setIsVisible] = useState(true);
  const fadeTimeoutRef = React.useRef<number | null>(null);
  const isMouseOverRef = React.useRef(false);

  // Auto-hide after inactivity
  const resetFadeTimeout = () => {
    if (fadeTimeoutRef.current) {
      clearTimeout(fadeTimeoutRef.current);
    }
    setIsVisible(true);
    fadeTimeoutRef.current = window.setTimeout(() => {
      // Don't hide if mouse is over the overlay
      if (!isMouseOverRef.current) {
        setIsVisible(false);
      }
    }, 5000); // Fade after 5 seconds of inactivity
  };

  // Show overlay immediately (for activity)
  const showOverlay = () => {
    if (fadeTimeoutRef.current) {
      clearTimeout(fadeTimeoutRef.current);
      fadeTimeoutRef.current = null;
    }
    setIsVisible(true);
  };

  useEffect(() => {
    // Start the initial fade timeout
    resetFadeTimeout();

    // Native macOS mouse tracking events (from macos_tracking.rs)
    const unlistenMouseEnter = listen("overlay-mouse-enter", () => {
      console.log("Mouse entered overlay (native)");
      isMouseOverRef.current = true;
      showOverlay();
    });

    const unlistenMouseLeave = listen("overlay-mouse-leave", () => {
      console.log("Mouse left overlay (native)");
      isMouseOverRef.current = false;
      resetFadeTimeout();
    });

    const unlistenReady = listen("ready", () => {
      setStatus("");
      resetFadeTimeout();
    });

    const unlistenLoading = listen<string>("loading", (event) => {
      setStatus(event.payload);
      showOverlay();
    });

    const unlistenError = listen<string>("error", (event) => {
      setError(event.payload);
      setStatus("");
      showOverlay();
    });

    // Streaming transcription (orange)
    const unlistenStreaming = listen<string>("transcription-streaming", (event) => {
      setTranscription(event.payload);
      setIsStreaming(true);
      setShowResponse(false);
      showOverlay();
    });

    // Final transcription (white)
    const unlistenTranscription = listen<string>("transcription", (event) => {
      setTranscription(event.payload);
      setIsStreaming(false);
      setShowResponse(false);
      showOverlay();
      setTimeout(() => {
        setTranscription("");
        resetFadeTimeout();
      }, 3000);
    });

    const unlistenResponse = listen<string>("command-response", (event) => {
      setResponse(event.payload);
      setShowResponse(true);
      showOverlay();
      // Clear response after 5 seconds, then start fade
      setTimeout(() => {
        setShowResponse(false);
        setResponse("");
        resetFadeTimeout();
      }, 5000);
    });

    const unlistenRecordingStarted = listen<string>("recording-started", () => {
      setIsRecording(true);
      showOverlay();
    });

    const unlistenRecordingStopped = listen<string>("recording-stopped", () => {
      setIsRecording(false);
      resetFadeTimeout();
    });

    return () => {
      if (fadeTimeoutRef.current) {
        clearTimeout(fadeTimeoutRef.current);
      }
      unlistenMouseEnter.then((fn) => fn());
      unlistenMouseLeave.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenLoading.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenStreaming.then((fn) => fn());
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
        background: "rgba(20, 20, 25, 0.85)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "0 20px",
        boxSizing: "border-box",
        borderRadius: "12px",
        gap: "12px",
        fontFamily: "-apple-system, BlinkMacSystemFont, sans-serif",
        opacity: isVisible ? 1 : 0.01,
        transition: "opacity 0.5s ease-in-out",
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
            color: isStreaming ? "#ff9500" : "white",
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
