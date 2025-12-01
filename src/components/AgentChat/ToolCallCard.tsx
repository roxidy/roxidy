/**
 * ToolCallCard - Displays a tool call in finalized agent messages.
 *
 * This is a thin wrapper around ToolItem from ToolCallDisplay for
 * backwards compatibility and semantic clarity in message contexts.
 */
import { ToolItem } from "@/components/ToolCallDisplay";
import type { ToolCall } from "@/store";

interface ToolCallCardProps {
  tool: ToolCall;
}

export function ToolCallCard({ tool }: ToolCallCardProps) {
  return <ToolItem tool={tool} />;
}
