import { describe, expect, it } from "vitest";
import {
  createTab,
  defaultSettings,
  projectTab,
  reorderTabs,
  savedTabs,
} from "./tabs";
describe("global tabs", () => {
  it("reorders without losing per-tab drafts or settings", () => {
    const a = { ...createTab("/a", defaultSettings), draft: "draft A" };
    const b = createTab("/b", { ...defaultSettings, permission: "ask" });
    const c = createTab("/a", defaultSettings);
    expect(reorderTabs([a, b, c], a.id, c.id).map((tab) => tab.id)).toEqual([
      b.id,
      c.id,
      a.id,
    ]);
    expect(reorderTabs([a, b, c], a.id, c.id)[2].draft).toBe("draft A");
    expect(savedTabs([a, b])[1].settings.permission).toBe("ask");
    expect(savedTabs([a])[0]).not.toHaveProperty("snapshot");
  });
  it("selects the most recently focused tab of a project", () => {
    const a = { ...createTab("/a", defaultSettings), lastFocusedAt: 1 };
    const b = { ...createTab("/a", defaultSettings), lastFocusedAt: 2 };
    expect(projectTab([a, b], "/a")?.id).toBe(b.id);
    expect(projectTab([a, b], "/missing")).toBeUndefined();
  });
});
