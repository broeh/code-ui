import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTab, defaultSettings } from "../lib/tabs";
import type { ChatSnapshot } from "../ipc/types";
vi.mock("../ipc/commands", () => ({
  setUiState: vi.fn().mockResolvedValue(undefined),
  scanNativeSessions: vi.fn().mockResolvedValue(undefined),
  openNativeSession: vi.fn(),
  touchProject: vi.fn().mockResolvedValue(undefined),
  markRead: vi.fn().mockResolvedValue(undefined),
  listChats: vi.fn().mockResolvedValue([]),
  listProjects: vi.fn().mockResolvedValue([]),
  openChat: vi.fn(),
  sendMessage: vi.fn(),
  closeChat: vi.fn().mockResolvedValue(undefined),
  setAgentDefaults: vi.fn().mockResolvedValue(undefined),
  handoffPreview: vi.fn().mockResolvedValue({ count: 2, lastFrom: "codex" }),
  getUiState: vi.fn().mockResolvedValue("all"),
  trustProject: vi.fn(),
}));
import * as ipc from "../ipc/commands";
import {
  activateTab,
  closeTab,
  persistTabs,
  send,
  setDraft,
  selectAgent,
  applyLiveEvent,
  openChatRow,
  useWorkspace,
  trustAndContinue,
} from "./chats";
const snapshot = (id: string, project: string): ChatSnapshot => ({
  id,
  project,
  title: "Title",
  status: "idle",
  unread: false,
  activeAgent: "claude",
  messages: [],
  steps: [],
  interactions: [],
  queue: [],
  sessions: [],
});
beforeEach(() => {
  vi.clearAllMocks();
  useWorkspace.setState({
    tabs: [],
    activeTab: null,
    snapshot: null,
    draft: "",
    project: "",
    sending: false,
    selectedStep: null,
    defaults: {},
    error: null,
    appSettings: null,
    trustRequest: null,
    agents: [],
    lastAgent: "claude",
  });
});
describe("per-tab ownership", () => {
  it("keeps a pending send in its original tab after a project switch", async () => {
    const a = { ...createTab("/a", defaultSettings), draft: "message A" };
    const b = { ...createTab("/b", defaultSettings), draft: "draft B" };
    useWorkspace.setState({
      tabs: [a, b],
      activeTab: a.id,
      project: a.project,
      draft: a.draft,
    });
    let finish!: (value: { chatId: string }) => void;
    vi.mocked(ipc.sendMessage).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    vi.mocked(ipc.openChat).mockResolvedValue(snapshot("chat-a", "/a"));
    const pending = send();
    await activateTab(b.id);
    finish({ chatId: "chat-a" });
    await pending;
    const state = useWorkspace.getState();
    expect(state.activeTab).toBe(b.id);
    expect(state.project).toBe("/b");
    expect(state.draft).toBe("draft B");
    expect(state.tabs.find((t) => t.id === a.id)?.snapshot?.id).toBe("chat-a");
    expect(state.tabs.find((t) => t.id === a.id)?.draft).toBe("");
  });
  it("persists distinct drafts and closes a chat created after its draft tab was closed", async () => {
    const a = createTab("/a", defaultSettings);
    const b = createTab("/b", defaultSettings);
    useWorkspace.setState({
      tabs: [a, b],
      activeTab: a.id,
      project: a.project,
    });
    setDraft("draft A", a.id);
    setDraft("draft B", b.id);
    await persistTabs();
    const saves = vi
      .mocked(ipc.setUiState)
      .mock.calls.filter(([key]) => key === "open_tabs");
    expect(
      JSON.parse(saves.at(-1)![1]).map((tab: { draft: string }) => tab.draft),
    ).toEqual(["draft A", "draft B"]);
    let finish!: (value: { chatId: string }) => void;
    vi.mocked(ipc.sendMessage).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const pending = send();
    await closeTab(a.id);
    finish({ chatId: "late-chat" });
    await pending;
    expect(ipc.closeChat).toHaveBeenCalledWith("late-chat");
    expect(useWorkspace.getState().tabs).toHaveLength(1);
  });
});

describe("agent handoff", () => {
  it("restores that agent's session settings and missed-message chooser", async () => {
    const chat = snapshot("chat", "/a");
    chat.sessions = [
      {
        agent: "claude",
        nativeId: "native-claude",
        model: "haiku",
        effort: "low",
        permission: "ask",
        syncedSeq: 2,
        contextUsed: null,
        contextWindow: null,
      },
    ];
    const tab = {
      ...createTab("/a", { ...defaultSettings, agent: "codex" }),
      chatId: "chat",
      snapshot: chat,
    };
    useWorkspace.setState({
      tabs: [tab],
      activeTab: tab.id,
      defaults: { claude: { ...defaultSettings, model: "sonnet" } },
    });
    await selectAgent("claude");
    expect(useWorkspace.getState().tabs[0].settings.model).toBe("haiku");
    expect(useWorkspace.getState().tabs[0].settings.permission).toBe("ask");
    expect(useWorkspace.getState().tabs[0].handoff).toEqual({
      count: 2,
      from: "codex",
      mode: "all",
    });
  });
  it("keeps stream deltas in their chat and clears them when a turn finishes", () => {
    const a = { ...createTab("/a", defaultSettings), chatId: "a" };
    const b = { ...createTab("/b", defaultSettings), chatId: "b" };
    useWorkspace.setState({ tabs: [a, b], activeTab: b.id });
    applyLiveEvent("a", { type: "replyDelta", text: "hello" });
    expect(useWorkspace.getState().tabs[0].liveReply).toBe("hello");
    expect(useWorkspace.getState().tabs[1].liveReply).toBe("");
    applyLiveEvent("a", { type: "turnFinished" });
    expect(useWorkspace.getState().tabs[0].liveReply).toBe("");
  });
});

describe("terminal row ownership", () => {
  it("imports once and selects the returned app chat instead of the native row id", async () => {
    const imported = snapshot("imported", "/a");
    vi.mocked(ipc.openNativeSession).mockResolvedValue(imported);
    vi.mocked(ipc.openChat).mockResolvedValue(imported);
    await openChatRow({
      kind: "terminal",
      id: "native:claude:terminal-id",
      nativeId: "terminal-id",
      agent: "claude",
      project: "/a",
      title: "Terminal",
      status: "idle",
      unread: false,
      updatedAt: 1,
    });
    expect(ipc.openNativeSession).toHaveBeenCalledWith("claude", "terminal-id");
    expect(useWorkspace.getState().tabs[0].chatId).toBe("imported");
    expect(useWorkspace.getState().snapshot?.id).toBe("imported");
  });
});

describe("project trust", () => {
  it("waits for explicit trust and keeps the send owned by its original tab", async () => {
    const settings = { schemaVersion: 1, projectsRoot: "/", binaries: { claude: null, codex: null, agy: null, muse: null }, enabledAgents: ["claude" as const], defaultAgent: "claude" as const, defaultPermission: "ask" as const, historyAgents: [], trustedProjects: [] as string[], setupComplete: true, notifications: true };
    const a = { ...createTab("/a", defaultSettings), draft: "message A" };
    const b = { ...createTab("/b", defaultSettings), draft: "draft B" };
    useWorkspace.setState({ tabs: [a, b], activeTab: a.id, project: "/a", draft: a.draft, appSettings: settings });
    await send();
    expect(ipc.sendMessage).not.toHaveBeenCalled();
    expect(useWorkspace.getState().trustRequest?.project).toBe("/a");
    await activateTab(b.id);
    vi.mocked(ipc.trustProject).mockResolvedValue({ settings: { ...settings, trustedProjects: ["/a"] }, error: null, profile: "preview" });
    vi.mocked(ipc.sendMessage).mockResolvedValue({ chatId: "chat-a" });
    vi.mocked(ipc.openChat).mockResolvedValue(snapshot("chat-a", "/a"));
    await trustAndContinue();
    expect(ipc.sendMessage).toHaveBeenCalledWith(null, "/a", "message A", defaultSettings, undefined);
    expect(useWorkspace.getState().activeTab).toBe(b.id);
    expect(useWorkspace.getState().draft).toBe("draft B");
  });
});
