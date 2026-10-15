import claude from "../../assets/agents/claude.svg";
import codex from "../../assets/agents/codex.svg";
import agy from "../../assets/agents/agy.svg";
import muse from "../../assets/agents/muse.svg";
import type { AgentKind } from "../../ipc/types";

const icons: Record<AgentKind, string> = { claude, codex, agy, muse };

export default function AgentIcon({ agent }: { agent: AgentKind }) {
  return <img className="agent-icon" src={icons[agent]} alt="" />;
}
