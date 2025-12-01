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
