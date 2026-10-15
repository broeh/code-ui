import { useState } from "react";
import { Dialog } from "../common/Dialog";
import { createFile, createFolder } from "../../ipc/commands";
import { openFile, refreshFiles } from "../../store/files";
export default function NewFileDialog({
  project,
  dir,
  folder,
  onClose,
}: {
  project: string;
  dir: string;
  folder: boolean;
  onClose: () => void;
}) {
  const [name, setName] = useState(folder ? "untitled" : "untitled.md");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function create() {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      const rel = folder
        ? await createFolder(project, dir, name.trim())
        : await createFile(project, dir, name.trim());
      await refreshFiles();
      if (!folder)
        await openFile(
          { name: name.trim(), rel, isDir: false, size: 0, kind: "text" },
          project,
        );
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      title={folder ? "New folder" : "New file"}
      confirmLabel={folder ? "Create folder" : "Create file"}
      onClose={onClose}
      busy={busy}
      confirmDisabled={!name.trim()}
      onSubmit={() => void create()}
    >
      <p className="new-file-location">{dir || "Project root"}</p>
      <label className="dialog-field">
        Name
        <input
          aria-label={folder ? "Folder name" : "File name"}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      {!folder && (
        <div className="extension-chips">
          {[".md", ".txt"].map((ext) => (
            <button
              type="button"
              key={ext}
              onClick={() => setName(name.replace(/\.[^/.]+$/, "") + ext)}
            >
              {ext}
            </button>
          ))}
        </div>
      )}
      {error && (
        <p className="dialog-error" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}
