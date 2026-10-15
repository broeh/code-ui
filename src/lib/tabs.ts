import type { ChatTab, TurnSettings } from "../ipc/types";
export const defaultSettings: TurnSettings = {
  agent: "claude",
  model: null,
  effort: null,
  permission: "ask",
};
export function createTab(project: string, settings: TurnSettings): ChatTab {
  return {
    id: crypto.randomUUID(),
    handoff: null,
    liveReply: "",
    preparingHandoff: false,
    chatId: null,
    project,
    draft: "",
    settings: { ...settings },
    status: "idle",
    unread: false,
    sending: false,
    lastFocusedAt: Date.now(),
    snapshot: null,
  };
}
export function savedTabs(tabs: ChatTab[]) {
  return tabs.map(
    ({
      snapshot: _snapshot,
      sending: _sending,
      status: _status,
      liveReply: _reply,
      preparingHandoff: _preparing,
      ...tab
    }) => tab,
  );
}
export function reorderTabs(tabs: ChatTab[], from: string, to: string) {
  const index = tabs.findIndex((tab) => tab.id === to);
  const source = tabs.find((tab) => tab.id === from);
  if (!source || index === -1) return tabs;
  const next = tabs.filter((tab) => tab.id !== from);
  next.splice(index, 0, source);
  return next;
}
export function projectTab(tabs: ChatTab[], project: string) {
  return tabs
    .filter((tab) => tab.project === project)
    .sort((a, b) => b.lastFocusedAt - a.lastFocusedAt)[0];
}
