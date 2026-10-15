import { useEffect, useRef, useState } from "react";
import { LoaderCircle, Plus, X } from "lucide-react";
import {
  activateTab,
  closeTab,
  moveTab,
  newChat,
  reportError,
  useWorkspace,
} from "../../store/chats";
import { Dialog } from "../common/Dialog";
import { agentNames, type ChatTab } from "../../ipc/types";
import AgentIcon from "../common/AgentIcon";
import { projectName } from "../../lib/format";
export default function TabBar() {
  const { tabs, activeTab, project } = useWorkspace();
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    Array.from(
      root.current?.querySelectorAll<HTMLElement>('[role="tab"]') ?? [],
    )
      .find((element) => element.dataset.tab === activeTab)
      ?.closest(".tab")
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeTab, tabs.length]);
  const [closing, setClosing] = useState<ChatTab | null>(null);
  const [busy, setBusy] = useState(false);
  function requestClose(tab: ChatTab) {
    if (tab.status === "working" || tab.status === "needs_input" || tab.sending)
      setClosing(tab);
    else closeTab(tab.id).catch(reportError);
  }
  useEffect(() => {
    function key(event: KeyboardEvent) {
      if (
        !event.ctrlKey ||
        event.altKey ||
        document.querySelector("dialog[open]")
      )
        return;
      const key = event.key.toLowerCase();
      if (key === "t") {
        event.preventDefault();
        newChat().catch(reportError);
      } else if (key === "w") {
        event.preventDefault();
        const tab = tabs.find((t) => t.id === activeTab);
        if (tab) requestClose(tab);
      } else if (key === "tab" && tabs.length) {
        event.preventDefault();
        const current = Math.max(
          0,
          tabs.findIndex((t) => t.id === activeTab),
        );
        activateTab(
          tabs[
            (current + (event.shiftKey ? -1 : 1) + tabs.length) % tabs.length
          ].id,
        ).catch(reportError);
      } else if (/^[1-9]$/.test(key) && tabs[Number(key) - 1]) {
        event.preventDefault();
        activateTab(tabs[Number(key) - 1].id).catch(reportError);
      }
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [tabs, activeTab]);
  async function confirmClose() {
    if (!closing || busy) return;
    setBusy(true);
    try {
      await closeTab(closing.id);
      setClosing(null);
    } catch (e) {
      reportError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <div className="tab-bar" ref={root} role="tablist" aria-label="Chats">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            data-tab={tab.id}
            className={tab.id === activeTab ? "tab active" : "tab"}
            title={`${tab.project}${tab.snapshot ? ` · ${tab.snapshot.title}` : ""}`}
            onMouseDown={(e) => {
              if (e.button === 1) e.preventDefault();
            }}
            onAuxClick={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                requestClose(tab);
              }
            }}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData("application/x-code-ui-tab", tab.id);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes("application/x-code-ui-tab"))
                e.preventDefault();
            }}
            onDrop={(e) => {
              e.preventDefault();
              const from = e.dataTransfer.getData("application/x-code-ui-tab");
              if (from) moveTab(from, tab.id).catch(reportError);
            }}
          >
            <button
              type="button"
              role="tab"
              data-tab={tab.id}
              aria-selected={tab.id === activeTab}
              tabIndex={tab.id === activeTab ? 0 : -1}
              className="tab-main"
              onClick={() => activateTab(tab.id).catch(reportError)}
            >
              <span className={`agent-chip ${tab.settings.agent}`}>
                <AgentIcon agent={tab.settings.agent} />
              </span>
              <span className="tab-label">
                {projectName(tab.project)} ·{" "}
                {agentNames[tab.settings.agent].replace(" Code", "")}
              </span>
              {tab.status === "working" || tab.sending ? (
                <LoaderCircle size={12} className="spinner" />
              ) : tab.status === "needs_input" ? (
                <span className="tab-dot needs" />
              ) : tab.unread ? (
                <span className="tab-dot unread" />
              ) : null}
            </button>
            <button
              className="icon-button"
              aria-label={`Close ${projectName(tab.project)} tab`}
              onClick={(e) => {
                e.stopPropagation();
                requestClose(tab);
              }}
            >
              <X size={13} />
            </button>
          </div>
        ))}
        <button
          className="icon-button new-tab"
          aria-label="New tab"
          disabled={!project}
          onClick={() => newChat().catch(reportError)}
        >
          <Plus size={16} />
        </button>
      </div>
      {closing && (
        <Dialog
          title="Close working tab?"
          confirmLabel="Stop and close"
          onClose={() => setClosing(null)}
          onSubmit={() => void confirmClose()}
          busy={busy}
        >
          <p>
            {agentNames[closing.settings.agent].replace(" Code", "")} is still
            working. Stop it and close the tab?
          </p>
        </Dialog>
      )}
    </>
  );
}
