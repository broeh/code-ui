import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  LoaderCircle,
  User,
} from "lucide-react";
import { agentNames, type ChatSnapshot, type Step } from "../../ipc/types";
import AgentIcon from "../common/AgentIcon";
import { useWorkspace } from "../../store/chats";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import {
  commentaryText,
  currentTool,
  mergeSteps,
  splitSteps,
} from "../../lib/stepMerge";
import { duration, time } from "../../lib/format";
import Markdown from "../common/Markdown";
import InteractionCard from "./InteractionCard";
function Steps({
  steps,
  live,
  working,
  now,
  turnKey,
  open,
  setOpen,
}: {
  steps: Step[];
  live: boolean;
  working: boolean;
  now: number;
  turnKey: string;
  open: Record<string, boolean>;
  setOpen: (key: string, open: boolean) => void;
}) {
  const { tools, commentary } = splitSteps(steps);
  const total = tools.length + commentary.length;
  if (!live && !total) return null;
  const toolRunning = tools.some((s) => s.status === "running");
  const toolsKey = `${turnKey}:tools`;
  const body = (
    <>
      {live && (
        <div className="step-line thinking">
          {working && !toolRunning ? (
            <LoaderCircle className="spinner" />
          ) : (
            <Brain />
          )}
          <span>Thinking</span>
        </div>
      )}
      {tools.length > 0 && (
        <>
          <button
            className="step-fold"
            onClick={() => setOpen(toolsKey, !open[toolsKey])}
            aria-expanded={!!open[toolsKey]}
          >
            {open[toolsKey] ? <ChevronDown /> : <ChevronRight />}
            <span className="fold-label">
              {live
                ? currentTool(tools)?.title
                : `${tools.length} ${tools.length === 1 ? "tool call" : "tool calls"}`}
            </span>
            {working && toolRunning && <LoaderCircle className="spinner" />}
          </button>
          {open[toolsKey] && (
            <div className="tool-calls">
              {mergeSteps(tools).map((group) => (
                <button
                  className="step-line"
                  key={group.steps[0].id}
                  onClick={() =>
                    useWorkspace.setState({ selectedStep: group.steps[0].id })
                  }
                >
                  {group.steps.some((s) => s.status === "running") ? (
                    <LoaderCircle className="spinner" />
                  ) : (
                    <Check />
                  )}
                  <span>{group.title}</span>
                </button>
              ))}
            </div>
          )}
        </>
      )}
      {commentary.map((step) => (
        <div className="commentary" key={step.id}>
          <Markdown text={commentaryText(step)} />
        </div>
      ))}
    </>
  );
  if (live) return <div className="turn-steps">{body}</div>;
  const end = Math.max(...steps.map((s) => s.endedAt ?? now));
  const start = Math.min(...steps.map((s) => s.startedAt));
  return (
    <div className="turn-steps">
      <button
        className="step-fold"
        onClick={() => setOpen(turnKey, !open[turnKey])}
        aria-expanded={!!open[turnKey]}
      >
        {open[turnKey] ? <ChevronDown /> : <ChevronRight />}
        {total} {total === 1 ? "step" : "steps"} · {duration(end - start)}
      </button>
      {open[turnKey] && body}
    </div>
  );
}
export default function ChatView({ snapshot }: { snapshot: ChatSnapshot }) {
  const { tabs, activeTab } = useWorkspace();
  const current = tabs.find((tab) => tab.id === activeTab);
  const liveReply = current?.liveReply ?? "";
  const scroll = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  const [expandedTurns, setExpandedTurns] = useState<Record<string, boolean>>(
    {},
  );
  const pending = new Set(
    snapshot.interactions
      .filter((row) => row.status === "pending")
      .map((row) => row.turnId),
  );
  const virtualizer = useVirtualizer({
    count: snapshot.messages.length,
    getScrollElement: () => scroll.current,
    getItemKey: (index) => snapshot.messages[index].seq,
    estimateSize: (index) =>
      snapshot.messages[index].role === "assistant" ? 280 : 120,
    overscan: 6,
    gap: 26,
    useFlushSync: false,
    useAnimationFrameWithResizeObserver: true,
    rangeExtractor: (range) =>
      Array.from(
        new Set([
          ...defaultRangeExtractor(range),
          ...snapshot.messages.flatMap((message, index) =>
            pending.has(message.turnId ?? "") ? [index] : [],
          ),
        ]),
      ).sort((a, b) => a - b),
  });
  const [now, setNow] = useState(Date.now());
  const live =
    snapshot.status === "working" || snapshot.status === "needs_input";
  useEffect(() => {
    if (!live) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [live]);
  const total = virtualizer.getTotalSize();
  useLayoutEffect(() => {
    if (follow.current && scroll.current)
      scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [
    snapshot.messages.length,
    snapshot.steps.length,
    snapshot.interactions.length,
    snapshot.status,
    liveReply,
    total,
  ]);
  return (
    <div
      className="chat-scroll conversation"
      ref={scroll}
      onScroll={() => {
        if (scroll.current)
          follow.current =
            scroll.current.scrollHeight -
              scroll.current.scrollTop -
              scroll.current.clientHeight <
            90;
      }}
    >
      <div className="message-column">
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualizer.getVirtualItems().map((row) => {
            const message = snapshot.messages[row.index];
            return (
              <div
                key={row.key}
                data-index={row.index}
                ref={virtualizer.measureElement}
                className={`message ${message.role}`}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${row.start}px)`,
                }}
              >
                {message.role === "user" && (
                  <>
                    <div className="message-label">
                      <span className="user-avatar">
                        <User />
                      </span>
                      <strong>You</strong>
                      <small>{time(message.createdAt)}</small>
                    </div>
                    <p className="user-text">{message.text}</p>
                    {message.meta.handoff && (
                      <details className="handoff-chip">
                        <summary>
                          handoff · {message.meta.handoff.mode} ·{" "}
                          {message.meta.handoff.count} messages
                        </summary>
                        <pre>{message.meta.handoff.context}</pre>
                      </details>
                    )}
                    <Steps
                      steps={snapshot.steps.filter(
                        (s) => s.turnId === message.turnId,
                      )}
                      live={
                        live &&
                        message.turnId ===
                          snapshot.messages
                            .filter((m) => m.role === "user")
                            .at(-1)?.turnId
                      }
                      working={snapshot.status === "working"}
                      now={now}
                      turnKey={message.turnId ?? String(message.seq)}
                      open={expandedTurns}
                      setOpen={(key, open) =>
                        setExpandedTurns((current) => ({
                          ...current,
                          [key]: open,
                        }))
                      }
                    />
                    {snapshot.interactions
                      .filter((r) => r.turnId === message.turnId)
                      .map((row) => (
                        <InteractionCard row={row} key={row.id} />
                      ))}
                  </>
                )}
                {message.role === "assistant" && (
                  <>
                    <div className="message-label">
                      <span
                        className={`agent-chip ${message.agent ?? "claude"}`}
                      >
                        <AgentIcon agent={message.agent ?? "claude"} />
                      </span>
                      <strong>
                        {agentNames[message.agent ?? "claude"].replace(
                          " Code",
                          "",
                        )}
                      </strong>
                      <small>
                        {message.meta.model ?? "Default"} · worked{" "}
                        {duration(message.meta.durationMs ?? 0)}
                      </small>
                      <button
                        className="copy-reply icon-button"
                        aria-label="Copy reply"
                        onClick={() =>
                          navigator.clipboard
                            .writeText(message.text)
                            .catch(() => {})
                        }
                      >
                        <Copy size={13} />
                      </button>
                    </div>
                    <Markdown text={message.text} />
                  </>
                )}
                {message.role === "handoff" && (
                  <details className="handoff-note">
                    <summary>
                      Handoff note from{" "}
                      {agentNames[message.agent ?? "claude"].replace(
                        " Code",
                        "",
                      )}
                    </summary>
                    <Markdown text={message.text} />
                  </details>
                )}
                {message.role === "notice" && (
                  <p className="notice-text">{message.text}</p>
                )}
              </div>
            );
          })}
        </div>
        {snapshot.interactions
          .filter(
            (row) =>
              row.status === "pending" &&
              !snapshot.messages.some(
                (message) => message.turnId === row.turnId,
              ),
          )
          .map((row) => (
            <InteractionCard row={row} key={row.id} />
          ))}
        {liveReply && (
          <div className="message assistant live-reply">
            <div className="message-label">
              <span className={`agent-chip ${snapshot.activeAgent}`}>
                <AgentIcon agent={snapshot.activeAgent} />
              </span>
              <strong>
                {agentNames[snapshot.activeAgent].replace(" Code", "")}
              </strong>
              <small>Replying</small>
            </div>
            <Markdown text={liveReply} />
          </div>
        )}
      </div>
    </div>
  );
}
