import { ChevronDown, ChevronRight, File, Folder, Image } from "lucide-react";
import type { FileEntry } from "../../ipc/types";
import { useFiles, openFile, toggleDir } from "../../store/files";
import { reportError } from "../../store/chats";
export default function FileTree({
  dir = "",
  depth = 0,
  onContext,
}: {
  dir?: string;
  depth?: number;
  onContext: (entry: FileEntry, x: number, y: number) => void;
}) {
  const { dirs, expanded, selectedDir, editor, project } = useFiles();
  const entries = dirs[dir];
  if (!entries)
    return (
      <p className="tree-note" style={{ paddingLeft: 14 + depth * 14 }}>
        Loading…
      </p>
    );
  return (
    <>
      {entries.map((entry) => (
        <div key={entry.rel}>
          <button
            className={`file-row ${entry.isDir ? (selectedDir === entry.rel ? "selected" : "") : editor?.project === project && editor.rel === entry.rel ? "selected" : ""}`}
            style={{ paddingLeft: 8 + depth * 14 }}
            title={entry.rel}
            data-file={entry.rel}
            draggable
            onDragStart={(event) => {
              event.dataTransfer.setData(
                "application/x-code-ui-file",
                JSON.stringify({ project, rel: entry.rel }),
              );
              event.dataTransfer.effectAllowed = "copy";
            }}
            onClick={() =>
              entry.isDir
                ? toggleDir(entry.rel).catch(reportError)
                : openFile(entry).catch(reportError)
            }
            onContextMenu={(e) => {
              e.preventDefault();
              onContext(entry, e.clientX, e.clientY);
            }}
          >
            {entry.isDir ? (
              expanded.includes(entry.rel) ? (
                <ChevronDown size={12} />
              ) : (
                <ChevronRight size={12} />
              )
            ) : (
              <span className="tree-indent" />
            )}
            {entry.isDir ? (
              <Folder size={14} />
            ) : entry.kind === "image" ? (
              <Image size={14} />
            ) : (
              <File size={14} />
            )}
            <span>{entry.name}</span>
          </button>
          {entry.isDir && expanded.includes(entry.rel) && (
            <FileTree dir={entry.rel} depth={depth + 1} onContext={onContext} />
          )}
        </div>
      ))}
      {entries.length === 2000 && (
        <p className="tree-note">Showing the first 2,000 entries.</p>
      )}
    </>
  );
}
