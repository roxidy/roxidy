import { useCallback, useState } from "react";

interface UseCommandHistoryReturn {
  /** Current history array (readonly) */
  history: readonly string[];
  /** Add a command to history */
  add: (command: string) => void;
  /** Navigate up in history, returns the command or null if at end */
  navigateUp: () => string | null;
  /** Navigate down in history, returns the command or empty string if at beginning */
  navigateDown: () => string;
  /** Reset navigation index (call when user edits input manually) */
  reset: () => void;
  /** Current history index (-1 means not navigating) */
  index: number;
}

/**
 * Hook for managing command history with up/down navigation.
 *
 * @param initialHistory - Optional initial history array
 * @returns History management functions
 *
 * @example
 * ```tsx
 * const { add, navigateUp, navigateDown, reset } = useCommandHistory();
 *
 * // On submit
 * add(input);
 *
 * // On ArrowUp
 * const cmd = navigateUp();
 * if (cmd !== null) setInput(cmd);
 *
 * // On ArrowDown
 * setInput(navigateDown());
 *
 * // On manual input change
 * reset();
 * ```
 */
export function useCommandHistory(initialHistory: string[] = []): UseCommandHistoryReturn {
  const [history, setHistory] = useState<string[]>(initialHistory);
  const [index, setIndex] = useState(-1);

  const add = useCallback((command: string) => {
    if (!command.trim()) return;
    setHistory((prev) => [...prev, command]);
    setIndex(-1);
  }, []);

  const navigateUp = useCallback((): string | null => {
    if (history.length === 0) return null;

    const newIndex = index < history.length - 1 ? index + 1 : index;
    setIndex(newIndex);
    return history[history.length - 1 - newIndex] ?? null;
  }, [history, index]);

  const navigateDown = useCallback((): string => {
    if (index > 0) {
      const newIndex = index - 1;
      setIndex(newIndex);
      return history[history.length - 1 - newIndex] ?? "";
    }
    setIndex(-1);
    return "";
  }, [history, index]);

  const reset = useCallback(() => {
    setIndex(-1);
  }, []);

  return {
    history,
    add,
    navigateUp,
    navigateDown,
    reset,
    index,
  };
}
