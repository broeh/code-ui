import { useState, type CSSProperties } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderPlus,
  LoaderCircle,
  Pin,
  Plus,
  Search,
} from "lucide-react";
import PanelHeader from "../common/PanelHeader";
import ContextMenu from "../common/ContextMenu";
import { NameDialog } from "../common/Dialog";
import {
  addFolder,
  createProject,
  hideProject,
  openExternal,
  setPinned,
} from "../../ipc/commands";
import {
  newChat,
  refreshChat,
  reportError,
  selectProject,
  useWorkspace,
} from "../../store/chats";
import { projectColor, relativeTime } from "../../lib/format";
import type { ProjectRow } from "../../ipc/types";
export default function ProjectsPanel({
  collapsed,
  onToggle,
}: {
  collapsed: boolean;
  onToggle: () => void;
}) {
  const { projects, project } = useWorkspace();
  const [search, setSearch] = useState("");
  const [dialog, setDialog] = useState<"create" | "add" | null>(null);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    project: ProjectRow;
  } | null>(null);
  async function add(path: string) {
    const canonical = await addFolder(path);
    await refreshChat();
    await selectProject(canonical);
  }
  async function chooseFolder() {
    if (!isTauri()) {
      setDialog("add");
      return;
    }
    const path = await open({
      title: "Add a project folder",
      directory: true,
      multiple: false,
    });
    if (path) await add(path);
  }
  const visible = projects.filter((row) =>
    `${row.name} ${row.path}`.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <section className={`panel projects-panel ${collapsed ? "collapsed" : ""}`}>
      <PanelHeader
        collapsed={collapsed}
        onToggle={onToggle}
        actions={
          <>
            <button
              className="icon-button"
              aria-label="New project"
              title="New project"
              onClick={() => setDialog("create")}
            >
              <Plus size={16} />
            </button>
            <button
              className="icon-button"
              aria-label="Add other folder"
              title="Add other folder"
              onClick={() => chooseFolder().catch(reportError)}
            >
              <FolderPlus size={16} />
            </button>
          </>
        }
      >
        Projects <span className="count">{projects.length}</span>
      </PanelHeader>
      <label className="search-field">
        <Search size={14} />
        <input
          aria-label="Find a project"
          placeholder="Find a project"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </label>
      <div className="project-list">
        {visible.map((row) => (
          <div
            className={`project-entry ${row.path === project ? "selected" : ""}`}
            key={row.path}
            title={row.path}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, project: row });
            }}
          >
            <button
              className="project-main"
              onClick={() => selectProject(row.path).catch(reportError)}
            >
              <span
                className="project-avatar"
                style={{ "--c": projectColor(row.name) } as CSSProperties}
              >
                {row.name.charAt(0).toUpperCase()}
              </span>
              <span>{row.name}</span>
              {row.working > 0 && (
                <span className="working-count">
                  <LoaderCircle size={11} className="spinner" />
                  {row.working}
                </span>
              )}
              {row.needsInput > 0 && (
                <span className="needs-count">{row.needsInput}</span>
              )}
              <small>{relativeTime(row.lastActivity)}</small>
            </button>
            <div className="row-actions">
              <button
                className={`icon-button ${row.pinned ? "pinned" : ""}`}
                aria-label={`${row.pinned ? "Unpin" : "Pin"} ${row.name}`}
                onClick={() =>
                  setPinned(row.path, !row.pinned)
                    .then(() => refreshChat())
                    .catch(reportError)
                }
              >
                <Pin size={12} />
              </button>
              <button
                className="icon-button"
                aria-label={`New chat in ${row.name}`}
                onClick={() => newChat(row.path).catch(reportError)}
              >
                <Plus size={13} />
              </button>
            </div>
          </div>
        ))}
      </div>
      {!visible.length && (
        <p className="panel-note">
          {projects.length
            ? "No matching projects."
            : "Create a project or add a folder to begin."}
        </p>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: menu.project.pinned ? "Unpin" : "Pin",
              action: () =>
                setPinned(menu.project.path, !menu.project.pinned)
                  .then(() => refreshChat())
                  .catch(reportError),
            },
            {
              label: "Open in file manager",
              action: () => openExternal(menu.project.path).catch(reportError),
            },
            {
              label: "Hide from list",
              action: () =>
                hideProject(menu.project.path)
                  .then(() => refreshChat())
                  .catch(reportError),
            },
          ]}
        />
      )}{" "}
      {dialog && (
        <NameDialog
          title={dialog === "create" ? "New project" : "Add other folder"}
          label={dialog === "create" ? "Project name" : "Folder path"}
          confirmLabel={dialog === "create" ? "Create project" : "Add folder"}
          onClose={() => setDialog(null)}
          submit={
            dialog === "create"
              ? async (name) => {
                  const path = await createProject(name);
                  await refreshChat();
                  await newChat(path);
                }
              : add
          }
        />
      )}
    </section>
  );
}
