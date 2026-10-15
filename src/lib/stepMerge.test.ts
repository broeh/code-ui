import { describe, expect, it } from "vitest";
import {
  commentaryText,
  currentTool,
  mergeSteps,
  splitSteps,
} from "./stepMerge";
import type { Step } from "../ipc/types";
const step = (
  kind: Step["kind"],
  id: string,
  turnId = "turn",
  path = id,
): Step => ({
  id,
  turnId,
  kind,
  title: `Read ${path}`,
  status: "done",
  parentId: null,
  startedAt: 1,
  endedAt: 2,
  detail: { input: { file_path: path } },
});
describe("compact steps", () => {
  it("merges only consecutive related steps in one turn", () => {
    const groups = mergeSteps([
      step("read", "1"),
      step("read", "2"),
      step("command", "3"),
      step("command", "4"),
      step("read", "5", "next"),
    ]);
    expect(groups.map((g) => g.steps.length)).toEqual([2, 1, 1, 1]);
    expect(groups[0].title).toBe("Read 2 files");
  });
  it("counts distinct edited files and keeps a running group live", () => {
    const a = step("edit", "1", "turn", "a");
    const b = {
      ...step("edit", "2", "turn", "a"),
      detail: { input: { file_path: "a", old_string: "different edit" } },
      status: "running" as const,
    };
    expect(mergeSteps([a, b])[0].title).toBe("Editing 1 file");
  });
});
describe("turn steps", () => {
  it("separates tool calls from commentary and drops thinking", () => {
    const note = {
      ...step("other", "note"),
      detail: { input: { commentary: "Reading the road code." } },
    };
    const { tools, commentary } = splitSteps([
      step("thinking", "t1"),
      step("read", "r1"),
      note,
      step("thinking", "t2"),
      step("other", "mcp"),
    ]);
    expect(tools.map((s) => s.id)).toEqual(["r1", "mcp"]);
    expect(commentary.map((s) => s.id)).toEqual(["note"]);
    expect(commentaryText(note)).toBe("Reading the road code.");
    expect(commentaryText(step("read", "r1"))).toBe("");
  });
  it("names the running tool call, else the latest one", () => {
    const running = { ...step("command", "c2"), status: "running" as const };
    const done = [step("read", "r1"), step("read", "r2")];
    expect(currentTool(done)?.id).toBe("r2");
    expect(currentTool([done[0], running, done[1]])?.id).toBe("c2");
    expect(currentTool([])).toBeUndefined();
  });
});
