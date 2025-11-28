import { CommandBlock } from "@/components/CommandBlock/CommandBlock";
import { AgentMessage } from "@/components/AgentChat/AgentMessage";
import { useStore } from "@/store";
import type { UnifiedBlock as UnifiedBlockType } from "@/store";

interface UnifiedBlockProps {
  block: UnifiedBlockType;
}

export function UnifiedBlock({ block }: UnifiedBlockProps) {
  const toggleBlockCollapse = useStore((state) => state.toggleBlockCollapse);

  switch (block.type) {
    case "command":
      return (
        <CommandBlock
          block={block.data}
          onToggleCollapse={toggleBlockCollapse}
        />
      );

    case "agent_message":
      return <AgentMessage message={block.data} />;

    case "agent_streaming":
      // This shouldn't appear in the timeline as streaming is handled separately
      // but we include it for completeness
      return null;

    default:
      return null;
  }
}
