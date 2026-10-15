import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronUp } from "lucide-react";
import { listModels } from "../../ipc/commands";
import {
  reportError,
  setTurnSettings,
  selectAgent,
  useWorkspace,
} from "../../store/chats";
import {
  agentNames,
  type ModelOption,
  type Permission,
} from "../../ipc/types";
import AgentIcon from "../common/AgentIcon";
export default function AgentPicker() {
  const { settings, agents, activeTab } = useWorkspace();
  const [open, setOpen] = useState(false);
  const [showEffort, setShowEffort] = useState(false);
  useEffect(() => setShowEffort(false), [settings.agent]);
  const [models, setModels] = useState<ModelOption[]>([]);
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open && agents.some((agent) => agent.agent === settings.agent && agent.enabled && agent.installed)) listModels(settings.agent).then(setModels).catch((error) => { setModels([]); reportError(error); });
    else setModels([]);
  }, [open, settings.agent]);
  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (
        !panel.current?.contains(e.target as Node) &&
        !anchor.current?.contains(e.target as Node)
      )
        setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        anchor.current?.focus();
      }
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  const rect = anchor.current?.getBoundingClientRect();
  const selected = settings.model
    ? models.find((m) => m.id === settings.model)
    : models.find((m) => m.isDefault);
  const efforts = selected?.efforts ?? [
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
  ];
  const update = (patch: Partial<typeof settings>) => {
    const tab = useWorkspace
      .getState()
      .tabs.find((tab) => tab.id === activeTab);
    if (tab)
      setTurnSettings({ ...tab.settings, ...patch }, activeTab).catch(
        reportError,
      );
  };
  return (
    <>
      <button
        className="picker"
        ref={anchor}
        aria-label="Agent settings"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <span className={`agent-chip ${settings.agent}`}>
          <AgentIcon agent={settings.agent} />
        </span>
        <span className="picker-label">
          {" "}
          {agentNames[settings.agent].replace(" Code", "")} ·{" "}
          {settings.model ?? "Default"} · {settings.effort ?? "Default"} ·{" "}
          {settings.agent === "agy" && settings.permission === "ask"
            ? "Configured"
            : settings.permission === "yolo" ? "Full access" : settings.permission.charAt(0).toUpperCase() +
              settings.permission.slice(1)}
        </span>
        <ChevronUp size={12} />
      </button>
      {open &&
        rect &&
        createPortal(
          <div
            className="agent-popover"
            ref={panel}
            role="dialog"
            aria-label="Agent settings"
            style={{
              gridTemplateColumns:
                settings.agent === "agy" && !showEffort
                  ? "1.2fr 1fr 1.2fr"
                  : undefined,
              bottom: window.innerHeight - rect.top + 8,
              left: Math.max(
                12,
                Math.min(
                  rect.left,
                  window.innerWidth -
                    Math.min(760, window.innerWidth - 24) -
                    12,
                ),
              ),
              width: Math.min(760, window.innerWidth - 24),
            }}
          >
            <section>
              <h3>Agent</h3>
              {agents.map((agent) => (
                <button
                  key={agent.agent}
                  className={settings.agent === agent.agent ? "selected" : ""}
                  disabled={!agent.enabled || !agent.installed || !!agent.problem}
                  onClick={() =>
                    selectAgent(agent.agent, activeTab).catch(reportError)
                  }
                >
                  <span className={`agent-chip ${agent.agent}`}>
                    <AgentIcon agent={agent.agent} />
                  </span>
                  <span>
                    {agentNames[agent.agent]}
                    <small>
                      {!agent.enabled ? "Disabled in Settings" : agent.problem ?? "Detected"}
                    </small>
                  </span>
                </button>
              ))}
            </section>
            <section>
              <h3>Model</h3>
              <button
                className={settings.model === null ? "selected" : ""}
                onClick={() => update({ model: null })}
              >
                Default
              </button>
              {settings.model &&
                !models.some((m) => m.id === settings.model) && (
                  <button className="selected">{settings.model}</button>
                )}
              {models.map((model) => (
                <button
                  key={model.id}
                  className={settings.model === model.id ? "selected" : ""}
                  onClick={() => update({ model: model.id })}
                >
                  {model.label}
                </button>
              ))}
              {settings.agent === "agy" && (
                <button
                  className="effort-toggle"
                  onClick={() => setShowEffort(!showEffort)}
                >
                  {showEffort ? "Hide effort" : "Show effort"}
                </button>
              )}
            </section>
            {(settings.agent !== "agy" || showEffort) && (
              <section>
                <h3>Effort</h3>
                <button
                  className={settings.effort === null ? "selected" : ""}
                  onClick={() => update({ effort: null })}
                >
                  Default
                </button>
                {efforts.map((effort) => (
                  <button
                    key={effort}
                    className={settings.effort === effort ? "selected" : ""}
                    onClick={() => update({ effort })}
                  >
                    {effort}
                  </button>
                ))}
              </section>
            )}
            <section>
              <h3>Permission</h3>
              {(
                [
                  ["yolo", "Full access", "Run commands and edit files without approval"],
                  ["ask", "Ask", "Ask before actions"],
                  ["plan", "Plan", "Plan before edits"],
                ] as [Permission, string, string][]
              )
                .filter(
                  ([permission]) =>
                    !["codex", "muse"].includes(settings.agent) ||
                    permission !== "plan",
                )
                .map(([permission, label, note]) => (
                  <button
                    key={permission}
                    className={
                      settings.permission === permission ? "selected" : ""
                    }
                    onClick={() => update({ permission })}
                  >
                    {settings.agent === "agy" && permission === "ask"
                      ? "Configured"
                      : label}
                    <small>
                      {settings.agent === "agy" && permission === "ask"
                        ? "Follows CLI settings. Actions needing approval are skipped or fail."
                        : settings.agent === "agy" && permission === "plan"
                          ? "Follows CLI artifact review settings. May continue into implementation."
                          : note}
                    </small>
                  </button>
                ))}
            </section>
          </div>,
          document.body,
        )}
    </>
  );
}
