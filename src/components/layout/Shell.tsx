import {
  Fragment,
  lazy,
  Suspense,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  Group,
  Panel,
  Separator,
  useGroupRef,
  usePanelRef,
  type Layout,
} from "react-resizable-panels";
import { Code2, Folder, Power, Terminal, X, Settings } from "lucide-react";
import { getUiState, setUiState } from "../../ipc/commands";
import { subscribe } from "../../ipc/events";
import { agentNames, type ChatStatus } from "../../ipc/types";
import AgentIcon from "../common/AgentIcon";
import { projectName } from "../../lib/format";
import {
  applyChatStatus,
  applyLiveEvent,
  initializeWorkspace,
  refreshChat,
  reportError,
  selectChat,
  useWorkspace,
} from "../../store/chats";
import ProjectsPanel from "../projects/ProjectsPanel";
import ChatList from "../chats/ChatList";
import TabBar from "../tabs/TabBar";
import PanelHeader from "../common/PanelHeader";
import ChatView from "../chat/ChatView";
import StatusLine from "../chat/StatusLine";
import Composer from "../composer/Composer";
import ActivityPanel from "../activity/ActivityPanel";
import TodoPanel from "../todo/TodoPanel";
import { latestTodos } from "../../lib/todos";
import FilesPanel from "../files/FilesPanel";
import PanelMenu from "./PanelMenu";
const EditorPane = lazy(() => import("../editor/EditorPane"));
import { Dialog } from "../common/Dialog";
import {
  useFiles,
  dirty,
  requestEditorAction,
  saveAndContinue,
  discardAndContinue,
  cancelDiscard,
} from "../../store/files";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import SettingsDialog from "../settings/SettingsDialog";
import TrustDialog from "../settings/TrustDialog";

const headerRem = 3.875;
const headerHeight = `${headerRem}rem`;
// Smallest open size of each left panel, and the size used when one opens without a remembered size.
const minRem = { projects: 8, files: 8, chats: 8, activity: 10, todo: 10 };
const openRem = {
  projects: 25,
  files: 20,
  chats: 25.5,
  activity: 16,
  todo: 16,
};
type SideId = keyof typeof minRem;
const sidePanels: { id: SideId; label: string }[] = [
  { id: "projects", label: "Projects" },
  { id: "files", label: "Files" },
  { id: "chats", label: "Chats" },
  { id: "activity", label: "Activity" },
  { id: "todo", label: "To do" },
];
const defaultShown: SideId[] = ["projects", "files", "chats"];
const defaultSize: Partial<Record<SideId, string>> = {
  projects: "31.5%",
  chats: "32%",
  activity: headerHeight,
  todo: headerHeight,
};
function useCollapse() {
  const panel = usePanelRef();
  const [collapsed, setCollapsed] = useState(false);
  return {
    panel,
    collapsed,
    sync: () => setCollapsed(!!panel.current?.isCollapsed()),
  };
}
function SidePanel({
  id,
  state,
  defaultSize,
  children,
}: {
  id: SideId;
  state: ReturnType<typeof useCollapse>;
  defaultSize?: string;
  children: ReactNode;
}) {
  return (
    <Panel
      id={id}
      defaultSize={defaultSize}
      minSize={`${minRem[id]}rem`}
      collapsible
      collapsedSize={headerHeight}
      panelRef={state.panel}
      onResize={state.sync}
    >
      {children}
    </Panel>
  );
}
export default function Shell() {
  const workspace = useWorkspace();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const fileState = useFiles();
  const editorVisible =
    !!workspace.activeTab && fileState.editor?.project === workspace.project;
  const [layouts, setLayouts] = useState<Record<string, Layout> | null>(null);
  const [shown, setShown] = useState<SideId[]>(defaultShown);
  const saved = useRef<Record<string, Layout>>({});
  const toggled = useRef(false);
  const group = useGroupRef();
  const lastSize = useRef<Partial<Record<SideId, number>>>({});
  const side = {
    projects: useCollapse(),
    files: useCollapse(),
    chats: useCollapse(),
    activity: useCollapse(),
    todo: useCollapse(),
  };
  // The library only takes space from the panels below, so a panel above a collapsed
  // one could not open. Share the space out here instead: Files gives and takes first.
  function toggle(id: SideId) {
    const handle = group.current;
    const panel = side[id].panel.current;
    if (!handle || !panel) return;
    const { asPercentage, inPixels } = panel.getSize();
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    const percent = (size: number) => (size * rem * asPercentage) / inPixels;
    const layout = handle.getLayout();
    const next = { ...layout };
    const others = (Object.keys(layout) as SideId[]).filter(
      (other) => other !== id,
    );
    const biggest = (ids: SideId[]) =>
      [...ids].sort(
        (a, b) =>
          Number(b === "files") - Number(a === "files") ||
          layout[b] - layout[a],
      );
    toggled.current = true;
    if (panel.isCollapsed()) {
      const want = lastSize.current[id] ?? percent(openRem[id]);
      let need = want - layout[id];
      for (const other of biggest(others)) {
        const take = Math.min(layout[other] - percent(minRem[other]), need);
        if (take <= 0) continue;
        next[other] -= take;
        need -= take;
        if (need <= 0) break;
      }
      next[id] = want - Math.max(need, 0);
    } else {
      const receiver = biggest(
        others.filter((other) => !side[other].collapsed),
      )[0];
      if (!receiver) return panel.collapse();
      lastSize.current[id] = layout[id];
      const closed = percent(headerRem);
      next[receiver] += layout[id] - closed;
      next[id] = closed;
    }
    handle.setLayout(next);
  }
  useEffect(() => {
    let disposed = false;
    Promise.all([getUiState("panel_sizes"), getUiState("shown_panels")])
      .then(([panels, visible]) => {
        if (!disposed) {
          saved.current = panels ? JSON.parse(panels) : {};
          if (visible) setShown(JSON.parse(visible));
          setLayouts(saved.current);
        }
      })
      .catch((error) => {
        if (!disposed) {
          reportError(error);
          setLayouts({});
        }
      });
    return () => {
      disposed = true;
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const cleanup: (() => void)[] = [];
    const pending = new Set<string>();
    initializeWorkspace().catch((error) => {
      reportError(error);
      useWorkspace.setState({ ready: true });
    });
    function schedule(chatId: string) {
      pending.add(chatId);
      if (timer) return;
      timer = setTimeout(() => {
        timer = undefined;
        const ids = Array.from(pending);
        pending.clear();
        for (const id of ids) refreshChat(id).catch(reportError);
      }, 50);
    }
    subscribe<{ chatId: string }>("focus-chat", ({ chatId }) =>
      selectChat(chatId).catch(reportError),
    )
      .then((fn) => (disposed ? fn() : cleanup.push(fn)))
      .catch(reportError);
    subscribe("native-sessions-updated", () => refreshChat().catch(reportError))
      .then((fn) => (disposed ? fn() : cleanup.push(fn)))
      .catch(reportError);
    subscribe<{
      chatId: string;
      event: {
        type: string;
        message?: string;
        text?: string;
        id?: string;
        title?: string | null;
        outputAppend?: string | null;
      };
    }>("chat-event", (payload) => {
      if (payload.event.type === "error" && payload.event.message)
        reportError(payload.event.message);
      applyLiveEvent(payload.chatId, payload.event);
      if (!["replyDelta", "stepUpdated"].includes(payload.event.type))
        schedule(payload.chatId);
    })
      .then((unlisten) => (disposed ? unlisten() : cleanup.push(unlisten)))
      .catch(reportError);
    subscribe<{ chatId: string; status: ChatStatus; unread: boolean }>(
      "chat-status",
      (payload) => {
        applyChatStatus(payload);
        schedule(payload.chatId);
      },
    )
      .then((unlisten) => (disposed ? unlisten() : cleanup.push(unlisten)))
      .catch(reportError);
    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      cleanup.forEach((fn) => fn());
    };
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested((event) => {
        if (dirty()) {
          event.preventDefault();
          requestEditorAction(async () => {
            useFiles.setState({ editor: null });
            await getCurrentWindow().close();
          }).catch(reportError);
        }
      })
      .then((fn) => (disposed ? fn() : (cleanup = fn)))
      .catch(reportError);
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);
  function persist(id: string, layout: Layout) {
    saved.current = { ...saved.current, [id]: layout };
    setUiState("panel_sizes", JSON.stringify(saved.current)).catch(reportError);
  }
  function showPanel(id: SideId, on: boolean) {
    const next = on ? [...shown, id] : shown.filter((other) => other !== id);
    setShown(next);
    setUiState("shown_panels", JSON.stringify(next)).catch(reportError);
  }
  if (!layouts || !workspace.ready)
    return <div className="startup">Opening workspace…</div>;
  const installedAgents = workspace.agents.filter((status) => status.enabled && status.installed && !status.problem);
  const todos = latestTodos(workspace.snapshot?.steps ?? []);
  const visible = sidePanels.filter((panel) => shown.includes(panel.id));
  const leftLayout = `left:${visible.map((panel) => panel.id).join(",")}`;
  const content: Record<SideId, ReactNode> = {
    projects: (
      <ProjectsPanel
        collapsed={side.projects.collapsed}
        onToggle={() => toggle("projects")}
      />
    ),
    files: (
      <FilesPanel
        collapsed={side.files.collapsed}
        onToggle={() => toggle("files")}
      />
    ),
    chats: (
      <ChatList
        collapsed={side.chats.collapsed}
        onToggle={() => toggle("chats")}
      />
    ),
    activity: (
      <section
        className={`panel ${side.activity.collapsed ? "collapsed" : ""}`}
      >
        <PanelHeader
          collapsed={side.activity.collapsed}
          onToggle={() => toggle("activity")}
        >
          Activity
        </PanelHeader>
        <ActivityPanel />
      </section>
    ),
    todo: (
      <section className={`panel ${side.todo.collapsed ? "collapsed" : ""}`}>
        <PanelHeader
          collapsed={side.todo.collapsed}
          onToggle={() => toggle("todo")}
        >
          To do
          {todos && (
            <span className="count">
              {todos.filter((t) => t.state === "done").length}/{todos.length}
            </span>
          )}
        </PanelHeader>
        <TodoPanel
          todos={todos}
          working={workspace.snapshot?.status === "working"}
        />
      </section>
    ),
  };
  return (
    <div className="app-shell">
      {workspace.appSettings && (settingsOpen || !workspace.appSettings.setupComplete || !!workspace.settingsError) && <SettingsDialog setup={!workspace.appSettings.setupComplete} onClose={() => setSettingsOpen(false)} />}
      {workspace.trustRequest && <TrustDialog />}
      <div className="app-bar">
        <div className="brand">
          <span className="brand-logo">
            <Code2 />
          </span>
          <strong>{workspace.profile === "preview" ? "CODE_UI Preview" : "CODE_UI"}</strong>
        </div>
        {workspace.project && (
          <div className="breadcrumb">
            <Folder />
            <b>{projectName(workspace.project)}</b>
            <span>{workspace.project}</span>
          </div>
        )}
        <div className="app-bar-meta">
          <span className="agents-pill">
            <span className="agent-stack">
              {installedAgents.map((status) => (
                <AgentIcon key={status.agent} agent={status.agent} />
              ))}
            </span>
            <i
              className={
                installedAgents.length ? "" : "warn"
              }
            />
            {installedAgents.length} CLIs detected
          </span>
          <PanelMenu panels={sidePanels} shown={shown} onChange={showPanel} />
          <button className="icon-button" aria-label="Settings" title="Settings" onClick={() => setSettingsOpen(true)}><Settings size={17} /></button>
          {isTauri() && (
            <button
              className="app-exit"
              aria-label="Exit CODE_UI"
              onClick={() => getCurrentWindow().close().catch(reportError)}
            >
              <Power />
              Exit
            </button>
          )}
        </div>
      </div>
      {workspace.error && (
        <div role="alert" className="error-banner">
          {workspace.error}
          <button
            onClick={() => useWorkspace.setState({ error: null })}
            aria-label="Dismiss error"
          >
            <X size={14} />
          </button>
        </div>
      )}
      {fileState.confirmDiscard && (
        <Dialog
          title="Unsaved changes"
          confirmLabel="Save and continue"
          busy={fileState.editor?.saving}
          confirmDisabled={fileState.editor?.conflict}
          onClose={cancelDiscard}
          onSubmit={() => saveAndContinue().catch(reportError)}
        >
          <p>Save changes to {fileState.editor?.rel} before continuing?</p>
          <button
            type="button"
            className="discard-button"
            onClick={() => discardAndContinue().catch(reportError)}
          >
            Discard changes
          </button>
        </Dialog>
      )}
      <Group
        id="columns"
        className="columns"
        defaultLayout={layouts.columns}
        onLayoutChanged={(layout, meta) => {
          if (meta.isUserInteraction) persist("columns", layout);
        }}
      >
        <Panel id="left" defaultSize="17.5%" minSize="280px" maxSize="480px">
          <Group
            key={leftLayout}
            orientation="vertical"
            groupRef={group}
            defaultLayout={saved.current[leftLayout]}
            onLayoutChanged={(layout, meta) => {
              if (meta.isUserInteraction || toggled.current) {
                toggled.current = false;
                persist(leftLayout, layout);
              }
            }}
          >
            {visible.map((panel, index) => (
              <Fragment key={panel.id}>
                {index > 0 && (
                  <Separator className="resize-handle horizontal" />
                )}
                <SidePanel
                  id={panel.id}
                  state={side[panel.id]}
                  defaultSize={defaultSize[panel.id]}
                >
                  {content[panel.id]}
                </SidePanel>
              </Fragment>
            ))}
          </Group>
        </Panel>
        <Separator className="resize-handle" />
        <Panel id="center" minSize="380px">
          <main className="center">
            <TabBar />
            <Group
              id="chat-editor"
              className="chat-editor"
              defaultLayout={editorVisible ? layouts.editor_split : undefined}
              onLayoutChanged={(layout, meta) => {
                if (meta.isUserInteraction) persist("editor_split", layout);
              }}
            >
              <Panel id="chat-pane" defaultSize="50%" minSize="150px">
                <div className="chat-pane">
                  {workspace.activeTab && <StatusLine />}
                  {workspace.snapshot ? (
                    <ChatView
                      key={`chat:${workspace.activeTab}`}
                      snapshot={workspace.snapshot}
                    />
                  ) : (
                    <div className="chat-scroll">
                      <div className="welcome">
                        <div className="welcome-mark">
                          <Terminal size={28} strokeWidth={1.5} />
                        </div>
                        <div className="eyebrow">YOUR FOLDER. YOUR AGENTS.</div>
                        <h1>A place to build.</h1>
                        <p>
                          Keep your projects, conversations and files together.
                          <br />
                          Work with the coding agents you already use.
                        </p>
                        <div className="agent-grid">
                          {workspace.agents.map((status) => (
                            <div
                              className="agent-card"
                              key={status.agent}
                              title={status.path ?? undefined}
                            >
                              <span className={`agent-chip ${status.agent}`}>
                                <AgentIcon agent={status.agent} />
                              </span>
                              <div>
                                <strong>{agentNames[status.agent]}</strong>
                                <small>
                                  {!status.enabled ? "Disabled in Settings" : status.problem ?? status.version ?? "Not detected"}
                                </small>
                              </div>
                              <span
                                className={`health-dot ${status.enabled && status.installed && !status.problem ? "" : "offline"}`}
                              />
                            </div>
                          ))}
                        </div>
                        <div className="welcome-note">
                          <Code2 size={14} />
                          {workspace.activeTab
                            ? "Send a message to start."
                            : "Choose a project or start a new chat."}
                        </div>
                      </div>
                    </div>
                  )}
                  {workspace.activeTab && (
                    <Composer key={`composer:${workspace.activeTab}`} />
                  )}
                </div>
              </Panel>
              {editorVisible && (
                <>
                  <Separator className="resize-handle" />
                  <Panel id="editor-pane" defaultSize="50%" minSize="180px">
                    <Suspense
                      fallback={<div className="startup">Opening editor…</div>}
                    >
                      <EditorPane />
                    </Suspense>
                  </Panel>
                </>
              )}
            </Group>
          </main>
        </Panel>
      </Group>
    </div>
  );
}
