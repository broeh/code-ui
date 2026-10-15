import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowUp, Clock, Square, X } from "lucide-react";
import {
  useWorkspace,
  reportError,
  send,
  setDraft,
  appendDraft,
  resumeWithTrust,
} from "../../store/chats";
import { agentNames } from "../../ipc/types";
import { stopChat, unqueue } from "../../ipc/commands";
import AgentPicker from "./AgentPicker";
import HandoffChooser from "./HandoffChooser";
import MentionPopover from "./MentionPopover";
import { Fzf } from "fzf";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useFiles } from "../../store/files";
import { insertText, mentionAt, reference } from "../../lib/references";
export default function Composer() {
  const { snapshot, draft, settings, sending, activeTab, agents } =
    useWorkspace();
  const textarea = useRef<HTMLTextAreaElement>(null);
  const { allFiles } = useFiles();
  const [cursor, setCursor] = useState(draft.length);
  const [index, setIndex] = useState(0);
  const [dismissed, setDismissed] = useState(false);
  const [drag, setDrag] = useState(false);
  const selection = useRef({ start: draft.length, end: draft.length });
  const mention = dismissed ? null : mentionAt(draft, cursor);
  const finder = useMemo(() => new Fzf(allFiles), [allFiles]);
  const results = useMemo(
    () =>
      mention
        ? finder
            .find(mention.query)
            .slice(0, 50)
            .map((match) => match.item)
        : [],
    [finder, mention?.query],
  );
  function insert(
    value: string,
    start = selection.current.start,
    end = selection.current.end,
  ) {
    const current = useWorkspace
      .getState()
      .tabs.find((tab) => tab.id === activeTab);
    if (!current) return;
    const next = insertText(current.draft, start, end, value);
    setDraft(next.text, activeTab);
    setCursor(next.cursor);
    selection.current = { start: next.cursor, end: next.cursor };
    setDismissed(true);
    requestAnimationFrame(() => {
      textarea.current?.focus();
      textarea.current?.setSelectionRange(next.cursor, next.cursor);
    });
  }
  function choose(path: string) {
    if (mention) insert(reference(path), mention.start, cursor);
  }
  useEffect(() => {
    const handler = (event: Event) =>
      insert((event as CustomEvent<{ text: string }>).detail.text);
    window.addEventListener("code-ui-insert-reference", handler);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (isTauri())
      getCurrentWindow()
        .onDragDropEvent(({ payload }) => {
          setDrag(payload.type === "enter" || payload.type === "over");
          if (payload.type === "drop" && payload.paths.length)
            insert(payload.paths.map((path) => reference(path, true)).join(""));
        })
        .then((fn) => (disposed ? fn() : (unlisten = fn)))
        .catch(reportError);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("code-ui-insert-reference", handler);
    };
  }, [activeTab]);
  useEffect(() => setIndex(0), [mention?.query]);
  const busy =
    snapshot?.status === "working" || snapshot?.status === "needs_input";
  // Size the box from a hidden copy of the text and only touch the textarea when the
  // height changes. Resizing it on every key made WebKit repaint most of the window.
  const mirror = useRef<HTMLDivElement>(null);
  const height = useRef(0);
  useLayoutEffect(() => {
    if (!mirror.current || !textarea.current) return;
    mirror.current.textContent = `${draft}\u200b`;
    const needed = mirror.current.offsetHeight;
    if (needed !== height.current) {
      height.current = needed;
      textarea.current.style.height = `${needed}px`;
    }
  }, [draft]);
  return (
    <>
      <HandoffChooser />
      {!!snapshot?.queue.length && (
        <div className="queue-list">
          <div className="queue-heading">
            {["failed", "stopped", "interrupted"].includes(snapshot.status) ? (
              <>
                Queue paused{" "}
                <button
                  onClick={() => resumeWithTrust(snapshot.id).catch(reportError)}
                >
                  Resume
                </button>
              </>
            ) : (
              "Queued"
            )}
          </div>
          {snapshot.queue.map((item, i) => (
            <div key={item.pos}>
              <small>{i + 1}</small>
              <span>{item.text}</span>
              <button
                className="icon-button"
                aria-label={`Remove queued message ${i + 1}`}
                onClick={async () => {
                  try {
                    const text = await unqueue(snapshot.id, item.pos);
                    if (activeTab) appendDraft(activeTab, text);
                  } catch (e) {
                    reportError(e);
                  }
                }}
              >
                <X size={13} />
              </button>
            </div>
          ))}
        </div>
      )}
      <div
        className={`composer ${drag ? "drag-over" : ""}`}
        onDragOver={(event) => {
          if (event.dataTransfer.types.includes("application/x-code-ui-file")) {
            event.preventDefault();
            setDrag(true);
          }
        }}
        onDragLeave={() => setDrag(false)}
        onDrop={(event) => {
          const value = event.dataTransfer.getData(
            "application/x-code-ui-file",
          );
          if (value) {
            event.preventDefault();
            setDrag(false);
            const file = JSON.parse(value);
            insert(reference(file.rel));
          }
        }}
      >
        {mention && (
          <MentionPopover files={results} index={index} onSelect={choose} />
        )}
        <div ref={mirror} className="composer-mirror" aria-hidden />
        <textarea
          ref={textarea}
          aria-label={`Message ${agentNames[settings.agent].replace(" Code", "")}`}
          placeholder={`Message ${agentNames[settings.agent].replace(" Code", "")}. @ to mention a file`}
          rows={3}
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
            setCursor(e.target.selectionStart);
            selection.current = {
              start: e.target.selectionStart,
              end: e.target.selectionEnd,
            };
            setDismissed(false);
          }}
          onSelect={(e) => {
            const target = e.currentTarget;
            setCursor(target.selectionStart);
            selection.current = {
              start: target.selectionStart,
              end: target.selectionEnd,
            };
          }}
          onKeyDown={(e) => {
            if (mention) {
              if (e.key === "Escape") {
                e.preventDefault();
                setDismissed(true);
                return;
              }
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((previous) =>
                  Math.max(
                    0,
                    Math.min(
                      results.length - 1,
                      previous + (e.key === "ArrowDown" ? 1 : -1),
                    ),
                  ),
                );
                return;
              }
              if (
                e.key === "Tab" &&
                !e.shiftKey &&
                results[index]
              ) {
                e.preventDefault();
                choose(results[index]);
                return;
              }
            }
            if (
              e.key === "Enter" &&
              e.shiftKey &&
              !e.nativeEvent.isComposing
            ) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="composer-toolbar">
          <AgentPicker />
          <div className="composer-buttons">
            {busy && snapshot && (
              <button
                className="icon-button"
                aria-label="Stop agent"
                onClick={() => stopChat(snapshot.id).catch(reportError)}
              >
                <Square size={15} />
              </button>
            )}
            <button
              disabled={
                !draft.trim() ||
                sending ||
                !agents.some((agent) => agent.agent === settings.agent && agent.enabled && agent.installed && !agent.problem)
              }
              className="send"
              aria-label={busy ? "Queue message" : "Send message"}
              onClick={() => void send()}
            >
              {busy ? <Clock size={16} /> : <ArrowUp size={17} />}
            </button>
          </div>
        </div>
      </div>
      <div className="composer-hint">
        <span>
          Enter for a new line · Shift+Enter to {busy ? "queue" : "send"}
        </span>
        <span>Uses your CLI authentication</span>
      </div>
    </>
  );
}
