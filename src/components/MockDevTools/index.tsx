/**
 * MockDevTools - Development panel for triggering mock Tauri events and commands.
 *
 * This component is only rendered in browser mode (when not running in Tauri).
 * It provides a UI for simulating backend events to test frontend behavior.
 */

import { useState, useCallback } from "react";
import {
  emitTerminalOutput,
  emitCommandBlock,
  emitDirectoryChanged,
  emitSessionEnded,
  emitAiEvent,
  simulateAiResponse,
  type AiEventType,
} from "@/mocks";

// =============================================================================
// Types
// =============================================================================

type TabId = "terminal" | "ai" | "session";

interface TabConfig {
  id: TabId;
  label: string;
}

const TABS: TabConfig[] = [
  { id: "terminal", label: "Terminal" },
  { id: "ai", label: "AI" },
  { id: "session", label: "Session" },
];

// =============================================================================
// Styles (inline for self-contained component)
// =============================================================================

const styles = {
  toggleButton: {
    position: "fixed" as const,
    bottom: "16px",
    right: "16px",
    width: "48px",
    height: "48px",
    borderRadius: "50%",
    backgroundColor: "#6366f1",
    color: "white",
    border: "none",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    fontSize: "20px",
    boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
    zIndex: 9999,
  },
  panel: {
    position: "fixed" as const,
    bottom: "80px",
    right: "16px",
    width: "380px",
    maxHeight: "500px",
    backgroundColor: "#1e1e2e",
    borderRadius: "12px",
    boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
    zIndex: 9998,
    overflow: "hidden",
    display: "flex",
    flexDirection: "column" as const,
  },
  header: {
    padding: "12px 16px",
    backgroundColor: "#313244",
    borderBottom: "1px solid #45475a",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  title: {
    margin: 0,
    fontSize: "14px",
    fontWeight: 600,
    color: "#cdd6f4",
  },
  badge: {
    fontSize: "10px",
    padding: "2px 6px",
    backgroundColor: "#f38ba8",
    color: "#1e1e2e",
    borderRadius: "4px",
    fontWeight: 600,
  },
  tabs: {
    display: "flex",
    borderBottom: "1px solid #45475a",
  },
  tab: {
    flex: 1,
    padding: "10px",
    backgroundColor: "transparent",
    border: "none",
    color: "#a6adc8",
    cursor: "pointer",
    fontSize: "12px",
    fontWeight: 500,
    transition: "all 0.2s",
  },
  tabActive: {
    backgroundColor: "#313244",
    color: "#cdd6f4",
    borderBottom: "2px solid #89b4fa",
  },
  content: {
    padding: "16px",
    overflowY: "auto" as const,
    flex: 1,
  },
  section: {
    marginBottom: "16px",
  },
  sectionTitle: {
    fontSize: "11px",
    fontWeight: 600,
    color: "#6c7086",
    textTransform: "uppercase" as const,
    marginBottom: "8px",
    letterSpacing: "0.5px",
  },
  inputGroup: {
    marginBottom: "12px",
  },
  label: {
    display: "block",
    fontSize: "12px",
    color: "#a6adc8",
    marginBottom: "4px",
  },
  input: {
    width: "100%",
    padding: "8px 10px",
    backgroundColor: "#313244",
    border: "1px solid #45475a",
    borderRadius: "6px",
    color: "#cdd6f4",
    fontSize: "12px",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  textarea: {
    width: "100%",
    padding: "8px 10px",
    backgroundColor: "#313244",
    border: "1px solid #45475a",
    borderRadius: "6px",
    color: "#cdd6f4",
    fontSize: "12px",
    outline: "none",
    resize: "vertical" as const,
    minHeight: "60px",
    fontFamily: "monospace",
    boxSizing: "border-box" as const,
  },
  button: {
    padding: "8px 16px",
    backgroundColor: "#89b4fa",
    color: "#1e1e2e",
    border: "none",
    borderRadius: "6px",
    cursor: "pointer",
    fontSize: "12px",
    fontWeight: 600,
    marginRight: "8px",
    marginBottom: "8px",
    transition: "opacity 0.2s",
  },
  buttonSecondary: {
    backgroundColor: "#45475a",
    color: "#cdd6f4",
  },
  buttonDanger: {
    backgroundColor: "#f38ba8",
  },
  buttonSuccess: {
    backgroundColor: "#a6e3a1",
  },
  quickActions: {
    display: "flex",
    flexWrap: "wrap" as const,
    gap: "8px",
  },
  log: {
    fontSize: "11px",
    color: "#6c7086",
    fontStyle: "italic" as const,
    marginTop: "8px",
  },
};

// =============================================================================
// Component
// =============================================================================

export function MockDevTools() {
  const [isOpen, setIsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<TabId>("terminal");
  const [lastAction, setLastAction] = useState<string>("");

  // Terminal state
  const [sessionId, setSessionId] = useState("mock-session-001");
  const [terminalOutput, setTerminalOutput] = useState("Hello from mock terminal!\n");
  const [command, setCommand] = useState("ls -la");
  const [commandOutput, setCommandOutput] = useState("total 0\ndrwxr-xr-x  2 user user  40 Jan 15 10:00 .\ndrwxr-xr-x 10 user user 200 Jan 15 09:00 ..");
  const [exitCode, setExitCode] = useState(0);
  const [workingDir, setWorkingDir] = useState("/home/user/project");

  // AI state
  const [aiResponse, setAiResponse] = useState("I'll help you with that task. Let me analyze the code and provide suggestions.");
  const [streamDelay, setStreamDelay] = useState(30);
  const [toolName, setToolName] = useState("read_file");
  const [toolArgs, setToolArgs] = useState('{"path": "/home/user/file.txt"}');

  const logAction = useCallback((action: string) => {
    setLastAction(`${new Date().toLocaleTimeString()}: ${action}`);
  }, []);

  // Terminal handlers
  const handleEmitOutput = useCallback(async () => {
    await emitTerminalOutput(sessionId, terminalOutput);
    logAction(`Emitted terminal output (${terminalOutput.length} chars)`);
  }, [sessionId, terminalOutput, logAction]);

  const handleEmitCommandBlock = useCallback(async () => {
    await emitCommandBlock(sessionId, command, commandOutput, exitCode, workingDir);
    logAction(`Emitted command block: ${command}`);
  }, [sessionId, command, commandOutput, exitCode, workingDir, logAction]);

  const handleEmitDirectoryChanged = useCallback(async () => {
    await emitDirectoryChanged(sessionId, workingDir);
    logAction(`Emitted directory changed: ${workingDir}`);
  }, [sessionId, workingDir, logAction]);

  // AI handlers
  const handleSimulateResponse = useCallback(async () => {
    logAction("Starting AI response simulation...");
    await simulateAiResponse(aiResponse, streamDelay);
    logAction("AI response simulation completed");
  }, [aiResponse, streamDelay, logAction]);

  const handleEmitToolRequest = useCallback(async () => {
    const event: AiEventType = {
      type: "tool_request",
      tool_name: toolName,
      args: JSON.parse(toolArgs),
      request_id: `req-${Date.now()}`,
    };
    await emitAiEvent(event);
    logAction(`Emitted tool request: ${toolName}`);
  }, [toolName, toolArgs, logAction]);

  const handleEmitToolResult = useCallback(async () => {
    const event: AiEventType = {
      type: "tool_result",
      tool_name: toolName,
      result: "Mock tool result content",
      success: true,
      request_id: `req-${Date.now()}`,
    };
    await emitAiEvent(event);
    logAction(`Emitted tool result: ${toolName}`);
  }, [toolName, logAction]);

  const handleEmitError = useCallback(async () => {
    const event: AiEventType = {
      type: "error",
      message: "Mock error for testing",
      error_type: "MockError",
    };
    await emitAiEvent(event);
    logAction("Emitted AI error event");
  }, [logAction]);

  // Session handlers
  const handleEmitSessionEnded = useCallback(async () => {
    await emitSessionEnded(sessionId);
    logAction(`Emitted session ended: ${sessionId}`);
  }, [sessionId, logAction]);

  // Render tab content
  const renderTabContent = () => {
    switch (activeTab) {
      case "terminal":
        return (
          <>
            <div style={styles.section}>
              <div style={styles.sectionTitle}>Session</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Session ID</label>
                <input
                  type="text"
                  style={styles.input}
                  value={sessionId}
                  onChange={(e) => setSessionId(e.target.value)}
                />
              </div>
            </div>

            <div style={styles.section}>
              <div style={styles.sectionTitle}>Terminal Output</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Output Data</label>
                <textarea
                  style={styles.textarea}
                  value={terminalOutput}
                  onChange={(e) => setTerminalOutput(e.target.value)}
                />
              </div>
              <button style={styles.button} onClick={handleEmitOutput}>
                Emit Output
              </button>
            </div>

            <div style={styles.section}>
              <div style={styles.sectionTitle}>Command Block</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Command</label>
                <input
                  type="text"
                  style={styles.input}
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                />
              </div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Output</label>
                <textarea
                  style={styles.textarea}
                  value={commandOutput}
                  onChange={(e) => setCommandOutput(e.target.value)}
                />
              </div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Exit Code</label>
                <input
                  type="number"
                  style={styles.input}
                  value={exitCode}
                  onChange={(e) => setExitCode(Number(e.target.value))}
                />
              </div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Working Directory</label>
                <input
                  type="text"
                  style={styles.input}
                  value={workingDir}
                  onChange={(e) => setWorkingDir(e.target.value)}
                />
              </div>
              <button style={styles.button} onClick={handleEmitCommandBlock}>
                Emit Command Block
              </button>
              <button
                style={{ ...styles.button, ...styles.buttonSecondary }}
                onClick={handleEmitDirectoryChanged}
              >
                Emit Directory Changed
              </button>
            </div>
          </>
        );

      case "ai":
        return (
          <>
            <div style={styles.section}>
              <div style={styles.sectionTitle}>Streaming Response</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Response Text</label>
                <textarea
                  style={styles.textarea}
                  value={aiResponse}
                  onChange={(e) => setAiResponse(e.target.value)}
                />
              </div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Stream Delay (ms)</label>
                <input
                  type="number"
                  style={styles.input}
                  value={streamDelay}
                  onChange={(e) => setStreamDelay(Number(e.target.value))}
                />
              </div>
              <button style={styles.button} onClick={handleSimulateResponse}>
                Simulate Response
              </button>
            </div>

            <div style={styles.section}>
              <div style={styles.sectionTitle}>Tool Events</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Tool Name</label>
                <input
                  type="text"
                  style={styles.input}
                  value={toolName}
                  onChange={(e) => setToolName(e.target.value)}
                />
              </div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Tool Arguments (JSON)</label>
                <textarea
                  style={styles.textarea}
                  value={toolArgs}
                  onChange={(e) => setToolArgs(e.target.value)}
                />
              </div>
              <button style={styles.button} onClick={handleEmitToolRequest}>
                Emit Tool Request
              </button>
              <button
                style={{ ...styles.button, ...styles.buttonSuccess }}
                onClick={handleEmitToolResult}
              >
                Emit Tool Result
              </button>
            </div>

            <div style={styles.section}>
              <div style={styles.sectionTitle}>Quick Actions</div>
              <div style={styles.quickActions}>
                <button
                  style={{ ...styles.button, ...styles.buttonDanger }}
                  onClick={handleEmitError}
                >
                  Emit Error
                </button>
              </div>
            </div>
          </>
        );

      case "session":
        return (
          <>
            <div style={styles.section}>
              <div style={styles.sectionTitle}>Session Management</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Session ID</label>
                <input
                  type="text"
                  style={styles.input}
                  value={sessionId}
                  onChange={(e) => setSessionId(e.target.value)}
                />
              </div>
              <button
                style={{ ...styles.button, ...styles.buttonDanger }}
                onClick={handleEmitSessionEnded}
              >
                End Session
              </button>
            </div>

            <div style={styles.section}>
              <div style={styles.sectionTitle}>Presets</div>
              <div style={styles.quickActions}>
                <button
                  style={{ ...styles.button, ...styles.buttonSecondary }}
                  onClick={() => {
                    setSessionId(`mock-session-${Date.now()}`);
                    logAction("Generated new session ID");
                  }}
                >
                  New Session ID
                </button>
              </div>
            </div>
          </>
        );
    }
  };

  return (
    <>
      {/* Toggle Button */}
      <button
        style={styles.toggleButton}
        onClick={() => setIsOpen(!isOpen)}
        title="Toggle Mock Dev Tools"
      >
        {isOpen ? "✕" : "🔧"}
      </button>

      {/* Panel */}
      {isOpen && (
        <div style={styles.panel}>
          {/* Header */}
          <div style={styles.header}>
            <h3 style={styles.title}>Mock Dev Tools</h3>
            <span style={styles.badge}>BROWSER MODE</span>
          </div>

          {/* Tabs */}
          <div style={styles.tabs}>
            {TABS.map((tab) => (
              <button
                key={tab.id}
                style={{
                  ...styles.tab,
                  ...(activeTab === tab.id ? styles.tabActive : {}),
                }}
                onClick={() => setActiveTab(tab.id)}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Content */}
          <div style={styles.content}>
            {renderTabContent()}

            {/* Last Action Log */}
            {lastAction && <div style={styles.log}>{lastAction}</div>}
          </div>
        </div>
      )}
    </>
  );
}

export default MockDevTools;
