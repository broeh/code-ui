import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
export const mockEvents = new EventTarget();
export async function subscribe<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (isTauri()) return listen<T>(name, ({ payload }) => handler(payload));
  const listener = (event: Event) => handler((event as CustomEvent<T>).detail);
  mockEvents.addEventListener(name, listener);
  return () => mockEvents.removeEventListener(name, listener);
}
export function mockEmit(name: string, payload: unknown) {
  mockEvents.dispatchEvent(new CustomEvent(name, { detail: payload }));
}
