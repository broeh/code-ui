export type AgentKind = "claude" | "codex" | "agy" | "muse";
export interface AgentStatus {
  agent: AgentKind;
  installed: boolean;
  enabled: boolean;
  problem: string | null;
  path: string | null;
  version: string | null;
}
export interface AppSettings {
  schemaVersion: number;
  projectsRoot: string;
  binaries: Record<AgentKind, string | null>;
  enabledAgents: AgentKind[];
  defaultAgent: AgentKind;
  defaultPermission: "ask" | "yolo";
  historyAgents: AgentKind[];
  trustedProjects: string[];
  setupComplete: boolean;
  notifications: boolean;
}
export interface SettingsSnapshot {
  settings: AppSettings;
  error: string | null;
  profile: string;
}
export const agentNames: Record<AgentKind, string> = {
  claude: "Claude Code",
  codex: "Codex",
  agy: "Antigravity",
  muse: "Muse",
};
export const agentLetters: Record<AgentKind, string> = {
  claude: "C",
  codex: "X",
  agy: "A",
  muse: "M",
};
export type Permission = "yolo" | "ask" | "plan";
export interface TurnSettings {
  agent: AgentKind;
  model: string | null;
  effort: string | null;
  permission: Permission;
}
export type ChatStatus =
  "idle" | "working" | "needs_input" | "failed" | "stopped" | "interrupted";
export type StepKind =
  | "thinking"
  | "read"
  | "search"
  | "list"
  | "edit"
  | "write"
  | "command"
  | "web"
  | "subagent"
  | "todo"
  | "mcp"
  | "skill"
  | "compact"
  | "other";
export interface Message {
  seq: number;
  role: "user" | "assistant" | "handoff" | "notice";
  agent: AgentKind | null;
  turnId: string | null;
  text: string;
  meta: {
    imported?: boolean;
    model?: string;
    effort?: string;
    durationMs?: number;
    handoff?: { mode: HandoffMode; count: number; context: string };
  };
  createdAt: number;
}
export interface Step {
  id: string;
  turnId: string;
  kind: StepKind;
  title: string;
  status: "running" | "done" | "failed" | "declined" | "interrupted";
  parentId: string | null;
  startedAt: number;
  endedAt: number | null;
  detail: { input?: unknown; output?: string };
}
export interface Question {
  id: string;
  header: string;
  text: string;
  options: { label: string; description: string }[];
  multi: boolean;
  allowOther: boolean;
  secret?: boolean;
}
export interface Interaction {
  id: string;
  kind: "question" | "approval" | "plan";
  title: string;
  questions: Question[];
  canDismiss?: boolean;
  maxTextLength?: number;
  approval: {
    summary: string;
    detail: string;
    choices: { id: string; label: string; kind: string }[];
  } | null;
  plan: string | null;
}
export interface InteractionRow {
  id: string;
  turnId: string;
  payload: Interaction;
  status: string;
  answer: string | null;
}
export type InteractionResponse =
  | {
      type: "answers";
      answers: { questionId: string; labels: string[]; other: string | null }[];
      note: string | null;
    }
  | { type: "choice"; choiceId: string; feedback: string | null }
  | { type: "dismiss" };
export interface QueueRow {
  pos: number;
  text: string;
  settings: TurnSettings;
}
export interface AgentSession {
  agent: AgentKind;
  nativeId: string | null;
  model: string | null;
  effort: string | null;
  permission: Permission;
  syncedSeq: number;
  contextUsed: number | null;
  contextWindow: number | null;
}
export interface ChatSnapshot {
  unread: boolean;
  id: string;
  project: string;
  title: string;
  status: ChatStatus;
  activeAgent: AgentKind;
  messages: Message[];
  steps: Step[];
  interactions: InteractionRow[];
  queue: QueueRow[];
  sessions: AgentSession[];
}
export interface ChatRow {
  unread: boolean;
  kind: "app" | "terminal";
  nativeId?: string;
  imported?: boolean;
  id: string;
  project: string;
  title: string;
  status: ChatStatus;
  agent: AgentKind;
  updatedAt: number;
}
export interface ProjectRow {
  path: string;
  name: string;
  pinned: boolean;
  extra: boolean;
  lastActivity: number;
  working: number;
  needsInput: number;
}
export interface ModelOption {
  isDefault?: boolean;
  id: string;
  label: string;
  efforts?: string[];
}
export interface ChatTab {
  handoff: HandoffChoice | null;
  liveReply: string;
  preparingHandoff: boolean;
  id: string;
  chatId: string | null;
  project: string;
  draft: string;
  settings: TurnSettings;
  status: ChatStatus;
  unread: boolean;
  sending: boolean;
  lastFocusedAt: number;
  snapshot: ChatSnapshot | null;
}
export interface FileEntry {
  name: string;
  rel: string;
  isDir: boolean;
  size: number;
  kind: "folder" | "text" | "image" | "other";
}
export type TextFile =
  { text: string; mtime: string } | { error: "tooLarge" | "binary" };
export type WriteResult = { mtime: string } | { conflict: true };
export interface EditorFile {
  project: string;
  rel: string;
  text: string;
  savedText: string;
  mtime: string;
  kind: "text" | "image";
  imageUrl?: string;
  conflict: boolean;
  preview: boolean;
  saving: boolean;
}

export type HandoffMode = "all" | "last" | "summary";
export interface HandoffChoice {
  count: number;
  from: AgentKind | null;
  mode: HandoffMode;
}
export type SendResult =
  | { chatId: string }
  | { needsHandoffChoice: { count: number; from: AgentKind | null } };
