import React, { useEffect, useState, useRef } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";

// Configure marked for inline rendering
marked.setOptions({
  breaks: true,
  gfm: true,
  async: false,
});

type CopilotStateType = "listening" | "thinking" | "responding" | "idle";

interface CopilotUIState {
  visible: boolean;
  state: string;
  response_text: string;
  should_close: boolean;
  heard_text: string;
}

interface WaveAnimationProps {
  state: CopilotStateType;
}

function WaveAnimation({ state }: WaveAnimationProps) {
  const isActive = state !== "idle";
  const isListening = state === "listening";
  const isThinking = state === "thinking";

  const circles = [1, 2, 3, 4, 5];

  const getColor = () => {
    if (isListening) return "#007aff";
    if (isThinking) return "#ff9500";
    return "#34c759";
  };

  return (
    <div style={waveContainerStyle}>
      {circles.map((i) => (
        <div
          key={i}
          style={{
            position: "absolute",
            borderRadius: "50%",
            border: `2px solid ${getColor()}`,
            width: `${20 + i * 14}px`,
            height: `${20 + i * 14}px`,
            opacity: isActive ? 0.8 - i * 0.12 : 0,
            transition: "all 0.3s ease",
            animation: isActive
              ? isThinking
                ? `pulse-rotate 1.5s ease-in-out infinite ${i * 0.15}s`
                : `pulse-wave 2s ease-in-out infinite ${i * 0.15}s`
              : "none",
          }}
        />
      ))}
      <div
        style={{
          position: "absolute",
          width: "10px",
          height: "10px",
          borderRadius: "50%",
          backgroundColor: getColor(),
          transition: "background-color 0.3s ease",
          animation: isActive ? "pulse-dot 1s ease-in-out infinite" : "none",
        }}
      />
      <style>{`
        @keyframes pulse-wave {
          0%, 100% { transform: scale(1); opacity: 0.6; }
          50% { transform: scale(1.15); opacity: 0.25; }
        }
        @keyframes pulse-rotate {
          0% { transform: scale(1) rotate(0deg); opacity: 0.6; }
          50% { transform: scale(1.08) rotate(180deg); opacity: 0.35; }
          100% { transform: scale(1) rotate(360deg); opacity: 0.6; }
        }
        @keyframes pulse-dot {
          0%, 100% { transform: scale(1); }
          50% { transform: scale(1.3); }
        }
        @keyframes blink {
          0%, 50% { opacity: 1; }
          51%, 100% { opacity: 0; }
        }
      `}</style>
    </div>
  );
}

const waveContainerStyle: React.CSSProperties = {
  position: "relative",
  width: "100px",
  height: "100px",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  flexShrink: 0,
};

function Copilot() {
  const [state, setState] = useState<CopilotStateType>("idle");
  const [displayedText, setDisplayedText] = useState("");
  const [heardText, setHeardText] = useState("");
  const [isHovered, setIsHovered] = useState(false);

  const lastResponseTextRef = useRef<string>("");
  const animationRef = useRef<number | null>(null);
  const textQueueRef = useRef<string>("");
  const closeTimeoutRef = useRef<number | null>(null);
  const fadeTimeoutRef = useRef<number | null>(null);
  const isClosingRef = useRef<boolean>(false);
  const textContainerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll when text changes
  useEffect(() => {
    if (textContainerRef.current) {
      textContainerRef.current.scrollTop = textContainerRef.current.scrollHeight;
    }
  }, [displayedText]);

  // Text animation function - display multiple chars per frame for speed
  const animateText = () => {
    if (animationRef.current) return;

    const tick = () => {
      if (textQueueRef.current.length > 0) {
        // Display up to 5 characters per frame for faster animation
        const charsToShow = Math.min(5, textQueueRef.current.length);
        const nextChars = textQueueRef.current.slice(0, charsToShow);
        textQueueRef.current = textQueueRef.current.slice(charsToShow);
        setDisplayedText((prev) => prev + nextChars);
        animationRef.current = requestAnimationFrame(tick);
      } else {
        animationRef.current = null;
      }
    };

    animationRef.current = requestAnimationFrame(tick);
  };

  // Polling effect - no dependencies to avoid re-running
  useEffect(() => {
    const pollState = async () => {
      try {
        const backendState = await invoke<CopilotUIState>("get_copilot_state");
        const newState = backendState.state as CopilotStateType;

        // Update state
        setState(newState);

        // Update heard text
        setHeardText(backendState.heard_text || "");

        // Handle text changes - find new characters to animate
        if (backendState.response_text !== lastResponseTextRef.current) {
          const newText = backendState.response_text;
          const oldText = lastResponseTextRef.current;

          if (newText.startsWith(oldText)) {
            // New text is appended - animate the new part
            const newChars = newText.slice(oldText.length);
            textQueueRef.current += newChars;
            animateText();
          } else {
            // Text was reset - clear and start fresh
            setDisplayedText("");
            textQueueRef.current = newText;
            animateText();
          }

          lastResponseTextRef.current = newText;
        }

        // Handle close signal with fade-out
        if (backendState.should_close && !isClosingRef.current) {
          isClosingRef.current = true;
          // Start fade after 4.5s
          closeTimeoutRef.current = window.setTimeout(() => {
            // Animate window opacity from 1 to 0 over 500ms
            let alpha = 1.0;
            const fadeStep = () => {
              alpha -= 0.05;
              if (alpha <= 0) {
                // Fade complete, hide window
                if (!isClosingRef.current) return;
                invoke("hide_copilot").then(() => {
                  invoke("set_copilot_alpha", { alpha: 1.0 }); // Reset for next time
                  setState("idle");
                  setDisplayedText("");
                  lastResponseTextRef.current = "";
                  textQueueRef.current = "";
                  isClosingRef.current = false;
                });
              } else {
                invoke("set_copilot_alpha", { alpha });
                fadeTimeoutRef.current = window.setTimeout(fadeStep, 25);
              }
            };
            fadeStep();
          }, 4500);
        }

        // Reset closing state if should_close became false (new command started)
        if (!backendState.should_close && isClosingRef.current) {
          if (closeTimeoutRef.current) {
            clearTimeout(closeTimeoutRef.current);
            closeTimeoutRef.current = null;
          }
          if (fadeTimeoutRef.current) {
            clearTimeout(fadeTimeoutRef.current);
            fadeTimeoutRef.current = null;
          }
          isClosingRef.current = false;
          invoke("set_copilot_alpha", { alpha: 1.0 }); // Reset alpha if fade was in progress
        }

      } catch (e) {
        console.error("[Copilot] Polling error:", e);
      }
    };

    // Start polling
    const interval = setInterval(pollState, 100);

    // Initial poll
    pollState();

    // Escape key handler
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (closeTimeoutRef.current) {
          clearTimeout(closeTimeoutRef.current);
          closeTimeoutRef.current = null;
        }
        if (fadeTimeoutRef.current) {
          clearTimeout(fadeTimeoutRef.current);
          fadeTimeoutRef.current = null;
        }
        await invoke("hide_copilot");
        await invoke("set_copilot_alpha", { alpha: 1.0 });
        setState("idle");
        setDisplayedText("");
        lastResponseTextRef.current = "";
        textQueueRef.current = "";
        isClosingRef.current = false;
      }
    };
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      clearInterval(interval);
      window.removeEventListener("keydown", handleKeyDown);
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
      if (closeTimeoutRef.current) clearTimeout(closeTimeoutRef.current);
      if (fadeTimeoutRef.current) clearTimeout(fadeTimeoutRef.current);
    };
  }, []); // Empty deps - run once only

  const getStatusText = () => {
    if (state === "listening") return "Listening...";
    if (state === "thinking") return "Thinking...";
    return "";
  };

  const handleClose = async () => {
    if (closeTimeoutRef.current) {
      clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }
    if (fadeTimeoutRef.current) {
      clearTimeout(fadeTimeoutRef.current);
      fadeTimeoutRef.current = null;
    }
    await invoke("hide_copilot");
    await invoke("set_copilot_alpha", { alpha: 1.0 });
    setState("idle");
    setDisplayedText("");
    lastResponseTextRef.current = "";
    textQueueRef.current = "";
    isClosingRef.current = false;
  };

  return (
    <div
      style={containerStyle}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {isHovered && (
        <button
          onClick={handleClose}
          style={{
            position: "absolute",
            top: 8,
            right: 8,
            background: "rgba(255,255,255,0.1)",
            border: "none",
            borderRadius: "50%",
            width: 24,
            height: 24,
            cursor: "pointer",
            color: "#888",
            fontSize: 14,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          ✕
        </button>
      )}
      <WaveAnimation state={state} />
      {heardText && (state === "listening" || state === "thinking") && (
        <p style={heardTextStyle}>"{heardText}"</p>
      )}
      <div ref={textContainerRef} style={textContainerStyle} className="text-container">
        {(state === "listening" || state === "thinking") && !heardText && (
          <p style={statusTextStyle}>{getStatusText()}</p>
        )}
        {state === "responding" && (
          <div
            className="markdown-response"
            style={responseTextStyle}
            dangerouslySetInnerHTML={{
              __html: (marked.parse(displayedText) as string) + '<span style="animation: blink 1s infinite; color: #007aff;">|</span>',
            }}
          />
        )}
      </div>
      <style>{`
        .markdown-response p {
          margin: 0 0 0.3em 0;
        }
        .markdown-response p:last-of-type {
          margin-bottom: 0;
        }
        .markdown-response ul, .markdown-response ol {
          margin: 0.2em 0;
          padding-left: 1.2em;
        }
        .markdown-response li {
          margin: 0;
          padding: 0;
        }
        .markdown-response code {
          background: rgba(255, 255, 255, 0.1);
          padding: 0.1em 0.3em;
          border-radius: 3px;
          font-family: monospace;
        }
        .markdown-response pre {
          background: rgba(0, 0, 0, 0.3);
          padding: 0.5em;
          border-radius: 4px;
          overflow-x: auto;
        }
        .markdown-response strong {
          color: #fff;
        }
        .markdown-response a {
          color: #5ac8fa;
          text-decoration: underline;
        }
        .markdown-response h1, .markdown-response h2, .markdown-response h3 {
          margin: 0.5em 0 0.3em 0;
          color: #fff;
        }
        .markdown-response blockquote {
          border-left: 3px solid #555;
          margin: 0.5em 0;
          padding-left: 1em;
          color: #aaa;
        }
      `}</style>
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  position: "relative",
  width: "100%",
  height: "100%",
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "flex-start",
  padding: "24px",
  paddingTop: "30px",
  fontFamily: "-apple-system, BlinkMacSystemFont, sans-serif",
};

const textContainerStyle: React.CSSProperties = {
  marginTop: "12px",
  maxHeight: "420px",
  overflowY: "auto",
  width: "100%",
  paddingLeft: "12px",
  paddingRight: "12px",
};

const statusTextStyle: React.CSSProperties = {
  color: "#ccc",
  fontSize: "18px",
  fontWeight: "bold",
  textAlign: "center",
};

const responseTextStyle: React.CSSProperties = {
  color: "#e0e0e0",
  fontSize: "14px",
  lineHeight: "1.1",
  textAlign: "left",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
};

const heardTextStyle: React.CSSProperties = {
  color: "#ff9500",
  fontSize: "14px",
  fontStyle: "italic",
  textAlign: "center",
  marginTop: "8px",
  marginBottom: "0",
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Copilot />
  </React.StrictMode>
);
