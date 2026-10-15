import { useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  Brain,
  Circle,
  Copy,
  FilePlus,
  FileText,
  Globe,
  List,
  ListTodo,
  Pencil,
  Plug,
  Search,
  Shrink,
  Sparkles,
  Terminal,
  type LucideIcon,
} from "lucide-react";
import { useWorkspace } from "../../store/chats";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Step, StepKind } from "../../ipc/types";
import { duration, time } from "../../lib/format";
const kindIcons: Record<StepKind, LucideIcon> = {
  thinking: Brain,
  read: FileText,
  search: Search,
  list: List,
  edit: Pencil,
  write: FilePlus,
  command: Terminal,
  web: Globe,
  subagent: Bot,
  todo: ListTodo,
  mcp: Plug,
  skill: Sparkles,
  compact: Shrink,
  other: Circle,
};
function StepIcon({ kind }: { kind: StepKind }) {
  const Icon = kindIcons[kind];
  return <Icon className="step-icon" />;
}
export default function ActivityPanel() {
  const { snapshot, selectedStep } = useWorkspace();
  const scroll = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  const [now, setNow] = useState(Date.now());
  const rows = useMemo(() => {
    const turns = Array.from(
      new Set(snapshot?.steps.map((step) => step.turnId) ?? []),
    );
    return turns.flatMap((turn, index) => {
      const steps = snapshot!.steps.filter((step) => step.turnId === turn);
      return [
        {
          key: `turn:${turn}`,
          label: `Turn ${index + 1} · ${time(steps[0].startedAt)}`,
          step: null as Step | null,
        },
        ...steps.map((step) => ({ key: step.id, label: "", step })),
      ];
    });
  }, [snapshot?.steps]);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroll.current,
    getItemKey: (index) => rows[index].key,
    estimateSize: (index) => (rows[index].step ? 72 : 28),
    overscan: 6,
    useFlushSync: false,
    useAnimationFrameWithResizeObserver: true,
  });
  useEffect(() => {
    if (snapshot?.status !== "working") return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [snapshot?.status]);
  useEffect(() => {
    if (selectedStep && scroll.current) {
      setFollow(false);
      const index = rows.findIndex((row) => row.step?.id === selectedStep);
      if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
    }
  }, [selectedStep]);
  useEffect(() => {
    if (follow && scroll.current)
      virtualizer.scrollToIndex(Math.max(0, rows.length - 1), { align: "end" });
  }, [snapshot?.steps.length, follow]);
  if (!snapshot?.steps.length)
    return (
      <div className="small-empty">
        <Terminal size={22} />
        <span>Nothing running</span>
        <p>Steps and command output will appear as an agent works.</p>
      </div>
    );
  return (
    <>
      <div
        className="activity-scroll"
        ref={scroll}
        onScroll={() => {
          const el = scroll.current;
          if (el && el.scrollHeight - el.scrollTop - el.clientHeight > 100)
            setFollow(false);
        }}
      >
        <div
          style={{
            height: virtualizer.getTotalSize(),
            position: "relative",
            width: "100%",
          }}
        >
          {virtualizer.getVirtualItems().map((row) => {
            const { step, label } = rows[row.index];
            return (
              <div
                key={row.key}
                data-index={row.index}
                ref={virtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${row.start}px)`,
                }}
              >
                {step ? (
                  <details
                    data-step={step.id}
                    className={`activity-step ${step.parentId ? "nested" : ""}`}
                    open={selectedStep === step.id || undefined}
                    onToggle={(e) => {
                      if (e.currentTarget.open)
                        useWorkspace.setState({ selectedStep: step.id });
                      else if (selectedStep === step.id)
                        useWorkspace.setState({ selectedStep: null });
                    }}
                  >
                    <summary>
                      <StepIcon kind={step.kind} />
                      <span>{step.title}</span>
                      <small className={`step-status ${step.status}`}>
                        {step.status}
                      </small>
                      <small>
                        {duration(
                          Math.max(0, (step.endedAt ?? now) - step.startedAt),
                        )}
                      </small>
                    </summary>
                    <div className="step-detail">
                      <strong>Input</strong>
                      <pre>
                        {JSON.stringify(step.detail.input ?? {}, null, 2)}
                      </pre>
                      {step.detail.output !== undefined && (
                        <>
                          <div className="detail-label">
                            <strong>Output</strong>
                            <button
                              className="icon-button"
                              aria-label="Copy output"
                              onClick={() =>
                                navigator.clipboard
                                  .writeText(step.detail.output ?? "")
                                  .catch(() => {})
                              }
                            >
                              <Copy size={12} />
                            </button>
                          </div>
                          <pre>{step.detail.output}</pre>
                        </>
                      )}
                    </div>
                  </details>
                ) : (
                  <div className="turn-label">{label}</div>
                )}
              </div>
            );
          })}
        </div>
      </div>
      {!follow && (
        <button className="follow-button" onClick={() => setFollow(true)}>
          Follow latest
        </button>
      )}
    </>
  );
}
