import { useSessionBlocks, useStore } from "@/store";
import { CommandBlock } from "./CommandBlock";

interface CommandBlockListProps {
  sessionId: string;
}

export function CommandBlockList({ sessionId }: CommandBlockListProps) {
  const blocks = useSessionBlocks(sessionId);
  const toggleBlockCollapse = useStore((state) => state.toggleBlockCollapse);

  if (blocks.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-col py-2">
      {blocks.map((block) => (
        <CommandBlock
          key={block.id}
          block={block}
          onToggleCollapse={toggleBlockCollapse}
        />
      ))}
    </div>
  );
}
