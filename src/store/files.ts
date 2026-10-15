import { create } from "zustand";
import * as ipc from "../ipc/commands";
import type { EditorFile, FileEntry } from "../ipc/types";
import { reportError } from "./chats";
interface FilesState {
  project: string;
  dirs: Record<string, FileEntry[]>;
  expanded: string[];
  selectedDir: string;
  showIgnored: boolean;
  allFiles: string[];
  editor: EditorFile | null;
  confirmDiscard: boolean;
  pending: (() => Promise<void>) | null;
}
export const useFiles = create<FilesState>(() => ({
  project: "",
  dirs: {},
  expanded: [""],
  selectedDir: "",
  showIgnored: false,
  allFiles: [],
  editor: null,
  confirmDiscard: false,
  pending: null,
}));
export function dirty() {
  const editor = useFiles.getState().editor;
  return !!editor && editor.kind === "text" && editor.text !== editor.savedText;
}
export async function loadDir(rel: string) {
  const { project, showIgnored } = useFiles.getState();
  if (!project) return;
  const rows = await ipc.listDir(project, rel, showIgnored);
  if (
    useFiles.getState().project === project &&
    useFiles.getState().showIgnored === showIgnored
  )
    useFiles.setState({ dirs: { ...useFiles.getState().dirs, [rel]: rows } });
}
export async function refreshFiles(dirs?: string[]) {
  const { project, expanded } = useFiles.getState();
  if (!project) return;
  await Promise.all(
    expanded.filter((dir) => !dirs || dirs.includes(dir)).map(loadDir),
  );
  const files = await ipc.listAllFiles(project);
  if (useFiles.getState().project === project)
    useFiles.setState({ allFiles: files });
}
export async function selectFileProject(project: string) {
  if (useFiles.getState().project === project) return;
  useFiles.setState({
    project,
    dirs: {},
    expanded: [""],
    selectedDir: "",
    allFiles: [],
  });
  await ipc.watchProject(project);
  if (useFiles.getState().project !== project) {
    await ipc.watchProject(useFiles.getState().project);
    return;
  }
  if (project) {
    await refreshFiles();
    await filesChanged(project, []);
  }
}
export async function toggleDir(rel: string) {
  const { expanded } = useFiles.getState();
  useFiles.setState({
    selectedDir: rel,
    expanded: expanded.includes(rel)
      ? expanded.filter((dir) => dir !== rel)
      : [...expanded, rel],
  });
  if (!expanded.includes(rel)) await loadDir(rel);
}
export async function toggleIgnored(showIgnored: boolean) {
  useFiles.setState({ showIgnored });
  await ipc.setUiState("show_ignored", JSON.stringify(showIgnored));
  await refreshFiles();
}
export async function requestEditorAction(action: () => Promise<void>) {
  if (dirty()) {
    useFiles.setState({ confirmDiscard: true, pending: action });
    return;
  }
  await action();
}
export async function saveAndContinue() {
  await saveEditor();
  if (!dirty() && !useFiles.getState().editor?.conflict)
    await discardAndContinue();
}
export async function discardAndContinue() {
  const pending = useFiles.getState().pending;
  useFiles.setState({ confirmDiscard: false, pending: null });
  if (pending) await pending();
}
export function cancelDiscard() {
  useFiles.setState({ confirmDiscard: false, pending: null });
}
let openRequest = 0;
export async function openFile(
  entry: FileEntry,
  project = useFiles.getState().project,
) {
  if (entry.isDir) {
    await toggleDir(entry.rel);
    return;
  }
  if (entry.kind === "other") {
    await ipc.openExternal(`${project}/${entry.rel}`);
    return;
  }
  const request = ++openRequest;
  await requestEditorAction(async () => {
    if (entry.kind === "image") {
      const imageUrl = await ipc.readImageFile(project, entry.rel);
      if (request !== openRequest) return;
      useFiles.setState({
        editor: {
          project,
          rel: entry.rel,
          kind: "image",
          imageUrl,
          text: "",
          savedText: "",
          mtime: "",
          conflict: false,
          preview: false,
          saving: false,
        },
      });
      return;
    }
    const data = await ipc.readTextFile(project, entry.rel);
    if ("error" in data) {
      await ipc.openExternal(`${project}/${entry.rel}`);
      return;
    }
    if (request !== openRequest) return;
    useFiles.setState({
      editor: {
        project,
        rel: entry.rel,
        kind: "text",
        text: data.text,
        savedText: data.text,
        mtime: data.mtime,
        conflict: false,
        preview: false,
        saving: false,
      },
    });
  });
}
export async function closeEditor() {
  await requestEditorAction(async () => {
    useFiles.setState({ editor: null });
  });
}
export function editText(text: string) {
  const editor = useFiles.getState().editor;
  if (editor) useFiles.setState({ editor: { ...editor, text } });
}
export async function saveEditor(overwrite = false) {
  const editor = useFiles.getState().editor;
  if (!editor || editor.kind !== "text" || editor.saving) return;
  useFiles.setState({ editor: { ...editor, saving: true } });
  try {
    const result = await ipc.writeTextFile(
      editor.project,
      editor.rel,
      editor.text,
      overwrite ? null : editor.mtime,
    );
    const current = useFiles.getState().editor;
    if (current?.rel !== editor.rel || current.project !== editor.project)
      return;
    if ("conflict" in result)
      useFiles.setState({
        editor: { ...current, conflict: true, saving: false },
      });
    else {
      useFiles.setState({
        editor: {
          ...current,
          mtime: result.mtime,
          savedText: editor.text,
          conflict: false,
          saving: false,
        },
      });
      await refreshFiles();
    }
  } catch (error) {
    const current = useFiles.getState().editor;
    if (current) useFiles.setState({ editor: { ...current, saving: false } });
    throw error;
  }
}
export async function reloadEditor() {
  const editor = useFiles.getState().editor;
  if (!editor || editor.kind !== "text") return;
  const data = await ipc.readTextFile(editor.project, editor.rel);
  if ("error" in data) throw Error("The file is no longer a text file.");
  useFiles.setState({
    editor: {
      ...editor,
      text: data.text,
      savedText: data.text,
      mtime: data.mtime,
      conflict: false,
    },
  });
}
export async function filesChanged(project: string, dirs: string[]) {
  if (useFiles.getState().project === project) await refreshFiles(dirs);
  const editor = useFiles.getState().editor;
  if (editor?.project !== project || editor.kind !== "text" || editor.saving)
    return;
  try {
    const data = await ipc.readTextFile(project, editor.rel);
    if ("error" in data || data.mtime === editor.mtime) return;
    const current = useFiles.getState().editor;
    if (current?.project !== project || current.rel !== editor.rel) return;
    if (current.text !== current.savedText)
      useFiles.setState({ editor: { ...current, conflict: true } });
    else
      useFiles.setState({
        editor: {
          ...current,
          text: data.text,
          savedText: data.text,
          mtime: data.mtime,
          conflict: false,
        },
      });
  } catch (error) {
    if (dirty()) {
      const current = useFiles.getState().editor;
      if (current)
        useFiles.setState({ editor: { ...current, conflict: true } });
    } else reportError(error);
  }
}
export function remapTree(from: string, to: string) {
  const state = useFiles.getState();
  const rename = (rel: string) =>
    rel === from || rel.startsWith(`${from}/`)
      ? to + rel.slice(from.length)
      : rel;
  useFiles.setState({
    expanded: state.expanded.map(rename),
    selectedDir: rename(state.selectedDir),
    dirs: Object.fromEntries(
      Object.entries(state.dirs).map(([rel, entries]) => [
        rename(rel),
        entries,
      ]),
    ),
  });
}
export function removeTree(rel: string) {
  const state = useFiles.getState();
  const removed = (path: string) => path === rel || path.startsWith(`${rel}/`);
  useFiles.setState({
    expanded: state.expanded.filter((path) => !removed(path)),
    selectedDir: removed(state.selectedDir)
      ? rel.split("/").slice(0, -1).join("/")
      : state.selectedDir,
    dirs: Object.fromEntries(
      Object.entries(state.dirs).filter(([path]) => !removed(path)),
    ),
  });
}
