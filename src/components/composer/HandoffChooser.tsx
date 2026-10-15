import { agentNames, type HandoffMode } from "../../ipc/types";
import { chooseHandoff, reportError, useWorkspace } from "../../store/chats";
export default function HandoffChooser() {
  const { tabs, activeTab, settings } = useWorkspace();
  const choice = tabs.find((tab) => tab.id === activeTab)?.handoff;
  if (!choice) return null;
  const from = choice.from
    ? agentNames[choice.from].replace(" Code", "")
    : "the previous agent";
  return (
    <div className="handoff-chooser">
      <span>
        Switching to {agentNames[settings.agent].replace(" Code", "")}. What
        should it get?
      </span>
      <div>
        {(
          [
            ["all", `All messages it missed (${choice.count})`],
            ["last", `Only ${from}'s last reply`],
            ["summary", `Summary written by ${from}`],
          ] as [HandoffMode, string][]
        ).map(([mode, label]) => (
          <label key={mode}>
            <input
              type="radio"
              name={`handoff-${activeTab}`}
              checked={choice.mode === mode}
              onChange={() => chooseHandoff(mode).catch(reportError)}
            />
            {label}
          </label>
        ))}
      </div>
    </div>
  );
}
