import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { X } from "lucide-react";
export function Dialog({
  title,
  children,
  onClose,
  onSubmit,
  confirmLabel = "Save",
  busy = false,
  confirmDisabled = false,
  dismissible = true,
  className = "",
  footerStart,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  onSubmit: () => void;
  confirmLabel?: string;
  busy?: boolean;
  confirmDisabled?: boolean;
  dismissible?: boolean;
  className?: string;
  footerStart?: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    dialog.querySelector<HTMLElement>("input,textarea,button.primary")?.focus();
    return () => dialog.close();
  }, []);
  return (
    <dialog
      className={`dialog ${className}`}
      ref={ref}
      aria-labelledby={id}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy && dismissible) onClose();
      }}
      onClick={(event) => {
        if (dismissible && !busy && event.target === event.currentTarget) {
          const bounds = event.currentTarget.getBoundingClientRect();
          if (
            event.clientX < bounds.left ||
            event.clientX > bounds.right ||
            event.clientY < bounds.top ||
            event.clientY > bounds.bottom
          )
            onClose();
        }
      }}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit();
        }}
      >
        <header>
          <h2 id={id}>{title}</h2>
          {dismissible && <button
            className="icon-button"
            type="button"
            aria-label="Close dialog"
            disabled={busy}
            onClick={onClose}
          >
            <X size={15} />
          </button>}
        </header>
        {children}
        <footer>
          {footerStart}
          {dismissible && <button type="button" disabled={busy} onClick={onClose}>
            Cancel
          </button>}
          <button
            className="primary"
            type="submit"
            disabled={busy || confirmDisabled}
          >
            {confirmLabel}
          </button>
        </footer>
      </form>
    </dialog>
  );
}
export function NameDialog({
  title,
  label,
  initial = "",
  confirmLabel = "Save",
  submit,
  onClose,
}: {
  title: string;
  label: string;
  initial?: string;
  confirmLabel?: string;
  submit: (value: string) => Promise<void>;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function save() {
    if (!value.trim() || busy) return;
    setBusy(true);
    try {
      await submit(value.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      title={title}
      onClose={onClose}
      onSubmit={() => void save()}
      busy={busy}
      confirmDisabled={!value.trim()}
      confirmLabel={confirmLabel}
    >
      <label className="dialog-field">
        {label}
        <input
          value={value}
          aria-label={label}
          onChange={(e) => setValue(e.target.value)}
        />
      </label>
      {error && (
        <p role="alert" className="dialog-error">
          {error}
        </p>
      )}
    </Dialog>
  );
}
