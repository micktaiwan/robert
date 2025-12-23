import React from "react";
import ReactDOM from "react-dom/client";

// Main window (hidden, just for app lifecycle)
function App() {
  return null;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
