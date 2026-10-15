import type { Step } from "../ipc/types";
const mergeable = new Set(["read", "search", "list", "edit", "write"]);
export interface StepGroup {
  title: string;
  steps: Step[];
}
export function mergeSteps(steps: Step[]): StepGroup[] {
  const groups: StepGroup[] = [];
  for (const step of steps) {
    const previous = groups.at(-1);
    if (
      previous &&
      previous.steps[0].kind === step.kind &&
      previous.steps[0].turnId === step.turnId &&
      previous.steps[0].parentId === step.parentId &&
      mergeable.has(step.kind)
    ) {
      previous.steps.push(step);
      const running = previous.steps.some((s) => s.status === "running");
      const count =
        step.kind === "edit"
          ? new Set(
              previous.steps.map((s) =>
                typeof s.detail.input === "object" &&
                s.detail.input !== null &&
                "file_path" in s.detail.input
                  ? String(s.detail.input.file_path)
                  : s.title.replace(/^Editing |^Edited /, ""),
              ),
            ).size
          : previous.steps.length;
      const verbs: Record<string, string[]> = {
        read: ["Reading", "Read"],
        search: ["Searching", "Searched"],
        list: ["Listing", "Listed"],
        edit: ["Editing", "Edited"],
        write: ["Writing", "Wrote"],
      };
      previous.title = `${verbs[step.kind][running ? 0 : 1]} ${count} ${step.kind === "search" ? (count === 1 ? "pattern" : "patterns") : step.kind === "list" ? (count === 1 ? "folder" : "folders") : count === 1 ? "file" : "files"}`;
    } else groups.push({ title: step.title, steps: [step] });
  }
  return groups;
}
export function isCommentary(step: Step) {
  const input = step.detail.input;
  return typeof input === "object" && input !== null && "commentary" in input;
}
export function commentaryText(step: Step) {
  return isCommentary(step)
    ? String((step.detail.input as { commentary: unknown }).commentary)
    : "";
}
export function splitSteps(steps: Step[]) {
  return {
    tools: steps.filter((s) => s.kind !== "thinking" && !isCommentary(s)),
    commentary: steps.filter(isCommentary),
  };
}
export function currentTool(tools: Step[]) {
  return (
    [...tools].reverse().find((s) => s.status === "running") ?? tools.at(-1)
  );
}
