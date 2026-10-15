import type { ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
export default function PanelHeader({
  children,
  actions,
  collapsed,
  onToggle,
}: {
  children: ReactNode;
  actions?: ReactNode;
  collapsed?: boolean;
  onToggle?: () => void;
}) {
  return (
    <header className="panel-header">
      <span>
        {onToggle && (
          <button
            className="icon-button collapse-toggle"
            aria-label={collapsed ? "Expand panel" : "Collapse panel"}
            aria-expanded={!collapsed}
            onClick={onToggle}
          >
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
          </button>
        )}
        {children}
      </span>
      {!collapsed && <div className="panel-actions">{actions}</div>}
    </header>
  );
}
