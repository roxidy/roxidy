/**
 * Shared utilities for tool call display components.
 */

/** Base properties shared by all tool call types */
export interface BaseToolCall {
  name: string;
  executedByAgent?: boolean;
}

/** Check if a tool call is a terminal command executed by the agent */
export function isAgentTerminalCommand(tool: BaseToolCall): boolean {
  return (tool.name === "run_pty_cmd" || tool.name === "shell") && tool.executedByAgent === true;
}

/** Format tool name for display (e.g., "read_file" -> "Read File") */
export function formatToolName(name: string): string {
  return name
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** Format result for display */
export function formatToolResult(result: unknown): string {
  if (typeof result === "string") {
    return result;
  }
  return JSON.stringify(result, null, 2);
}

/** Risk level for tool operations */
export type RiskLevel = "low" | "medium" | "high" | "critical";

/** Read-only tools that pose minimal risk */
const READ_ONLY_TOOLS = [
  "read_file",
  "grep_file",
  "list_files",
  "indexer_search_code",
  "indexer_search_files",
  "indexer_analyze_file",
  "indexer_extract_symbols",
  "indexer_get_metrics",
  "indexer_detect_language",
  "debug_agent",
  "analyze_agent",
  "get_errors",
  "list_skills",
  "search_skills",
  "load_skill",
  "search_tools",
  "update_plan",
  "web_fetch",
];

/** Write operations that are recoverable */
const WRITE_TOOLS = ["write_file", "create_file", "edit_file", "apply_patch", "save_skill"];

/** Shell execution tools */
const SHELL_TOOLS = ["run_pty_cmd", "create_pty_session", "send_pty_input"];

/** Destructive operations */
const DESTRUCTIVE_TOOLS = ["delete_file", "execute_code"];

/** Tools that can modify files or execute code (dangerous operations) */
export const DANGEROUS_TOOLS = [
  "write_file",
  "edit_file",
  "apply_patch",
  "run_pty_cmd",
  "shell",
  "execute_code",
  "delete_file",
];

/** Get the risk level for a tool based on its name */
export function getRiskLevel(toolName: string): RiskLevel {
  if (READ_ONLY_TOOLS.includes(toolName)) {
    return "low";
  }
  if (WRITE_TOOLS.includes(toolName)) {
    return "medium";
  }
  if (SHELL_TOOLS.includes(toolName)) {
    return "high";
  }
  if (DESTRUCTIVE_TOOLS.includes(toolName)) {
    return "critical";
  }
  // Sub-agents are medium risk
  if (toolName.startsWith("sub_agent_")) {
    return "medium";
  }
  // Default for unknown tools
  return "high";
}

/** Check if a tool is considered dangerous */
export function isDangerousTool(toolName: string, riskLevel?: RiskLevel): boolean {
  const level = riskLevel ?? getRiskLevel(toolName);
  return DANGEROUS_TOOLS.includes(toolName) || level === "high" || level === "critical";
}
