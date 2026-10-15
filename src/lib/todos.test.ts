import { describe, expect, it } from "vitest";
import { latestTodos } from "./todos";
import type { Step } from "../ipc/types";
const step = (kind: Step["kind"], input: unknown): Step => ({
  id: "1",
  turnId: "turn",
  kind,
  title: "Updating the task list",
  status: "done",
  parentId: null,
  startedAt: 1,
  endedAt: 2,
  detail: { input },
});
describe("task list", () => {
  it("uses the newest list and maps Muse statuses", () => {
    const todos = latestTodos([
      step("todo", { todos: [{ status: "pending", text: "Old" }] }),
      step("read", { file_path: "a" }),
      step("todo", {
        todos: [
          { status: "completed", text: "Read the log" },
          { status: "in_progress", text: "Switch branch" },
          { status: "pending", text: "Build" },
        ],
      }),
    ]);
    expect(todos).toEqual([
      { text: "Read the log", state: "done" },
      { text: "Switch branch", state: "active" },
      { text: "Build", state: "pending" },
    ]);
  });
  it("reads Claude's content field and ignores steps without a list", () => {
    expect(
      latestTodos([
        step("todo", { todos: [{ status: "in_progress", content: "Fix it" }] }),
        step("todo", { plan: "no list here" }),
      ]),
    ).toEqual([{ text: "Fix it", state: "active" }]);
    expect(latestTodos([step("read", {})])).toBeNull();
  });
});
