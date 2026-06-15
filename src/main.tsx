import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/global.css";

// In a plain-browser dev build (Claude Preview, `pnpm dev`) there's no Rust
// backend, so install a fake Tauri bridge that serves the IPC commands and
// events. Tree-shaken out of the production/Tauri bundle by the DEV guard.
if (import.meta.env.DEV) {
  const { installMockTauri } = await import("./dev/mockTauri");
  installMockTauri();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
