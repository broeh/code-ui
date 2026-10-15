export default function MentionPopover({
  files,
  index,
  onSelect,
}: {
  files: string[];
  index: number;
  onSelect: (path: string) => void;
}) {
  return (
    <div className="mention-popover" role="listbox" aria-label="Mention a file">
      {files.length ? (
        files.map((file, i) => (
          <button
            type="button"
            role="option"
            aria-selected={i === index}
            className={i === index ? "selected" : ""}
            key={file}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onSelect(file)}
          >
            {file}
          </button>
        ))
      ) : (
        <p>No matching files.</p>
      )}
    </div>
  );
}
