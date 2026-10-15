import { useEffect, useMemo, useState } from "react";
import { Fzf } from "fzf";
import {
  Folder,
  File,
  FilePlus,
  FolderPlus,
  MoreHorizontal,
  RefreshCw,
  ListTree,
  Search,
} from "lucide-react";
import PanelHeader from "../common/PanelHeader";
import ContextMenu from "../common/ContextMenu";
import { NameDialog } from "../common/Dialog";
import NewFileDialog from "./NewFileDialog";
import FileTree from "./FileTree";
import {
  getUiState,
  openExternal,
  renamePath,
  reveal,
  trashPath,
} from "../../ipc/commands";
import { subscribe } from "../../ipc/events";
import { projectName } from "../../lib/format";
import { insertReference } from "../../lib/references";
import { reportError, useWorkspace } from "../../store/chats";
import {
  filesChanged,
  openFile,
  refreshFiles,
  requestEditorAction,
  selectFileProject,
  toggleIgnored,
  useFiles,
  remapTree,
  removeTree,
} from "../../store/files";
import type { FileEntry } from "../../ipc/types";
export default function FilesPanel({
  collapsed,
  onToggle,
}: {
  collapsed: boolean;
  onToggle: () => void;
}) {
  const { project, activeTab } = useWorkspace();
  const files = useFiles();
  const [creating, setCreating] = useState<{
    folder: boolean;
    dir: string;
  } | null>(null);
  const [renaming, setRenaming] = useState<FileEntry | null>(null);
  const [menu, setMenu] = useState<{
    entry: FileEntry;
    x: number;
    y: number;
  } | null>(null);
  const [options, setOptions] = useState(false);
  const [search, setSearch] = useState("");
  const finder = useMemo(() => new Fzf(files.allFiles), [files.allFiles]);
  const found = search
    ? finder
        .find(search)
        .slice(0, 200)
        .map((result) => result.item)
    : [];
  useEffect(() => {
    getUiState("show_ignored")
      .then((value) => {
        useFiles.setState({ showIgnored: value === "true" });
        refreshFiles().catch(reportError);
      })
      .catch(reportError);
  }, []);
  useEffect(() => {
    selectFileProject(activeTab ? project : "").catch((error) => {
      if (
        useWorkspace.getState().project === project &&
        useWorkspace.getState().activeTab === activeTab
      )
        reportError(error);
    });
  }, [project, activeTab]);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    subscribe<{ project: string; dirs: string[] }>(
      "files-changed",
      ({ project, dirs }) => filesChanged(project, dirs).catch(reportError),
    )
      .then((fn) => (disposed ? fn() : (cleanup = fn)))
      .catch(reportError);
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);
  const absolute = (entry: FileEntry) => `${files.project}/${entry.rel}`;
  async function rename(entry: FileEntry, name: string) {
    const parent = entry.rel.includes("/")
      ? entry.rel.slice(0, entry.rel.lastIndexOf("/"))
      : "";
    const to = parent ? `${parent}/${name}` : name;
    await renamePath(files.project, entry.rel, to);
    if (entry.isDir) remapTree(entry.rel, to);
    const current = useFiles.getState().editor;
    if (
      current?.project === files.project &&
      (current.rel === entry.rel || current.rel.startsWith(`${entry.rel}/`))
    )
      useFiles.setState({
        editor: { ...current, rel: to + current.rel.slice(entry.rel.length) },
      });
    await refreshFiles();
  }
  async function trash(entry: FileEntry) {
    const action = async () => {
      await trashPath(files.project, entry.rel);
      if (entry.isDir) removeTree(entry.rel);
      const current = useFiles.getState().editor;
      if (
        current?.project === files.project &&
        (current.rel === entry.rel || current.rel.startsWith(`${entry.rel}/`))
      )
        useFiles.setState({ editor: null });
      await refreshFiles();
    };
    const editor = useFiles.getState().editor;
    if (
      editor?.project === files.project &&
      (editor.rel === entry.rel || editor.rel.startsWith(`${entry.rel}/`))
    )
      await requestEditorAction(action);
    else await action();
  }
  return (
    <section className={`panel files-panel ${collapsed ? "collapsed" : ""}`}>
      <PanelHeader
        collapsed={collapsed}
        onToggle={onToggle}
        actions={
          <>
            <button
              className="icon-button"
              aria-label="New file"
              disabled={!activeTab}
              onClick={() =>
                setCreating({ folder: false, dir: files.selectedDir })
              }
            >
              <FilePlus size={14} />
            </button>
            <button
              className="icon-button"
              aria-label="New folder"
              disabled={!activeTab}
              onClick={() =>
                setCreating({ folder: true, dir: files.selectedDir })
              }
            >
              <FolderPlus size={14} />
            </button>
            <button
              className="icon-button"
              aria-label="Refresh files"
              disabled={!activeTab}
              onClick={() => refreshFiles().catch(reportError)}
            >
              <RefreshCw size={14} />
            </button>
            <button
              className="icon-button"
              aria-label="File options"
              onClick={() => setOptions(!options)}
            >
              <MoreHorizontal size={14} />
            </button>
          </>
        }
      >
        Files{" "}
        <span className="header-meta">
          {activeTab ? projectName(project) : ""}
        </span>
      </PanelHeader>
      {options && (
        <label className="file-options">
          <input
            type="checkbox"
            checked={files.showIgnored}
            onChange={(e) => toggleIgnored(e.target.checked).catch(reportError)}
          />
          Show ignored files
        </label>
      )}
      {activeTab && (
        <label className="search-field">
          <Search size={14} />
          <input
            aria-label="Find a file"
            placeholder="Find a file"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
      )}
      {activeTab && search ? (
        <div className="file-tree">
          {found.map((rel) => (
            <button
              key={rel}
              className="file-row"
              title={rel}
              onClick={() =>
                openFile({
                  name: rel.slice(rel.lastIndexOf("/") + 1),
                  rel,
                  isDir: false,
                  size: 0,
                  kind: /\.(png|jpe?g|gif|webp|svg)$/i.test(rel)
                    ? "image"
                    : "text",
                }).catch(reportError)
              }
            >
              <File size={14} />
              <span>{rel}</span>
            </button>
          ))}
          {!found.length && <p className="tree-note">No matching files.</p>}
        </div>
      ) : activeTab ? (
        <div className="file-tree">
          <button
            className={`file-root ${files.selectedDir === "" ? "selected" : ""}`}
            aria-label="Select project root"
            onClick={() => useFiles.setState({ selectedDir: "" })}
          >
            <Folder size={13} />
            {projectName(project)}
          </button>
          <FileTree onContext={(entry, x, y) => setMenu({ entry, x, y })} />
          {files.dirs[""]?.length === 0 && (
            <p className="tree-note">No files yet.</p>
          )}
        </div>
      ) : (
        <div className="small-empty">
          <ListTree size={23} />
          <span>No tab open</span>
          <p>Open a chat to browse its project files.</p>
        </div>
      )}
      {creating && (
        <NewFileDialog
          project={files.project}
          dir={creating.dir}
          folder={creating.folder}
          onClose={() => setCreating(null)}
        />
      )}{" "}
      {renaming && (
        <NameDialog
          title="Rename file or folder"
          label="Name"
          initial={renaming.name}
          onClose={() => setRenaming(null)}
          submit={(name) => rename(renaming, name)}
        />
      )}{" "}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: "Open",
              action: () => openFile(menu.entry).catch(reportError),
            },
            {
              label: "Open with system app",
              action: () =>
                openExternal(absolute(menu.entry)).catch(reportError),
            },
            {
              label: "Show in file manager",
              action: () => reveal(absolute(menu.entry)).catch(reportError),
            },
            {
              label: "Copy path",
              action: () =>
                navigator.clipboard
                  .writeText(absolute(menu.entry))
                  .catch(reportError),
            },
            {
              label: "Copy relative path",
              action: () =>
                navigator.clipboard
                  .writeText(menu.entry.rel)
                  .catch(reportError),
            },
            {
              label: "Insert in message",
              action: () => insertReference(menu.entry.rel),
            },
            { label: "Rename", action: () => setRenaming(menu.entry) },
            {
              label: "Move to trash",
              action: () => trash(menu.entry).catch(reportError),
            },
            {
              label: "New file here",
              action: () =>
                setCreating({
                  folder: false,
                  dir: menu.entry.isDir
                    ? menu.entry.rel
                    : menu.entry.rel.split("/").slice(0, -1).join("/"),
                }),
            },
            {
              label: "New folder here",
              action: () =>
                setCreating({
                  folder: true,
                  dir: menu.entry.isDir
                    ? menu.entry.rel
                    : menu.entry.rel.split("/").slice(0, -1).join("/"),
                }),
            },
          ]}
        />
      )}
    </section>
  );
}
