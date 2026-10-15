import { beforeEach, expect, it, vi } from "vitest";
import type { FileEntry, TextFile } from "../ipc/types";
vi.mock("../ipc/commands", () => ({
  readTextFile: vi.fn(),
  writeTextFile: vi.fn(),
  listDir: vi.fn().mockResolvedValue([]),
  listAllFiles: vi.fn().mockResolvedValue([]),
  watchProject: vi.fn().mockResolvedValue(undefined),
  setUiState: vi.fn().mockResolvedValue(undefined),
}));
import * as ipc from "../ipc/commands";
import {
  useFiles,
  openFile,
  editText,
  filesChanged,
  saveEditor,
} from "./files";
const entry = (rel: string): FileEntry => ({
  name: rel,
  rel,
  isDir: false,
  size: 10,
  kind: "text",
});
beforeEach(() => {
  vi.clearAllMocks();
  useFiles.setState({
    project: "/project",
    dirs: {},
    expanded: [""],
    editor: null,
    confirmDiscard: false,
    pending: null,
  });
});
it("does not replace the latest opened file with an older delayed read", async () => {
  let finish!: (data: TextFile) => void;
  vi.mocked(ipc.readTextFile).mockImplementation((_project, rel) =>
    rel === "a.md"
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : Promise.resolve({ text: "B", mtime: "2" }),
  );
  const a = openFile(entry("a.md"));
  await openFile(entry("b.md"));
  finish({ text: "A", mtime: "1" });
  await a;
  expect(useFiles.getState().editor?.rel).toBe("b.md");
  expect(useFiles.getState().editor?.text).toBe("B");
});
it("reloads clean buffers and preserves dirty text when the disk changes", async () => {
  vi.mocked(ipc.readTextFile).mockResolvedValue({
    text: "original",
    mtime: "1",
  });
  await openFile(entry("notes.md"));
  vi.mocked(ipc.readTextFile).mockResolvedValue({
    text: "external",
    mtime: "2",
  });
  await filesChanged("/project", [""]);
  expect(useFiles.getState().editor?.text).toBe("external");
  editText("my draft");
  vi.mocked(ipc.readTextFile).mockResolvedValue({
    text: "new external",
    mtime: "3",
  });
  await filesChanged("/project", [""]);
  expect(useFiles.getState().editor?.text).toBe("my draft");
  expect(useFiles.getState().editor?.conflict).toBe(true);
  vi.mocked(ipc.writeTextFile).mockResolvedValue({ conflict: true });
  await saveEditor();
  expect(useFiles.getState().editor?.text).toBe("my draft");
  expect(ipc.writeTextFile).toHaveBeenLastCalledWith(
    "/project",
    "notes.md",
    "my draft",
    "2",
  );
});
it("keeps a renamed expanded folder usable and removes trashed descendants", async () => {
  const { remapTree, removeTree } = await import("./files");
  useFiles.setState({
    expanded: ["", "src", "src/nested"],
    selectedDir: "src/nested",
    dirs: { "": [], src: [], "src/nested": [] },
  });
  remapTree("src", "lib");
  expect(useFiles.getState().expanded).toEqual(["", "lib", "lib/nested"]);
  expect(useFiles.getState().selectedDir).toBe("lib/nested");
  removeTree("lib");
  expect(useFiles.getState().expanded).toEqual([""]);
  expect(useFiles.getState().selectedDir).toBe("");
});
