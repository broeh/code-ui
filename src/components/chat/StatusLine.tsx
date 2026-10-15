import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { stopChat } from "../../ipc/commands";
import { reportError, useWorkspace, retryWithTrust } from "../../store/chats";
import { duration, projectName, tokens } from "../../lib/format";
export default function StatusLine() {
  const { snapshot, project, tabs, activeTab } = useWorkspace();
  const [now, setNow] = useState(Date.now());
  const busy =
    snapshot?.status === "working" || snapshot?.status === "needs_input";
  useEffect(() => {
    if (!busy) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [busy]);
  const start =
    snapshot?.messages.filter((m) => m.role === "user").at(-1)?.createdAt ??
    now;
  const preparing = tabs.find((tab) => tab.id === activeTab)?.preparingHandoff;
  const status = snapshot?.status ?? "idle";
  const session = snapshot?.sessions.find(
    (s) => s.agent === snapshot.activeAgent,
  );
  const used = session?.contextUsed ?? null;
  const limit = session?.contextWindow ?? null;
  const percent =
    used !== null && limit
      ? Math.min(100, Math.round((used / limit) * 100))
      : null;
  const level =
    percent === null
      ? ""
      : percent >= 90
        ? "high"
        : percent >= 70
          ? "warn"
          : "";
  const tone = preparing
    ? "working"
    : {
        idle: "ok",
        working: "working",
        needs_input: "needs",
        failed: "failed",
        stopped: "muted",
        interrupted: "muted",
      }[status];
  return (
    <header className="chat-header">
      <div className="chat-header-inner">
        <h2>{snapshot?.title ?? "New chat"}</h2>
        <div className="chat-header-meta">
          <button
            className={`status-toggle pill ${tone}`}
            onClick={() =>
              useWorkspace.setState({
                selectedStep: snapshot?.steps.at(-1)?.id ?? null,
              })
            }
          >
            {status === "working" ? (
              <LoaderCircle className="spinner" />
            ) : (
              <span className="health-dot" />
            )}
            {preparing
              ? "Preparing handoff"
              : status === "working"
                ? `Working · ${duration(Math.max(0, now - start))}`
                : status === "needs_input"
                  ? "Needs your input"
                  : status === "idle"
                    ? "Ready"
                    : status.charAt(0).toUpperCase() + status.slice(1)}
          </button>
          {busy && snapshot ? (
            <button
              className="stop"
              onClick={() => stopChat(snapshot.id).catch(reportError)}
            >
              Stop
            </button>
          ) : snapshot &&
            ["failed", "stopped", "interrupted"].includes(status) ? (
            <button onClick={() => retryWithTrust(snapshot.id).catch(reportError)}>
              Retry
            </button>
          ) : null}
          <span className="status-path">{projectName(project)}</span>
          {used !== null && (
            <span
              className={`context-meter ${level}`}
              title={`${used.toLocaleString()}${limit ? ` of ${limit.toLocaleString()}` : ""} tokens in context${level ? ". Consider starting a new chat." : ""}`}
            >
              {percent !== null && (
                <span className="context-bar">
                  <i style={{ width: `${percent}%` }} />
                </span>
              )}
              Context {tokens(used)}
              {limit ? ` / ${tokens(limit)} · ${percent}%` : ""}
            </span>
          )}
        </div>
      </div>
    </header>
  );
}
