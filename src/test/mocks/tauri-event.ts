import { vi } from "vitest";

type EventCallback<T> = (event: { payload: T }) => void;
type UnlistenFn = () => void;

interface EventListener<T = unknown> {
  eventName: string;
  callback: EventCallback<T>;
}

// Store all registered listeners
const listeners: EventListener[] = [];

// Mock listen function
export async function listen<T>(
  eventName: string,
  callback: EventCallback<T>
): Promise<UnlistenFn> {
  const listener: EventListener<T> = { eventName, callback };
  listeners.push(listener as EventListener);

  // Return unlisten function
  return () => {
    const index = listeners.indexOf(listener as EventListener);
    if (index > -1) {
      listeners.splice(index, 1);
    }
  };
}

// Helper to emit events in tests
export function emitMockEvent<T>(eventName: string, payload: T): void {
  for (const listener of listeners) {
    if (listener.eventName === eventName) {
      listener.callback({ payload });
    }
  }
}

// Helper to clear all listeners (call in beforeEach/afterEach)
export function clearMockListeners(): void {
  listeners.length = 0;
}

// Helper to get listener count for an event
export function getListenerCount(eventName: string): number {
  return listeners.filter((l) => l.eventName === eventName).length;
}

// Export mock for vi.mock usage
export const mockListen = vi.fn(listen);

// Re-export type for type compatibility
export type { UnlistenFn };
