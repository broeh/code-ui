import { expect, it } from "vitest";
import { availableAgent } from "./agents";
import type { AgentKind, AgentStatus } from "../ipc/types";

const agent = (kind: AgentKind, installed = true, enabled = true): AgentStatus => ({
  agent: kind, installed, enabled, problem: null, path: installed ? `/bin/${kind}` : null, version: null,
});

it("uses the installed enabled agent on Claude-only and Codex-only machines", () => {
  expect(availableAgent([agent("claude"), agent("codex", false)], "codex")).toBe("claude");
  expect(availableAgent([agent("claude", false), agent("codex")], "claude")).toBe("codex");
  expect(availableAgent([agent("claude", true, false), agent("codex")], "claude")).toBe("codex");
  expect(availableAgent([], "claude")).toBe("claude");
});
