/**
 * MockDevTools - Development panel for triggering mock Tauri events and commands.
 *
 * This component is only rendered in browser mode (when not running in Tauri).
 * It provides a UI for simulating backend events to test frontend behavior.
 */

import { useState, useCallback } from "react";
import {
  emitTerminalOutput,
  simulateCommand,
  emitDirectoryChanged,
  emitSessionEnded,
  emitAiEvent,
  simulateAiResponse,
  type AiEventType,
} from "@/mocks";

// =============================================================================
// Types
// =============================================================================

type TabId = "terminal" | "ai" | "session" | "presets";

interface TabConfig {
  id: TabId;
  label: string;
}

const TABS: TabConfig[] = [
  { id: "presets", label: "Presets" },
  { id: "terminal", label: "Terminal" },
  { id: "ai", label: "AI" },
  { id: "session", label: "Session" },
];

// =============================================================================
// Preset Scenarios
// =============================================================================

interface Preset {
  id: string;
  name: string;
  description: string;
  icon: string;
  color: string;
  run: (sessionId: string, log: (msg: string) => void) => Promise<void>;
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const PRESETS: Preset[] = [
  {
    id: "fresh-start",
    name: "Fresh Start",
    description: "Clean slate - just a welcome message",
    icon: "🌱",
    color: "#a6e3a1",
    run: async (sessionId, log) => {
      log("Setting up fresh start...");
      await emitTerminalOutput(sessionId, "\x1b[2J\x1b[H"); // Clear screen
      await emitTerminalOutput(sessionId, "Welcome to Qbit Terminal!\r\n");
      await emitTerminalOutput(sessionId, "$ ");
      log("Fresh start complete");
    },
  },
  {
    id: "active-conversation",
    name: "Active Conversation",
    description: "AI helping with a coding task",
    icon: "💬",
    color: "#89b4fa",
    run: async (sessionId, log) => {
      log("Simulating active conversation...");

      // First, show some terminal activity
      await simulateCommand(
        sessionId,
        "cat src/main.rs",
        'fn main() {\n    println!("Hello, world!");\n}'
      );
      await delay(300);

      // Simulate AI response
      await emitAiEvent({ type: "started", turn_id: "turn-1" });
      await delay(100);

      const response = "I can see you have a basic Rust project. Let me help you add error handling and improve the structure. I'll read the Cargo.toml first to understand your dependencies.";
      const words = response.split(" ");
      let accumulated = "";
      for (const word of words) {
        accumulated += (accumulated ? " " : "") + word;
        await emitAiEvent({ type: "text_delta", delta: word + " ", accumulated });
        await delay(30);
      }

      await emitAiEvent({
        type: "completed",
        response: accumulated,
        tokens_used: 150,
        duration_ms: 2500,
      });

      log("Active conversation setup complete");
    },
  },
  {
    id: "tool-execution",
    name: "Tool Execution",
    description: "AI using tools to read files and run commands",
    icon: "🔧",
    color: "#f9e2af",
    run: async (_sessionId, log) => {
      log("Simulating tool execution...");

      // Start AI turn
      await emitAiEvent({ type: "started", turn_id: "turn-tools" });
      await delay(100);

      // Text before tool
      const preText = "Let me read the configuration file to understand your setup.";
      await emitAiEvent({ type: "text_delta", delta: preText, accumulated: preText });
      await delay(200);

      // Tool request
      await emitAiEvent({
        type: "tool_request",
        tool_name: "read_file",
        args: { path: "/home/user/project/config.toml" },
        request_id: "req-1",
      });
      await delay(500);

      // Tool result
      await emitAiEvent({
        type: "tool_result",
        tool_name: "read_file",
        result: '[package]\nname = "my-app"\nversion = "0.1.0"\nedition = "2021"',
        success: true,
        request_id: "req-1",
      });
      await delay(200);

      // Continue with analysis
      const postText = " I see you're using Rust 2021 edition. Let me also check your source files.";
      await emitAiEvent({
        type: "text_delta",
        delta: postText,
        accumulated: preText + postText,
      });
      await delay(200);

      // Another tool
      await emitAiEvent({
        type: "tool_request",
        tool_name: "glob",
        args: { pattern: "src/**/*.rs" },
        request_id: "req-2",
      });
      await delay(300);

      await emitAiEvent({
        type: "tool_result",
        tool_name: "glob",
        result: ["src/main.rs", "src/lib.rs", "src/utils/mod.rs"],
        success: true,
        request_id: "req-2",
      });
      await delay(200);

      await emitAiEvent({
        type: "completed",
        response: preText + postText,
        tokens_used: 320,
        duration_ms: 4200,
      });

      log("Tool execution demo complete");
    },
  },
  {
    id: "error-state",
    name: "Error State",
    description: "Show error handling UI",
    icon: "❌",
    color: "#f38ba8",
    run: async (_sessionId, log) => {
      log("Triggering error state...");

      await emitAiEvent({ type: "started", turn_id: "turn-error" });
      await delay(200);

      await emitAiEvent({
        type: "text_delta",
        delta: "Let me try to ",
        accumulated: "Let me try to ",
      });
      await delay(300);

      await emitAiEvent({
        type: "error",
        message: "Rate limit exceeded. Please wait 30 seconds before retrying.",
        error_type: "RateLimitError",
      });

      log("Error state triggered");
    },
  },
  {
    id: "command-history",
    name: "Command History",
    description: "Series of terminal commands with outputs",
    icon: "📜",
    color: "#cba6f7",
    run: async (sessionId, log) => {
      log("Building command history...");

      const commands = [
        {
          cmd: "git status",
          output: "On branch main\nYour branch is up to date with 'origin/main'.\n\nChanges not staged for commit:\n  modified:   src/main.rs\n  modified:   Cargo.toml",
          exitCode: 0,
        },
        {
          cmd: "cargo build",
          output: "   Compiling my-app v0.1.0 (/home/user/project)\n    Finished dev [unoptimized + debuginfo] target(s) in 2.34s",
          exitCode: 0,
        },
        {
          cmd: "cargo test",
          output: "running 3 tests\ntest tests::test_add ... ok\ntest tests::test_subtract ... ok\ntest tests::test_multiply ... ok\n\ntest result: ok. 3 passed; 0 failed",
          exitCode: 0,
        },
        {
          cmd: "ls -la",
          output: "total 24\ndrwxr-xr-x  5 user user  160 Jan 15 10:00 .\ndrwxr-xr-x 10 user user  320 Jan 15 09:00 ..\n-rw-r--r--  1 user user  234 Jan 15 10:00 Cargo.toml\ndrwxr-xr-x  2 user user   64 Jan 15 09:30 src\ndrwxr-xr-x  3 user user   96 Jan 15 10:00 target",
          exitCode: 0,
        },
      ];

      for (const { cmd, output, exitCode } of commands) {
        await simulateCommand(sessionId, cmd, output, exitCode);
        await delay(400);
      }

      log("Command history built");
    },
  },
  {
    id: "build-failure",
    name: "Build Failure",
    description: "Failed build with compiler errors",
    icon: "🔴",
    color: "#f38ba8",
    run: async (sessionId, log) => {
      log("Simulating build failure...");

      await simulateCommand(
        sessionId,
        "cargo build",
        `   Compiling my-app v0.1.0 (/home/user/project)
error[E0382]: borrow of moved value: \`data\`
  --> src/main.rs:15:20
   |
10 |     let data = vec![1, 2, 3];
   |         ---- move occurs because \`data\` has type \`Vec<i32>\`
...
13 |     process(data);
   |             ---- value moved here
14 |
15 |     println!("{:?}", data);
   |                      ^^^^ value borrowed here after move
   |
help: consider cloning the value
   |
13 |     process(data.clone());
   |                 ++++++++

error: aborting due to previous error

For more information about this error, try \`rustc --explain E0382\`.
error: could not compile \`my-app\` due to previous error`,
        1  // exit code 1 for failure
      );

      await delay(500);

      // AI offers help
      await emitAiEvent({ type: "started", turn_id: "turn-help" });
      await delay(100);

      const response = "I see a borrow checker error. The issue is that `data` was moved into `process()` and then you tried to use it again. You have two options:\n\n1. Clone the data before passing it\n2. Pass a reference instead of moving ownership\n\nWould you like me to fix this for you?";
      const words = response.split(" ");
      let accumulated = "";
      for (const word of words) {
        accumulated += (accumulated ? " " : "") + word;
        await emitAiEvent({ type: "text_delta", delta: word + " ", accumulated });
        await delay(25);
      }

      await emitAiEvent({
        type: "completed",
        response: accumulated,
        tokens_used: 180,
        duration_ms: 3000,
      });

      log("Build failure scenario complete");
    },
  },
  {
    id: "code-review",
    name: "Code Review",
    description: "AI reviewing code with suggestions",
    icon: "👀",
    color: "#94e2d5",
    run: async (sessionId, log) => {
      log("Starting code review scenario...");

      // Show the file being reviewed
      await simulateCommand(
        sessionId,
        "cat src/handlers.rs",
        `pub fn handle_request(req: Request) -> Response {
    let data = req.body;
    let result = process_data(data);
    if result.is_err() {
        return Response::error(500);
    }
    Response::ok(result.unwrap())
}`
      );
      await delay(400);

      // AI review
      await emitAiEvent({ type: "started", turn_id: "turn-review" });
      await delay(100);

      const review = `## Code Review: src/handlers.rs

**Issues Found:**

1. **🔴 Critical:** Using \`.unwrap()\` after checking \`.is_err()\` is an anti-pattern. Use \`match\` or \`?\` operator instead.

2. **🟡 Warning:** No error context is provided in the 500 response. Consider logging the error.

3. **🟢 Suggestion:** The function could be more idiomatic using the \`?\` operator.

**Suggested Fix:**

\`\`\`rust
pub fn handle_request(req: Request) -> Result<Response, Error> {
    let data = req.body;
    let result = process_data(data)?;
    Ok(Response::ok(result))
}
\`\`\`

Would you like me to apply these changes?`;

      const words = review.split(" ");
      let accumulated = "";
      for (const word of words) {
        accumulated += (accumulated ? " " : "") + word;
        await emitAiEvent({ type: "text_delta", delta: word + " ", accumulated });
        await delay(20);
      }

      await emitAiEvent({
        type: "completed",
        response: accumulated,
        tokens_used: 450,
        duration_ms: 5500,
      });

      log("Code review complete");
    },
  },
  {
    id: "long-output",
    name: "Long Output",
    description: "Test scrolling with lots of content",
    icon: "📄",
    color: "#fab387",
    run: async (sessionId, log) => {
      log("Generating long output...");

      // Generate a long test output
      let testOutput = "running 50 tests\n";
      for (let i = 1; i <= 50; i++) {
        testOutput += `test tests::test_case_${i.toString().padStart(2, "0")} ... ok\n`;
      }
      testOutput += "\ntest result: ok. 50 passed; 0 failed; 0 ignored\n";
      testOutput += "\n   Doc-tests my-app\n\nrunning 12 doc tests\n";
      for (let i = 1; i <= 12; i++) {
        testOutput += `test src/lib.rs - example_${i} (line ${i * 10}) ... ok\n`;
      }
      testOutput += "\ntest result: ok. 12 passed; 0 failed\n";

      await simulateCommand(sessionId, "cargo test", testOutput);

      log("Long output generated");
    },
  },
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
  presetCard: {
    display: "flex",
    alignItems: "flex-start",
    gap: "12px",
    padding: "12px",
    backgroundColor: "#313244",
    borderRadius: "8px",
    marginBottom: "8px",
    cursor: "pointer",
    transition: "all 0.2s",
    border: "1px solid transparent",
  },
  presetCardHover: {
    borderColor: "#45475a",
    backgroundColor: "#3b3d52",
  },
  presetIcon: {
    fontSize: "24px",
    width: "32px",
    height: "32px",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    borderRadius: "6px",
    flexShrink: 0,
  },
  presetContent: {
    flex: 1,
    minWidth: 0,
  },
  presetName: {
    fontSize: "13px",
    fontWeight: 600,
    color: "#cdd6f4",
    marginBottom: "2px",
  },
  presetDescription: {
    fontSize: "11px",
    color: "#a6adc8",
    lineHeight: 1.4,
  },
  presetRunning: {
    opacity: 0.6,
    pointerEvents: "none" as const,
  },
};

// =============================================================================
// Component
// =============================================================================

export function MockDevTools() {
  const [isOpen, setIsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<TabId>("presets");
  const [lastAction, setLastAction] = useState<string>("");
  const [runningPreset, setRunningPreset] = useState<string | null>(null);
  const [hoveredPreset, setHoveredPreset] = useState<string | null>(null);

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
    await simulateCommand(sessionId, command, commandOutput, exitCode);
    logAction(`Emitted command block: ${command}`);
  }, [sessionId, command, commandOutput, exitCode, logAction]);

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

  // Preset handlers
  const handleRunPreset = useCallback(
    async (preset: Preset) => {
      if (runningPreset) return;
      setRunningPreset(preset.id);
      try {
        await preset.run(sessionId, logAction);
      } catch (error) {
        logAction(`Error running preset: ${error}`);
      } finally {
        setRunningPreset(null);
      }
    },
    [sessionId, logAction, runningPreset]
  );

  // Render tab content
  const renderTabContent = () => {
    switch (activeTab) {
      case "presets":
        return (
          <>
            <div style={styles.section}>
              <div style={styles.sectionTitle}>Scenarios</div>
              <div style={styles.inputGroup}>
                <label style={styles.label}>Target Session ID</label>
                <input
                  type="text"
                  style={styles.input}
                  value={sessionId}
                  onChange={(e) => setSessionId(e.target.value)}
                />
              </div>
            </div>

            <div style={styles.section}>
              {PRESETS.map((preset) => (
                <div
                  key={preset.id}
                  style={{
                    ...styles.presetCard,
                    ...(hoveredPreset === preset.id ? styles.presetCardHover : {}),
                    ...(runningPreset ? styles.presetRunning : {}),
                  }}
                  onMouseEnter={() => setHoveredPreset(preset.id)}
                  onMouseLeave={() => setHoveredPreset(null)}
                  onClick={() => handleRunPreset(preset)}
                >
                  <div
                    style={{
                      ...styles.presetIcon,
                      backgroundColor: `${preset.color}20`,
                    }}
                  >
                    {runningPreset === preset.id ? "⏳" : preset.icon}
                  </div>
                  <div style={styles.presetContent}>
                    <div style={styles.presetName}>{preset.name}</div>
                    <div style={styles.presetDescription}>{preset.description}</div>
                  </div>
                </div>
              ))}
            </div>
          </>
        );

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
