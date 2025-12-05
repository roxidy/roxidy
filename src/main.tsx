import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";

/**
 * Check if we're running inside Tauri or in a browser.
 * When running in Tauri, window.__TAURI_INTERNALS__ is defined.
 */
function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Initialize the application.
 * In browser mode (not Tauri), load mocks before rendering.
 *
 * IMPORTANT: App is dynamically imported AFTER mocks are set up.
 * This ensures the mock event system is in place before hooks import
 * the listen() function from @tauri-apps/api/event.
 */
async function initApp(): Promise<void> {
  if (!isTauri()) {
    console.log("[App] Running in browser mode - loading Tauri IPC mocks");
    const { setupMocks } = await import("./mocks");
    setupMocks();
  }

  // Dynamic import AFTER mocks are set up
  // This ensures hooks get the patched listen() function
  const { default: App } = await import("./App");

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

initApp();
