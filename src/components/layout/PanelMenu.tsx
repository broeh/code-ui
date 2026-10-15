import { useEffect, useRef, useState } from "react";
import { Settings } from "lucide-react";
export default function PanelMenu<Id extends string>({
  panels,
  shown,
  onChange,
}: {
  panels: { id: Id; label: string }[];
  shown: Id[];
  onChange: (id: Id, on: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  return (
    <div className="panel-menu" ref={ref}>
      <button
        className="icon-button"
        aria-label="Choose panels"
        title="Choose panels"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Settings size={16} />
      </button>
      {open && (
        <div className="panel-menu-list" role="group" aria-label="Panels">
          {panels.map((panel) => (
            <label key={panel.id}>
              <input
                type="checkbox"
                checked={shown.includes(panel.id)}
                onChange={(e) => onChange(panel.id, e.target.checked)}
              />
              {panel.label}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
