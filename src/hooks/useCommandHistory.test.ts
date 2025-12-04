import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { InputMode } from "@/store";
import { useCommandHistory } from "./useCommandHistory";

describe("useCommandHistory", () => {
  describe("initialization", () => {
    it("should initialize with empty history", () => {
      const { result } = renderHook(() => useCommandHistory());

      expect(result.current.history).toEqual([]);
      expect(result.current.index).toBe(-1);
    });

    it("should initialize with provided initial history", () => {
      const initialHistory = [
        { command: "ls -la", mode: "terminal" as InputMode },
        { command: "cd /home", mode: "terminal" as InputMode },
        { command: "explain this code", mode: "agent" as InputMode },
      ];

      const { result } = renderHook(() => useCommandHistory(initialHistory));

      expect(result.current.history).toEqual(initialHistory);
      expect(result.current.index).toBe(-1);
    });
  });

  describe("add", () => {
    it("should add command with mode to history", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("echo hello", "terminal");
      });

      expect(result.current.history).toHaveLength(1);
      expect(result.current.history[0]).toEqual({
        command: "echo hello",
        mode: "terminal",
      });
    });

    it("should add multiple commands with different modes", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("ls -la", "terminal");
        result.current.add("what is this file?", "agent");
        result.current.add("npm install", "terminal");
      });

      expect(result.current.history).toHaveLength(3);
      expect(result.current.history[0]).toEqual({ command: "ls -la", mode: "terminal" });
      expect(result.current.history[1]).toEqual({
        command: "what is this file?",
        mode: "agent",
      });
      expect(result.current.history[2]).toEqual({ command: "npm install", mode: "terminal" });
    });

    it("should not add empty commands", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("", "terminal");
        result.current.add("   ", "agent");
        result.current.add("valid command", "terminal");
      });

      expect(result.current.history).toHaveLength(1);
      expect(result.current.history[0].command).toBe("valid command");
    });

    it("should reset index to -1 after adding command", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "terminal");
      });

      act(() => {
        result.current.navigateUp();
      });

      expect(result.current.index).toBe(0);

      act(() => {
        result.current.add("third", "agent");
      });

      expect(result.current.index).toBe(-1);
    });

    it("should preserve order of commands as they are added", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
        result.current.add("third", "terminal");
        result.current.add("fourth", "agent");
      });

      expect(result.current.history.map((e) => e.command)).toEqual([
        "first",
        "second",
        "third",
        "fourth",
      ]);
    });
  });

  describe("navigateUp", () => {
    it("should return null when history is empty", () => {
      const { result } = renderHook(() => useCommandHistory());

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry).toBeNull();
      expect(result.current.index).toBe(-1);
    });

    it("should navigate to most recent command on first up", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
        result.current.add("third", "terminal");
      });

      const entry = result.current.navigateUp();

      expect(entry).toEqual({ command: "third", mode: "terminal" });
      expect(result.current.index).toBe(0);
    });

    it("should navigate backwards through history", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
        result.current.add("third", "terminal");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry).toEqual({ command: "third", mode: "terminal" });

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry).toEqual({ command: "second", mode: "agent" });

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry).toEqual({ command: "first", mode: "terminal" });
    });

    it("should stay at oldest command when navigating past beginning", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
      });

      act(() => {
        result.current.navigateUp();
        result.current.navigateUp();
      });

      const indexBeforeExtra = result.current.index;

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry).toEqual({ command: "first", mode: "terminal" });
      expect(result.current.index).toBe(indexBeforeExtra);
    });

    it("should preserve mode information during navigation", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("terminal command", "terminal");
        result.current.add("agent command", "agent");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.mode).toBe("agent");

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.mode).toBe("terminal");
    });
  });

  describe("navigateDown", () => {
    it("should return null when not navigating", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("command", "terminal");
      });

      let entry: ReturnType<typeof result.current.navigateDown> | undefined;
      act(() => {
        entry = result.current.navigateDown();
      });

      expect(entry).toBeNull();
      expect(result.current.index).toBe(-1);
    });

    it("should navigate forward through history", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
        result.current.add("third", "terminal");
      });

      act(() => {
        result.current.navigateUp();
        result.current.navigateUp();
        result.current.navigateUp();
      });

      let entry: ReturnType<typeof result.current.navigateDown> | undefined;

      act(() => {
        entry = result.current.navigateDown();
      });
      expect(entry).toEqual({ command: "second", mode: "agent" });

      act(() => {
        entry = result.current.navigateDown();
      });
      expect(entry).toEqual({ command: "third", mode: "terminal" });
    });

    it("should return null when reaching current position", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
      });

      act(() => {
        result.current.navigateUp();
      });

      let entry: ReturnType<typeof result.current.navigateDown> | undefined;
      act(() => {
        entry = result.current.navigateDown();
      });

      expect(entry).toBeNull();
      expect(result.current.index).toBe(-1);
    });

    it("should reset index to -1 when reaching end", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("command", "terminal");
      });

      act(() => {
        result.current.navigateUp();
      });
      expect(result.current.index).toBe(0);

      act(() => {
        result.current.navigateDown();
      });
      expect(result.current.index).toBe(-1);
    });

    it("should preserve mode information during navigation", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("terminal command", "terminal");
        result.current.add("agent command", "agent");
      });

      act(() => {
        result.current.navigateUp();
        result.current.navigateUp();
      });

      let entry: ReturnType<typeof result.current.navigateDown> | undefined;

      act(() => {
        entry = result.current.navigateDown();
      });
      expect(entry?.mode).toBe("agent");
    });
  });

  describe("reset", () => {
    it("should reset index to -1", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
      });

      act(() => {
        result.current.navigateUp();
      });

      expect(result.current.index).toBe(0);

      act(() => {
        result.current.reset();
      });

      expect(result.current.index).toBe(-1);
    });

    it("should not affect history content", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
      });

      const historyBefore = [...result.current.history];

      act(() => {
        result.current.navigateUp();
        result.current.reset();
      });

      expect(result.current.history).toEqual(historyBefore);
    });
  });

  describe("full navigation workflow", () => {
    it("should handle complete up-down-up navigation cycle", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("ls", "terminal");
        result.current.add("pwd", "terminal");
        result.current.add("help me", "agent");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.command).toBe("help me");

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.command).toBe("pwd");

      act(() => {
        entry = result.current.navigateDown();
      });
      expect(entry?.command).toBe("help me");

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.command).toBe("pwd");
    });

    it("should handle navigation after adding new command", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("first", "terminal");
        result.current.add("second", "agent");
      });

      act(() => {
        result.current.navigateUp();
      });

      act(() => {
        result.current.add("third", "terminal");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;

      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry?.command).toBe("third");
      expect(entry?.mode).toBe("terminal");
    });

    it("should maintain correct state through multiple operations", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("cmd1", "terminal");
        result.current.add("cmd2", "agent");
        result.current.add("cmd3", "terminal");
      });

      act(() => {
        result.current.navigateUp();
        result.current.navigateUp();
      });

      act(() => {
        result.current.reset();
      });

      act(() => {
        result.current.add("cmd4", "agent");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry?.command).toBe("cmd4");
      expect(result.current.history).toHaveLength(4);
    });
  });

  describe("edge cases", () => {
    it("should handle single command history", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("only-command", "terminal");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.command).toBe("only-command");

      act(() => {
        entry = result.current.navigateUp();
      });
      expect(entry?.command).toBe("only-command");

      act(() => {
        entry = result.current.navigateDown();
      });
      expect(entry).toBeNull();
    });

    it("should handle rapid navigation changes", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("a", "terminal");
        result.current.add("b", "agent");
        result.current.add("c", "terminal");
      });

      act(() => {
        result.current.navigateUp();
        result.current.navigateDown();
        result.current.navigateUp();
        result.current.navigateDown();
        result.current.navigateUp();
      });

      expect(result.current.index).toBe(0);
    });

    it("should handle mixed mode sequences", () => {
      const { result } = renderHook(() => useCommandHistory());

      const modes: InputMode[] = ["terminal", "agent", "terminal", "agent", "terminal"];
      act(() => {
        modes.forEach((mode, i) => {
          result.current.add(`cmd${i}`, mode);
        });
      });

      const entries: (ReturnType<typeof result.current.navigateUp> | null)[] = [];
      act(() => {
        for (let i = 0; i < modes.length; i++) {
          entries.push(result.current.navigateUp());
        }
      });

      entries.reverse().forEach((entry, i) => {
        expect(entry?.mode).toBe(modes[i]);
      });
    });

    it("should handle commands with special characters", () => {
      const { result } = renderHook(() => useCommandHistory());

      const specialCommands = [
        'echo "hello world"',
        "grep -r 'pattern' .",
        "find . -name '*.ts'",
        "awk '{print $1}'",
      ];

      act(() => {
        specialCommands.forEach((cmd) => {
          result.current.add(cmd, "terminal");
        });
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry?.command).toBe(specialCommands[specialCommands.length - 1]);
    });

    it("should handle very long command strings", () => {
      const { result } = renderHook(() => useCommandHistory());

      const longCommand = "a".repeat(1000);

      act(() => {
        result.current.add(longCommand, "terminal");
      });

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry?.command).toBe(longCommand);
      expect(entry?.command.length).toBe(1000);
    });

    it("should handle rapid command additions", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        for (let i = 0; i < 100; i++) {
          result.current.add(`command-${i}`, i % 2 === 0 ? "terminal" : "agent");
        }
      });

      expect(result.current.history).toHaveLength(100);

      let entry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        entry = result.current.navigateUp();
      });

      expect(entry?.command).toBe("command-99");
    });
  });

  describe("history immutability", () => {
    it("should return readonly history array", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("test", "terminal");
      });

      const history = result.current.history;

      expect(() => {
        // @ts-expect-error - testing runtime immutability
        history.push({ command: "should not work", mode: "terminal" });
      }).toThrow();
    });

    it("should not allow direct modification of history entries", () => {
      const { result } = renderHook(() => useCommandHistory());

      act(() => {
        result.current.add("original", "terminal");
      });

      const entry = result.current.history[0];
      const originalCommand = entry.command;

      entry.command = "modified";

      let retrievedEntry: ReturnType<typeof result.current.navigateUp> | undefined;
      act(() => {
        retrievedEntry = result.current.navigateUp();
      });

      expect(retrievedEntry?.command).toBe(originalCommand);
    });
  });
});
