import { create } from "zustand";
import * as ipc from "../ipc/commands";
import type {
  AgentKind,
  AppSettings,
  SettingsSnapshot,
  AgentStatus,
  ChatRow,
  ChatSnapshot,
  ChatTab,
  ChatStatus,
  ProjectRow,
  TurnSettings,
  HandoffMode,
} from "../ipc/types";
import { availableAgent } from "../lib/agents";
import {
  createTab,
  defaultSettings,
  projectTab,
  reorderTabs,
  savedTabs,
} from "../lib/tabs";
interface Workspace {
  project: string;
  snapshot: ChatSnapshot | null;
  chats: ChatRow[];
  projects: ProjectRow[];
  agents: AgentStatus[];
  tabs: ChatTab[];
  activeTab: string | null;
  defaults: Partial<Record<AgentKind, TurnSettings>>;
  lastAgent: AgentKind;
  draft: string;
  selectedStep: string | null;
  settings: TurnSettings;
  error: string | null;
  sending: boolean;
  ready: boolean;
  appSettings: AppSettings | null;
  settingsError: string | null;
  profile: string;
  trustRequest: { project: string; action: () => Promise<void> } | null;
}
export const useWorkspace = create<Workspace>(() => ({
  project: "",
  snapshot: null,
  chats: [],
  projects: [],
  agents: [],
  tabs: [],
  activeTab: null,
  defaults: {},
  lastAgent: "claude",
  draft: "",
  selectedStep: null,
  settings: defaultSettings,
  error: null,
  sending: false,
  ready: false,
  appSettings: null,
  settingsError: null,
  profile: "stable",
  trustRequest: null,
}));
export function reportError(error: unknown) {
  useWorkspace.setState({ error: String(error) });
}
function projectActive(tabs: ChatTab[], activeTab: string | null) {
  const previous = useWorkspace.getState();
  const tab = tabs.find((tab) => tab.id === activeTab);
  return {
    tabs,
    activeTab: tab?.id ?? null,
    project: tab?.project ?? previous.project,
    snapshot: tab?.snapshot ?? null,
    draft: tab?.draft ?? "",
    settings: tab?.settings ?? defaultSettings,
    sending: tab?.sending ?? false,
  };
}
function updateTab(id: string, patch: Partial<ChatTab>) {
  const state = useWorkspace.getState();
  const tabs = state.tabs.map((tab) =>
    tab.id === id ? { ...tab, ...patch } : tab,
  );
  useWorkspace.setState(projectActive(tabs, state.activeTab));
}
let writes: Promise<void> = Promise.resolve();
export function persistTabs() {
  const { tabs, activeTab, project } = useWorkspace.getState();
  const saved = JSON.stringify(savedTabs(tabs));
  writes = writes.catch(reportError).then(async () => {
    await ipc.setUiState("open_tabs", saved);
    await ipc.setUiState("active_tab", activeTab ?? "");
    await ipc.setUiState("active_project", project);
  });
  return writes;
}
export function setDraft(
  text: string,
  tabId = useWorkspace.getState().activeTab,
) {
  if (!tabId) return;
  updateTab(tabId, { draft: text });
  void persistTabs().catch(reportError);
}
export function appendDraft(tabId: string, text: string) {
  const tab = useWorkspace.getState().tabs.find((tab) => tab.id === tabId);
  if (tab) setDraft(tab.draft ? `${tab.draft}\n${text}` : text, tabId);
}
export async function setTurnSettings(
  settings: TurnSettings,
  tabId = useWorkspace.getState().activeTab,
) {
  if (!tabId) return;
  updateTab(tabId, { settings });
  const state = useWorkspace.getState();
  useWorkspace.setState({
    defaults: { ...state.defaults, [settings.agent]: settings },
    lastAgent: settings.agent,
  });
  await ipc.setAgentDefaults(settings);
  await ipc.setUiState("last_agent", settings.agent);
  await persistTabs();
}
export async function refreshChat(chatId?: string) {
  const { project, tabs } = useWorkspace.getState();
  const [chats, projects, snapshot] = await Promise.all([
    ipc.listChats(project),
    ipc.listProjects(),
    chatId ? ipc.openChat(chatId) : Promise.resolve(null),
  ]);
  const current = useWorkspace.getState();
  useWorkspace.setState({
    projects,
    ...(current.project === project ? { chats } : {}),
  });
  if (snapshot)
    for (const tab of tabs.filter((tab) => tab.chatId === chatId))
      updateTab(tab.id, {
        snapshot,
        status: snapshot.status,
        unread: snapshot.unread,
      });
  if (snapshot) {
    for (const tab of useWorkspace
      .getState()
      .tabs.filter((t) => t.chatId === chatId && t.handoff)) {
      const preview = await ipc.handoffPreview(snapshot.id, tab.settings.agent);
      updateTab(tab.id, {
        handoff: preview.count
          ? { ...tab.handoff!, count: preview.count, from: preview.lastFrom }
          : null,
      });
    }
  }
}
export function applyChatStatus(payload: {
  chatId: string;
  status: ChatStatus;
  unread: boolean;
}) {
  const state = useWorkspace.getState();
  const tabs = state.tabs.map((tab) =>
    tab.chatId === payload.chatId
      ? {
          ...tab,
          status: payload.status,
          unread: payload.unread,
          snapshot: tab.snapshot
            ? {
                ...tab.snapshot,
                status: payload.status,
                unread: payload.unread,
              }
            : null,
        }
      : tab,
  );
  useWorkspace.setState(projectActive(tabs, state.activeTab));
}
export async function activateTab(id: string) {
  const requested = useWorkspace.getState().tabs.find((tab) => tab.id === id);
  if (requested) await ipc.scanNativeSessions(requested.project);
  const state = useWorkspace.getState();
  const tab = state.tabs.find((tab) => tab.id === id);
  if (!tab) return;
  const tabs = state.tabs.map((tab) =>
    tab.id === id ? { ...tab, lastFocusedAt: Date.now(), unread: false } : tab,
  );
  useWorkspace.setState({
    ...projectActive(tabs, id),
    selectedStep: null,
    error: null,
  });
  if (tab.chatId) {
    await ipc.markRead(tab.chatId);
    await refreshChat(tab.chatId);
  } else await refreshChat();
  await ipc.touchProject(tab.project);
  await persistTabs();
}
export async function newChat(project = useWorkspace.getState().project) {
  if (!project) return;
  const state = useWorkspace.getState();
  const agent = availableAgent(state.agents, state.lastAgent);
  const settings = state.defaults[agent] ?? { ...defaultSettings, agent, permission: state.appSettings?.defaultPermission ?? "ask" };
  const tab = createTab(project, settings);
  useWorkspace.setState({
    ...projectActive([...state.tabs, tab], tab.id),
    selectedStep: null,
    error: null,
  });
  await ipc.touchProject(project);
  await persistTabs();
  await refreshChat();
}
export async function selectProject(project: string) {
  await ipc.scanNativeSessions(project);
  const tab = projectTab(useWorkspace.getState().tabs, project);
  if (tab) await activateTab(tab.id);
  else await newChat(project);
}
export async function openChatRow(chat: ChatRow) {
  if (chat.kind === "terminal" && !projectTrusted(chat.project, () => openChatRow(chat))) return;
  const id =
    chat.kind === "terminal"
      ? (await ipc.openNativeSession(chat.agent, chat.nativeId!)).id
      : chat.id;
  await selectChat(id);
}
export async function appChatId(chat: ChatRow) {
  return chat.kind === "terminal"
    ? (await ipc.openNativeSession(chat.agent, chat.nativeId!)).id
    : chat.id;
}
export async function selectChat(chatId: string) {
  const existing = useWorkspace
    .getState()
    .tabs.find((tab) => tab.chatId === chatId);
  if (existing) {
    await activateTab(existing.id);
    return;
  }
  const snapshot = await ipc.openChat(chatId);
  const session = snapshot.sessions.find(
    (s) => s.agent === snapshot.activeAgent,
  );
  const settings = session
    ? {
        agent: session.agent,
        model: session.model,
        effort: session.effort,
        permission: session.permission,
      }
    : (useWorkspace.getState().defaults[snapshot.activeAgent] ??
      defaultSettings);
  const tab = {
    ...createTab(snapshot.project, settings),
    chatId,
    snapshot,
    status: snapshot.status,
    unread: snapshot.unread,
  };
  useWorkspace.setState(
    projectActive([...useWorkspace.getState().tabs, tab], tab.id),
  );
  await activateTab(tab.id);
}
export async function closeTab(id: string) {
  const state = useWorkspace.getState();
  const tab = state.tabs.find((tab) => tab.id === id);
  if (!tab) return;
  if (tab.chatId) await ipc.closeChat(tab.chatId);
  const latest = useWorkspace.getState();
  const index = latest.tabs.findIndex((tab) => tab.id === id);
  const tabs = latest.tabs.filter((tab) => tab.id !== id);
  const active =
    latest.activeTab === id
      ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? null)
      : latest.activeTab;
  useWorkspace.setState({ ...projectActive(tabs, active), selectedStep: null });
  await persistTabs();
  await refreshChat();
}
export async function moveTab(from: string, to: string) {
  const state = useWorkspace.getState();
  useWorkspace.setState({ tabs: reorderTabs(state.tabs, from, to) });
  await persistTabs();
}
async function loadWorkspace() {
  const [
    projects,
    agents,
    defaults,
    rawTabs,
    active,
    lastProject,
    lastAgent,
    legacyChat,
    legacyDraft,
    configuration,
  ] = await Promise.all([
    ipc.listProjects(),
    ipc.agentStatus(),
    ipc.getAgentDefaults(),
    ipc.getUiState("open_tabs"),
    ipc.getUiState("active_tab"),
    ipc.getUiState("active_project"),
    ipc.getUiState("last_agent"),
    ipc.getUiState("active_chat"),
    ipc.getUiState("draft"),
    ipc.getSettings(),
  ]);
  useWorkspace.setState({
    projects,
    agents,
    defaults: Object.fromEntries(defaults.map((s) => [s.agent, s])),
    lastAgent: availableAgent(agents, (lastAgent as AgentKind) || configuration.settings.defaultAgent),
    appSettings: configuration.settings,
    settingsError: configuration.error,
    profile: configuration.profile,
    project: lastProject ?? "",
  });
  let tabs: ChatTab[] = rawTabs
    ? JSON.parse(rawTabs).map((tab: ChatTab) => ({
        ...tab,
        snapshot: null,
        status: "idle",
        sending: false,
        liveReply: "",
        preparingHandoff: false,
        handoff: tab.handoff ?? null,
      }))
    : [];
  if (rawTabs === null && legacyChat) {
    const snapshot = await ipc.openChat(legacyChat);
    const session = snapshot.sessions.find(
      (s) => s.agent === snapshot.activeAgent,
    );
    const tab = createTab(
      snapshot.project,
      session
        ? {
            agent: session.agent,
            model: session.model,
            effort: session.effort,
            permission: session.permission,
          }
        : defaultSettings,
    );
    tabs = [
      {
        ...tab,
        chatId: snapshot.id,
        snapshot,
        status: snapshot.status,
        draft: legacyDraft ?? "",
      },
    ];
    if (!projects.some((p) => p.path === snapshot.project))
      await ipc.addFolder(snapshot.project);
  }
  const restored = await Promise.all(
    tabs.map(async (tab) =>
      tab.chatId ? { ...tab, snapshot: await ipc.openChat(tab.chatId) } : tab,
    ),
  );
  tabs = restored.map((tab) => ({
    ...tab,
    status: tab.snapshot?.status ?? "idle",
    unread: tab.snapshot?.unread ?? false,
  }));
  const id = tabs.some((t) => t.id === active) ? active : (tabs[0]?.id ?? null);
  useWorkspace.setState({ ...projectActive(tabs, id), ready: true });
  if (id) await activateTab(id);
  else if (rawTabs === null && projects[0] && configuration.settings.setupComplete) await newChat(projects[0].path);
  else await refreshChat();
}
let initialization: Promise<void> | undefined;
export function initializeWorkspace() {
  if (useWorkspace.getState().ready) return Promise.resolve();
  initialization ??= loadWorkspace();
  return initialization;
}
export async function send(tabId = useWorkspace.getState().activeTab) {
  const { tabs } = useWorkspace.getState();
  const tab = tabs.find((tab) => tab.id === tabId);
  if (!tab || !tab.draft.trim() || tab.sending) return;
  if (!projectTrusted(tab.project, () => send(tab.id))) return;
  updateTab(tab.id, { sending: true });
  try {
    const result = await ipc.sendMessage(
      tab.chatId,
      tab.project,
      tab.draft,
      tab.settings,
      tab.handoff?.mode,
    );
    if ("needsHandoffChoice" in result) {
      const raw = await ipc.getUiState("last_handoff_mode");
      updateTab(tab.id, {
        handoff: {
          ...result.needsHandoffChoice,
          mode: (raw as HandoffMode) || "all",
        },
      });
      return;
    }
    if (!useWorkspace.getState().tabs.some((t) => t.id === tab.id)) {
      await ipc.closeChat(result.chatId);
      return;
    }
    const snapshot = await ipc.openChat(result.chatId);
    const current = useWorkspace.getState().tabs.find((t) => t.id === tab.id)!;
    updateTab(tab.id, {
      chatId: result.chatId,
      snapshot,
      status: snapshot.status,
      handoff: null,
      liveReply: "",
      draft: current.draft === tab.draft ? "" : current.draft,
    });
    await persistTabs();
    await refreshChat(result.chatId);
  } catch (error) {
    reportError(error);
  } finally {
    updateTab(tab.id, { sending: false });
  }
}

export async function selectAgent(
  agent: AgentKind,
  tabId = useWorkspace.getState().activeTab,
) {
  const state = useWorkspace.getState();
  const tab = state.tabs.find((tab) => tab.id === tabId);
  if (!tab || agent === tab.settings.agent) return;
  const session = tab.snapshot?.sessions.find(
    (session) => session.agent === agent,
  );
  const settings = session
    ? {
        agent,
        model: session.model,
        effort: session.effort,
        permission: session.permission,
      }
    : (state.defaults[agent] ?? { ...defaultSettings, agent });
  await setTurnSettings(settings, tab.id);
  if (tab.chatId) {
    const preview = await ipc.handoffPreview(tab.chatId, agent);
    const raw = await ipc.getUiState("last_handoff_mode");
    if (
      useWorkspace.getState().tabs.find((current) => current.id === tab.id)
        ?.settings.agent !== agent
    )
      return;
    updateTab(tab.id, {
      handoff: preview.count
        ? {
            count: preview.count,
            from: preview.lastFrom,
            mode: (raw as HandoffMode) || "all",
          }
        : null,
    });
  }
  await persistTabs();
}
export async function chooseHandoff(mode: HandoffMode) {
  const { activeTab, tabs } = useWorkspace.getState();
  const tab = tabs.find((tab) => tab.id === activeTab);
  if (tab?.handoff) updateTab(tab.id, { handoff: { ...tab.handoff, mode } });
  await ipc.setUiState("last_handoff_mode", mode);
  await persistTabs();
}
export function applyLiveEvent(
  chatId: string,
  event: {
    type: string;
    text?: string;
    id?: string;
    title?: string | null;
    outputAppend?: string | null;
  },
) {
  for (const tab of useWorkspace
    .getState()
    .tabs.filter((tab) => tab.chatId === chatId)) {
    if (event.type === "replyDelta")
      updateTab(tab.id, { liveReply: tab.liveReply + (event.text ?? "") });
    else if (event.type === "handoffStarted")
      updateTab(tab.id, { preparingHandoff: true, liveReply: "" });
    else if (event.type === "turnStarted")
      updateTab(tab.id, { preparingHandoff: false, liveReply: "" });
    else if (event.type === "turnFinished")
      updateTab(tab.id, { preparingHandoff: false, liveReply: "" });
    else if (event.type === "stepUpdated" && tab.snapshot) {
      const turn = tab.snapshot.messages
        .filter((message) => message.role === "user")
        .at(-1)?.turnId;
      updateTab(tab.id, {
        snapshot: {
          ...tab.snapshot,
          steps: tab.snapshot.steps.map((step) =>
            step.turnId === turn && step.id.endsWith(`:${event.id}`)
              ? {
                  ...step,
                  title: event.title ?? step.title,
                  detail: {
                    ...step.detail,
                    output: (
                      (step.detail.output ?? "") + (event.outputAppend ?? "")
                    ).slice(0, 16384),
                  },
                }
              : step,
          ),
        },
      });
    }
  }
}

export async function applySettings(configuration: SettingsSnapshot) {
  const [agents, defaults] = await Promise.all([ipc.agentStatus(), ipc.getAgentDefaults()]);
  useWorkspace.setState({
    appSettings: configuration.settings, settingsError: configuration.error, profile: configuration.profile,
    agents, defaults: Object.fromEntries(defaults.map((settings) => [settings.agent, settings])),
    lastAgent: availableAgent(agents, configuration.settings.defaultAgent),
  });
  await refreshChat();
  const state = useWorkspace.getState();
  if (!state.tabs.length && state.projects[0]) await newChat(state.projects[0].path);
}

function projectTrusted(project: string, action: () => Promise<void>) {
  const settings = useWorkspace.getState().appSettings;
  if (!settings || settings.trustedProjects.includes(project)) return true;
  useWorkspace.setState({ trustRequest: { project, action } });
  return false;
}

export async function trustAndContinue() {
  const request = useWorkspace.getState().trustRequest;
  if (!request) return;
  const configuration = await ipc.trustProject(request.project);
  useWorkspace.setState({ appSettings: configuration.settings, trustRequest: null });
  await request.action();
}

export async function retryWithTrust(chatId: string) {
  const snapshot = await ipc.openChat(chatId);
  if (projectTrusted(snapshot.project, () => retryWithTrust(chatId))) await ipc.retryChat(chatId);
}

export async function resumeWithTrust(chatId: string) {
  const snapshot = await ipc.openChat(chatId);
  if (projectTrusted(snapshot.project, () => resumeWithTrust(chatId))) await ipc.resumeQueue(chatId);
}
