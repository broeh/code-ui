import { invoke, isTauri } from "@tauri-apps/api/core";
import { mockCommand } from "./mock";
import type { AgentStatus, AppSettings, SettingsSnapshot } from "./types";
export function command<T>(
  name: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  return isTauri() ? invoke<T>(name, args) : mockCommand<T>(name, args);
}
export const agentStatus = () => command<AgentStatus[]>("agent_status");
export const getSettings = () => command<SettingsSnapshot>("get_settings");
export const saveSettings = (settings: AppSettings, createRoot: boolean) =>
  command<SettingsSnapshot>("save_settings", { settings, createRoot });
export const refreshAgents = (settings?: AppSettings) =>
  command<AgentStatus[]>("refresh_agents", { settings: settings ?? null });
export const trustProject = (project: string) =>
  command<SettingsSnapshot>("trust_project", { project });
export const getUiState = (key: string) =>
  command<string | null>("get_ui_state", { key });
export const setUiState = (key: string, value: string) =>
  command<void>("set_ui_state", { key, value });
import type {
  ChatRow,
  ChatSnapshot,
  InteractionResponse,
  TurnSettings,
  HandoffMode,
  SendResult,
} from "./types";
export const listChats = (project: string | null) =>
  command<ChatRow[]>("list_chats", { project });
export const openChat = (chatId: string) =>
  command<ChatSnapshot>("open_chat", { chatId });
export const sendMessage = (
  chatId: string | null,
  project: string,
  text: string,
  settings: TurnSettings,
  handoffMode?: HandoffMode,
) =>
  command<SendResult>("send_message", {
    target: chatId ? { chatId } : { draft: { project, agent: settings.agent } },
    text,
    settings,
    handoffMode,
  });
export const stopChat = (chatId: string) => command<void>("stop", { chatId });
export const retryChat = (chatId: string) => command<void>("retry", { chatId });
export const resumeQueue = (chatId: string) =>
  command<void>("resume_queue", { chatId });
export const respondInteraction = (
  chatId: string,
  interactionId: string,
  response: InteractionResponse,
) => command<void>("respond_interaction", { chatId, interactionId, response });
export const unqueue = (chatId: string, pos: number) =>
  command<string>("unqueue", { chatId, pos });
import type { AgentKind, ModelOption, ProjectRow } from "./types";
export const listProjects = () => command<ProjectRow[]>("list_projects");
export const createProject = (name: string) =>
  command<string>("create_project", { name });
export const addFolder = (path: string) =>
  command<string>("add_folder", { path });
export const setPinned = (path: string, pinned: boolean) =>
  command<void>("set_pinned", { path, pinned });
export const hideProject = (path: string) =>
  command<void>("hide_project", { path });
export const touchProject = (path: string) =>
  command<void>("touch_project", { path });
export const renameChat = (chatId: string, title: string) =>
  command<void>("rename_chat", { chatId, title });
export const hideChat = (chatId: string) =>
  command<void>("hide_chat", { chatId });
export const markRead = (chatId: string) =>
  command<void>("mark_read", { chatId });
export const closeChat = (chatId: string) =>
  command<void>("close_chat", { chatId });
export const getAgentDefaults = () =>
  command<TurnSettings[]>("get_agent_defaults");
export const setAgentDefaults = (settings: TurnSettings) =>
  command<void>("set_agent_defaults", { settings });
export const listModels = (agent: AgentKind) =>
  command<ModelOption[]>("list_models", { agent });
export const openExternal = (absPath: string) =>
  command<void>("open_external", { absPath });
import type { FileEntry, TextFile, WriteResult } from "./types";
export const listDir = (project: string, rel: string, showIgnored: boolean) =>
  command<FileEntry[]>("list_dir", { project, rel, showIgnored });
export const listAllFiles = (project: string) =>
  command<string[]>("list_all_files", { project });
export const readTextFile = (project: string, rel: string) =>
  command<TextFile>("read_text_file", { project, rel });
export const writeTextFile = (
  project: string,
  rel: string,
  text: string,
  expectedMtime: string | null,
) =>
  command<WriteResult>("write_text_file", {
    project,
    rel,
    text,
    expectedMtime,
  });
export const readImageFile = (project: string, rel: string) =>
  command<string>("read_image_file", { project, rel });
export const createFile = (project: string, relDir: string, name: string) =>
  command<string>("create_file", { project, relDir, name });
export const createFolder = (project: string, relDir: string, name: string) =>
  command<string>("create_folder", { project, relDir, name });
export const renamePath = (project: string, from: string, to: string) =>
  command<void>("rename_path", { project, from, to });
export const trashPath = (project: string, rel: string) =>
  command<void>("trash_path", { project, rel });
export const reveal = (absPath: string) => command<void>("reveal", { absPath });
export const watchProject = (project: string) =>
  command<void>("watch_project", { project });

export const handoffPreview = (chatId: string, targetAgent: AgentKind) =>
  command<{
    count: number;
    lastFrom: AgentKind | null;
    from: AgentKind | null;
  }>("handoff_preview", { chatId, targetAgent });

export const scanNativeSessions = (project: string | null) =>
  command<void>("scan_native_sessions", { project });
export const openNativeSession = (agent: AgentKind, nativeId: string) =>
  command<ChatSnapshot>("open_native_session", { agent, nativeId });
