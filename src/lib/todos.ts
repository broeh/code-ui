import type { Step } from "../ipc/types";
export type TodoState = "pending" | "active" | "done";
export interface Todo {
  text: string;
  state: TodoState;
}
const states: Record<string, TodoState> = {
  pending: "pending",
  in_progress: "active",
  completed: "done",
};
// Muse sends {text, status}; Claude's TodoWrite sends {content, status}.
export function latestTodos(steps: Step[]): Todo[] | null {
  for (let i = steps.length - 1; i >= 0; i--) {
    const step = steps[i];
    if (step.kind !== "todo") continue;
    const input = step.detail.input as { todos?: unknown } | undefined;
    if (!Array.isArray(input?.todos)) continue;
    return input.todos.map(
      (item: { text?: string; content?: string; status?: string }) => ({
        text: String(item.text ?? item.content ?? ""),
        state: states[item.status ?? ""] ?? "pending",
      }),
    );
  }
  return null;
}
