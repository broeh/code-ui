import type {
  AgentStatus,
  AppSettings,
  AgentKind,
  SettingsSnapshot,
  ChatSnapshot,
  InteractionResponse,
  TurnSettings,
  ProjectRow,
  FileEntry,
} from "./types";
import { mockEmit } from "./events";
import { agentNames } from "./types";
const configuration: SettingsSnapshot = JSON.parse(localStorage.getItem("code-ui:mock_configuration") ?? "null") ?? {
  settings: { schemaVersion: 1, projectsRoot: "/home/user/Projects", binaries: { claude: null, codex: null, agy: null, muse: null }, enabledAgents: ["claude", "codex"], defaultAgent: "claude", defaultPermission: "ask", historyAgents: [], trustedProjects: [], setupComplete: false, notifications: true },
  error: null, profile: "preview",
};
function saveConfiguration() { localStorage.setItem("code-ui:mock_configuration", JSON.stringify(configuration)); }
function detectedAgents(settings: AppSettings = configuration.settings): AgentStatus[] {
  const overrides: Partial<Record<AgentKind, boolean>> = JSON.parse(localStorage.getItem("code-ui:mock_agents") ?? "{}");
  return (["claude", "codex", "agy", "muse"] as AgentKind[]).map((agent) => ({ agent, enabled: settings.enabledAgents.includes(agent), installed: overrides[agent] ?? true, problem: overrides[agent] === false ? "CLI executable not found." : null, path: null, version: "Browser preview" }));
}
const chats: Record<string, ChatSnapshot> = JSON.parse(
  localStorage.getItem("code-ui:mock_chats") ?? "{}",
);
const projects: ProjectRow[] = JSON.parse(
  localStorage.getItem("code-ui:mock_projects") ?? "null",
) ?? [
  {
    path: "/home/user/Projects/example-app",
    name: "example-app",
    pinned: true,
    extra: false,
    lastActivity: Date.now(),
    working: 0,
    needsInput: 0,
  },
  {
    path: "/tmp/code-ui-acceptance",
    name: "code-ui-acceptance",
    pinned: false,
    extra: true,
    lastActivity: Date.now() - 3600000,
    working: 0,
    needsInput: 0,
  },
  {
    path: "/home/user/Projects/website",
    name: "website",
    pinned: false,
    extra: false,
    lastActivity: Date.now() - 86400000,
    working: 0,
    needsInput: 0,
  },
];
const hidden = new Set<string>(
  JSON.parse(localStorage.getItem("code-ui:mock_hidden") ?? "[]"),
);
const defaults: Record<string, TurnSettings> = JSON.parse(
  localStorage.getItem("code-ui:mock_defaults") ?? "{}",
);
function saveProjects() {
  localStorage.setItem("code-ui:mock_projects", JSON.stringify(projects));
}
const fileProjects: Record<
  string,
  Record<
    string,
    { text: string; mtime: string; folder?: boolean; binary?: boolean }
  >
> = {};
let fileClock = 1;
function projectFiles(project: string) {
  return (fileProjects[project] ??= {
    "README.md": {
      text: "# Scratch project\nA local workspace.\n",
      mtime: "1",
    },
    "PLAN.md": { text: "# Plan\n\nBuild and verify the app.\n", mtime: "1" },
    ".gitignore": { text: "ignored.txt\n", mtime: "1" },
    "ignored.txt": { text: "Ignored example", mtime: "1" },
    src: { text: "", mtime: "1", folder: true },
    "src/main.ts": { text: "console.log('ready');\n", mtime: "1" },
  });
}
function fileChanged(project: string, rel: string) {
  mockEmit("files-changed", {
    project,
    dirs: [rel.split("/").slice(0, -1).join("/")],
  });
}
const timers = new Map<string, ReturnType<typeof setTimeout>>();
function changed(chat: ChatSnapshot) {
  localStorage.setItem("code-ui:mock_chats", JSON.stringify(chats));
  mockEmit("chat-status", {
    chatId: chat.id,
    status: chat.status,
    unread: chat.unread ?? false,
  });
}
function finish(chat: ChatSnapshot, reply: string) {
  chat.messages.push({
    seq: chat.messages.length + 1,
    role: "assistant",
    agent: chat.activeAgent,
    turnId: chat.messages.at(-1)?.turnId ?? null,
    text: reply,
    meta: { model: "Browser preview", durationMs: 1200 },
    createdAt: Date.now(),
  });
  chat.status = "idle";
  chat.sessions.forEach((session) => {
    if (session.agent === chat.activeAgent)
      session.syncedSeq = chat.messages.length;
  });
  chat.steps.forEach((s) => {
    if (s.status === "running") {
      s.status = "done";
      s.title = s.title.replace(/^Reading /, "Read ");
      s.endedAt = Date.now();
    }
  });
  chat.interactions.forEach((r) => {
    if (r.status === "pending") r.status = "expired";
  });
  timers.delete(chat.id);
  changed(chat);
  const next = chat.queue.shift();
  if (next) start(chat, next.text, next.settings);
}
function start(chat: ChatSnapshot, text: string, settings: TurnSettings) {
  const turnId = crypto.randomUUID();
  chat.status = "working";
  chat.activeAgent = settings.agent;
  chat.messages.push({
    seq: chat.messages.length + 1,
    role: "user",
    agent: settings.agent,
    turnId,
    text,
    meta: {},
    createdAt: Date.now(),
  });
  const session = {
    agent: settings.agent,
    nativeId: `mock-${chat.id}-${settings.agent}`,
    model: settings.model,
    effort: settings.effort,
    permission: settings.permission,
    syncedSeq: chat.messages.length,
    contextUsed: 48_000,
    contextWindow: 200_000,
  };
  chat.sessions = [
    ...chat.sessions.filter((s) => s.agent !== settings.agent),
    session,
  ];
  chat.steps.push({
    id: `${turnId}:read`,
    turnId,
    kind: "read",
    title: "Reading README.md",
    status: "running",
    parentId: null,
    startedAt: Date.now(),
    endedAt: null,
    detail: {
      input: { file_path: "README.md" },
      output: "# Scratch project\nA local test workspace.",
    },
  });
  chat.steps.push({
    id: `${turnId}:todo`,
    turnId,
    kind: "todo",
    title: "Updating the task list",
    status: "done",
    parentId: null,
    startedAt: Date.now(),
    endedAt: Date.now(),
    detail: {
      input: {
        todos: [
          { status: "completed", text: "Read the README" },
          {
            status: "in_progress",
            text: "Suggest a cleaner project structure",
          },
          { status: "pending", text: "Summarize the changes" },
        ],
      },
    },
  });
  changed(chat);
  timers.set(
    chat.id,
    setTimeout(() => {
      if (
        settings.agent !== "agy" &&
        (settings.permission !== "yolo" ||
          text.toLowerCase().includes("question"))
      ) {
        const kind =
          settings.permission === "plan"
            ? "plan"
            : settings.permission === "ask"
              ? "approval"
              : "question";
        chat.interactions.push({
          id: crypto.randomUUID(),
          turnId,
          status: "pending",
          answer: null,
          payload: {
            id: turnId,
            kind,
            title:
              kind === "question"
                ? `${agentNames[settings.agent]} has a question`
                : kind === "plan"
                  ? "Start working on this plan?"
                  : `Allow ${agentNames[settings.agent]} to run a command?`,
            questions:
              kind === "question"
                ? [
                    {
                      id: "0",
                      header: "Color",
                      text: "Do you prefer red or blue?",
                      options: [
                        { label: "Red", description: "Use red." },
                        { label: "Blue", description: "Use blue." },
                      ],
                      multi: false,
                      allowOther: true,
                    },
                  ]
                : [],
            approval:
              kind === "approval"
                ? {
                    summary: "Print the project folder",
                    detail: "pwd",
                    choices: [
                      { id: "allow", label: "Allow", kind: "allow" },
                      { id: "deny", label: "Decline", kind: "deny" },
                    ],
                  }
                : null,
            plan:
              kind === "plan"
                ? "## Plan\n\n1. Read the project.\n2. Write a small change.\n3. Verify it."
                : null,
          },
        });
        chat.status = "needs_input";
        changed(chat);
      } else
        finish(
          chat,
          `The scratch project is ready.\n\nYou asked: ${text}\n\n\`\`\`sh\npwd\n\`\`\``,
        );
    }, 1200),
  );
}
export async function mockCommand<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  const chatId = String(args.chatId ?? "");
  const chat = chats[chatId];
  switch (command) {
    case "watch_project":
      return undefined as T;
    case "list_dir": {
      const files = projectFiles(String(args.project));
      const dir = String(args.rel);
      const entries = Object.entries(files)
        .filter(
          ([rel]) =>
            rel.split("/").slice(0, -1).join("/") === dir &&
            rel !== ".git" &&
            (args.showIgnored || rel !== "ignored.txt"),
        )
        .map(([rel, value]) => ({
          name: rel.split("/").at(-1)!,
          rel,
          isDir: !!value.folder,
          size: value.text.length,
          kind: value.folder ? "folder" : "text",
        }));
      entries.sort(
        (a, b) =>
          Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name),
      );
      return entries as FileEntry[] as T;
    }
    case "list_all_files":
      return Object.entries(projectFiles(String(args.project)))
        .filter(([rel, value]) => !value.folder && rel !== "ignored.txt")
        .map(([rel]) => rel) as T;
    case "read_text_file": {
      const value = projectFiles(String(args.project))[String(args.rel)];
      if (!value) throw Error("File not found");
      return { text: value.text, mtime: value.mtime } as T;
    }
    case "write_text_file": {
      const files = projectFiles(String(args.project));
      const rel = String(args.rel);
      if (
        args.expectedMtime !== null &&
        args.expectedMtime !== files[rel]?.mtime
      )
        return { conflict: true } as T;
      files[rel] = { text: String(args.text), mtime: String(++fileClock) };
      fileChanged(String(args.project), rel);
      return { mtime: files[rel].mtime } as T;
    }
    case "create_file":
    case "create_folder": {
      const files = projectFiles(String(args.project));
      const rel = [args.relDir, args.name].filter(Boolean).join("/");
      if (files[rel]) throw Error("A file with that name already exists.");
      files[rel] = {
        text: "",
        mtime: String(++fileClock),
        folder: command === "create_folder",
      };
      fileChanged(String(args.project), rel);
      return rel as T;
    }
    case "rename_path": {
      const files = projectFiles(String(args.project));
      const from = String(args.from),
        to = String(args.to);
      if (files[to]) throw Error("A file with that name already exists.");
      files[to] = files[from];
      delete files[from];
      fileChanged(String(args.project), from);
      return undefined as T;
    }
    case "trash_path": {
      delete projectFiles(String(args.project))[String(args.rel)];
      fileChanged(String(args.project), String(args.rel));
      return undefined as T;
    }
    case "reveal":
      return undefined as T;
    case "read_image_file":
      return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6L3sAAAAASUVORK5CYII=" as T;
    case "list_projects":
      return projects
        .map((p) => ({
          ...p,
          working: Object.values(chats).filter(
            (c) => c.project === p.path && c.status === "working",
          ).length,
          needsInput: Object.values(chats).filter(
            (c) => c.project === p.path && c.status === "needs_input",
          ).length,
        }))
        .sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) ||
            (a.pinned
              ? a.name.localeCompare(b.name)
              : b.lastActivity - a.lastActivity),
        ) as T;
    case "create_project": {
      const name = String(args.name).trim();
      if (!name || name.includes("/") || name === ".." || name === ".")
        throw Error("Use one folder name, without slashes.");
      const path = `/home/user/Projects/${name}`;
      if (projects.some((p) => p.path === path))
        throw Error("A folder with that name already exists.");
      projects.push({
        path,
        name,
        pinned: false,
        extra: false,
        lastActivity: Date.now(),
        working: 0,
        needsInput: 0,
      });
      saveProjects();
      return path as T;
    }
    case "add_folder": {
      const path = String(args.path);
      if (!path.startsWith("/")) throw Error("Use an absolute folder path.");
      if (!projects.some((p) => p.path === path))
        projects.push({
          path,
          name: path.split("/").at(-1)!,
          pinned: false,
          extra: true,
          lastActivity: Date.now(),
          working: 0,
          needsInput: 0,
        });
      saveProjects();
      return path as T;
    }
    case "set_pinned": {
      const row = projects.find((p) => p.path === args.path)!;
      row.pinned = Boolean(args.pinned);
      saveProjects();
      return undefined as T;
    }
    case "hide_project": {
      const index = projects.findIndex((p) => p.path === args.path);
      projects.splice(index, 1);
      saveProjects();
      return undefined as T;
    }
    case "touch_project":
      return undefined as T;
    case "rename_chat":
      chat.title = String(args.title).trim();
      changed(chat);
      return undefined as T;
    case "hide_chat":
      hidden.add(chatId);
      localStorage.setItem("code-ui:mock_hidden", JSON.stringify([...hidden]));
      changed(chat);
      return undefined as T;
    case "mark_read":
      chat.unread = false;
      return undefined as T;
    case "close_chat":
      if (chat.status === "working" || chat.status === "needs_input")
        return mockCommand<T>("stop", args);
      return undefined as T;
    case "get_agent_defaults":
      return Object.values(defaults) as T;
    case "set_agent_defaults": {
      const settings = args.settings as TurnSettings;
      defaults[settings.agent] = settings;
      localStorage.setItem("code-ui:mock_defaults", JSON.stringify(defaults));
      return undefined as T;
    }
    case "handoff_preview": {
      const target = String(args.targetAgent);
      const session = chat.sessions.find((s) => s.agent === target);
      const missed = chat.messages.filter(
        (m) =>
          m.seq > (session?.syncedSeq ?? 0) &&
          ["user", "assistant", "handoff"].includes(m.role),
      );
      const from =
        missed.filter((m) => m.role === "assistant").at(-1)?.agent ?? null;
      return { count: missed.length, lastFrom: from, from } as T;
    }
    case "list_models":
      return (
        args.agent === "codex"
          ? [
              {
                id: "gpt-6-luna",
                label: "GPT-6-Luna",
                efforts: ["low", "medium", "high", "xhigh", "max"],
                isDefault: true,
              },
            ]
          : args.agent === "agy"
            ? [
                {
                  id: "gemini-3.8-flash-low",
                  label: "Gemini 3.8 Flash (Low)",
                  efforts: ["low", "medium", "high", "xhigh", "max"],
                },
              ]
            : args.agent === "muse"
              ? ["muse-wonder-1.0", "muse-spark-1.3", "muse-spark-1.2"].map(
                  (id, i) => ({
                    id,
                    label: id,
                    isDefault: i === 0,
                    efforts: [
                      "minimal",
                      "low",
                      "medium",
                      "high",
                      "xhigh",
                      "max",
                    ],
                  }),
                )
              : ["fable", "opus", "sonnet", "haiku"].map((id) => ({
                  id,
                  label: id,
                  efforts: ["low", "medium", "high", "xhigh", "max"],
                }))
      ) as T;
    case "scan_native_sessions":
      return undefined as T;
    case "open_native_session":
      throw new Error("Native sessions are available in the desktop app.");
    case "open_external":
      return undefined as T;
    case "agent_status":
      return detectedAgents() as T;
    case "get_settings":
      return structuredClone(configuration) as T;
    case "save_settings": {
      const settings = structuredClone(args.settings as AppSettings);
      if (!settings.projectsRoot.startsWith("/")) throw Error("Choose an absolute projects folder path.");
      configuration.settings = settings;
      configuration.error = null;
      saveConfiguration();
      return structuredClone(configuration) as T;
    }
    case "refresh_agents":
      return detectedAgents((args.settings as AppSettings | null) ?? configuration.settings) as T;
    case "trust_project":
      if (!configuration.settings.trustedProjects.includes(String(args.project))) configuration.settings.trustedProjects.push(String(args.project));
      saveConfiguration();
      return structuredClone(configuration) as T;
    case "get_ui_state":
      return localStorage.getItem(`code-ui:${args.key}`) as T;
    case "set_ui_state":
      localStorage.setItem(`code-ui:${args.key}`, String(args.value));
      return undefined as T;
    case "list_chats":
      return Object.values(chats)
        .filter(
          (c) =>
            !hidden.has(c.id) && (!args.project || c.project === args.project),
        )
        .map((c) => ({
          kind: "app",
          imported: c.messages.some((message) => message.meta.imported),
          unread: c.unread ?? false,
          id: c.id,
          title: c.title,
          project: c.project,
          status: c.status,
          agent: c.activeAgent,
          updatedAt: c.messages.at(-1)?.createdAt ?? 0,
        }))
        .sort((a, b) => b.updatedAt - a.updatedAt) as T;
    case "open_chat":
      if (!chat) throw new Error("Chat not found");
      return structuredClone({ ...chat, unread: chat.unread ?? false }) as T;
    case "send_message": {
      const target = args.target as {
        chatId?: string;
        draft?: { project: string };
      };
      let c = target.chatId ? chats[target.chatId] : undefined;
      if (!c) {
        const id = crypto.randomUUID();
        c = {
          unread: false,
          id,
          project: target.draft!.project,
          title: String(args.text).split("\n")[0].slice(0, 60),
          status: "idle",
          activeAgent: "claude",
          messages: [],
          steps: [],
          interactions: [],
          queue: [],
          sessions: [],
        };
        chats[id] = c;
      }
      if (args.handoffMode === "summary")
        c.messages.push({
          seq: c.messages.length + 1,
          role: "handoff",
          agent: c.activeAgent,
          turnId: null,
          text: "A short handoff summary.",
          meta: {},
          createdAt: Date.now(),
        });
      if (c.status === "working" || c.status === "needs_input") {
        c.queue.push({
          pos: (c.queue.at(-1)?.pos ?? 0) + 1,
          text: String(args.text),
          settings: args.settings as TurnSettings,
        });
        changed(c);
      } else start(c, String(args.text), args.settings as TurnSettings);
      return { chatId: c.id } as T;
    }
    case "respond_interaction": {
      const row = chat.interactions.find((r) => r.id === args.interactionId)!;
      const response = args.response as InteractionResponse;
      row.status =
        response.type === "answers"
          ? "answered"
          : response.type === "dismiss"
            ? "dismissed"
            : response.choiceId === "allow"
              ? "approved"
              : "declined";
      row.answer = JSON.stringify(response);
      chat.status = "working";
      changed(chat);
      timers.set(
        chat.id,
        setTimeout(
          () =>
            finish(
              chat,
              response.type === "answers"
                ? `You chose ${response.answers[0].other || response.answers[0].labels.join(", ")}.`
                : row.status === "approved"
                  ? "The action completed."
                  : "The request was declined.",
            ),
          300,
        ),
      );
      return undefined as T;
    }
    case "stop":
      clearTimeout(timers.get(chatId));
      chat.status = "stopped";
      chat.steps.forEach((s) => {
        if (s.status === "running") {
          s.status = "interrupted";
          s.endedAt = Date.now();
        }
      });
      changed(chat);
      return undefined as T;
    case "retry":
      start(chat, "Continue where you left off.", {
        agent: "claude",
        model: null,
        effort: null,
        permission: "yolo",
      });
      return undefined as T;
    case "resume_queue": {
      const item = chat.queue.shift();
      if (item) start(chat, item.text, item.settings);
      return undefined as T;
    }
    case "unqueue": {
      const index = chat.queue.findIndex((r) => r.pos === args.pos);
      const item = chat.queue.splice(index, 1)[0];
      changed(chat);
      return item.text as T;
    }
    default:
      throw new Error(`Unknown mock command: ${command}`);
  }
}
