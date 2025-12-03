import type { ActiveToolCall, FinalizedStreamingBlock, StreamingBlock, ToolCall } from "@/store";

/** Union type for both finalized and active tool calls */
export type AnyToolCall = ToolCall | ActiveToolCall;

/** Input block type - works with both streaming and finalized blocks */
type InputBlock = StreamingBlock | FinalizedStreamingBlock;

/** A group of consecutive tool calls of the same type */
export interface ToolGroup {
  type: "tool_group";
  toolName: string;
  tools: AnyToolCall[];
}

/** Grouped streaming block - either text, single tool, or tool group */
export type GroupedStreamingBlock =
  | { type: "text"; content: string }
  | { type: "tool"; toolCall: AnyToolCall }
  | ToolGroup;

/**
 * Groups consecutive tool calls of the same type.
 * Text blocks pass through unchanged and break tool grouping.
 * Single tools are kept as-is, 2+ consecutive same tools become a group.
 * Works with both StreamingBlock[] and FinalizedStreamingBlock[].
 */
export function groupConsecutiveTools(blocks: InputBlock[]): GroupedStreamingBlock[] {
  const result: GroupedStreamingBlock[] = [];
  let currentGroup: AnyToolCall[] = [];
  let currentToolName: string | null = null;

  const flushGroup = () => {
    if (currentGroup.length === 0) return;

    if (currentGroup.length === 1) {
      // Single tool - keep as individual
      result.push({ type: "tool", toolCall: currentGroup[0] });
    } else if (currentToolName) {
      // Multiple tools - create group
      result.push({
        type: "tool_group",
        toolName: currentToolName,
        tools: [...currentGroup],
      });
    }
    currentGroup = [];
    currentToolName = null;
  };

  for (const block of blocks) {
    if (block.type === "text") {
      // Text breaks any current group
      flushGroup();
      result.push(block);
    } else {
      // Tool block
      const tool = block.toolCall;

      if (currentToolName === null) {
        // Start new potential group
        currentToolName = tool.name;
        currentGroup.push(tool);
      } else if (tool.name === currentToolName) {
        // Same tool type - add to group
        currentGroup.push(tool);
      } else {
        // Different tool type - flush and start new
        flushGroup();
        currentToolName = tool.name;
        currentGroup.push(tool);
      }
    }
  }

  // Flush any remaining group
  flushGroup();

  return result;
}

/**
 * Primary argument mapping for each tool type.
 * Returns the key name to extract from args for inline display.
 */
const primaryArgKeys: Record<string, string> = {
  read_file: "path",
  write_file: "path",
  edit_file: "path",
  list_files: "path",
  grep_file: "pattern",
  run_pty_cmd: "command",
  shell: "command",
  web_fetch: "url",
  web_search: "query",
  web_search_answer: "query",
  apply_patch: "path",
};

/**
 * Extracts the primary argument from a tool call for inline display.
 * Returns null if no primary arg is defined or found.
 */
export function getPrimaryArgument(tool: AnyToolCall): string | null {
  const key = primaryArgKeys[tool.name];
  if (!key) return null;

  const value = tool.args[key];
  if (typeof value !== "string") return null;

  return value;
}

/**
 * Formats a primary argument for display.
 * - For file paths: extracts basename
 * - For patterns/queries: truncates with quotes
 * - For URLs: extracts domain
 * - For commands: first word + truncate
 */
export function formatPrimaryArg(tool: AnyToolCall, maxLength = 30): string | null {
  const value = getPrimaryArgument(tool);
  if (!value) return null;

  const toolName = tool.name;

  // File paths - extract basename
  if (["read_file", "write_file", "edit_file", "apply_patch"].includes(toolName)) {
    const parts = value.split("/");
    return parts[parts.length - 1];
  }

  // Directory paths - show last segment with trailing slash
  if (toolName === "list_files") {
    const parts = value.replace(/\/$/, "").split("/");
    const last = parts[parts.length - 1];
    return last ? `${last}/` : value;
  }

  // Patterns/queries - wrap in quotes, truncate
  if (["grep_file", "web_search", "web_search_answer"].includes(toolName)) {
    const truncated = value.length > maxLength ? `${value.slice(0, maxLength - 3)}...` : value;
    return `"${truncated}"`;
  }

  // URLs - extract domain
  if (toolName === "web_fetch") {
    try {
      const url = new URL(value);
      return url.hostname;
    } catch {
      return value.slice(0, maxLength);
    }
  }

  // Commands - truncate
  if (["run_pty_cmd", "shell"].includes(toolName)) {
    return value.length > maxLength ? `${value.slice(0, maxLength - 3)}...` : value;
  }

  return value.slice(0, maxLength);
}

/**
 * Gets the aggregate status for a group of tools.
 * - If any running → "running"
 * - If any error → "error"
 * - If all completed → "completed"
 * - Otherwise → first tool's status
 */
export function getGroupStatus(tools: AnyToolCall[]): AnyToolCall["status"] {
  const hasRunning = tools.some((t) => t.status === "running");
  if (hasRunning) return "running";

  const hasError = tools.some((t) => t.status === "error");
  if (hasError) return "error";

  const allCompleted = tools.every((t) => t.status === "completed");
  if (allCompleted) return "completed";

  return tools[0]?.status ?? "pending";
}
