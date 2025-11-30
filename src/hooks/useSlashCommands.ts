import { useCallback, useEffect, useState } from "react";
import { listPrompts, type PromptInfo } from "@/lib/tauri";

export function useSlashCommands(workingDirectory?: string) {
  const [prompts, setPrompts] = useState<PromptInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const loadPrompts = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await listPrompts(workingDirectory);
      setPrompts(result);
    } catch (error) {
      console.error("Failed to load prompts:", error);
      setPrompts([]);
    } finally {
      setIsLoading(false);
    }
  }, [workingDirectory]);

  // Load prompts on mount and when working directory changes
  useEffect(() => {
    loadPrompts();
  }, [loadPrompts]);

  return { prompts, isLoading, reload: loadPrompts };
}
