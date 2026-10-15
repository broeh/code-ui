import type { AgentKind, AgentStatus } from "../ipc/types";

export function availableAgent(agents: AgentStatus[], preferred: AgentKind): AgentKind {
  const available = agents.filter((agent) => agent.enabled && agent.installed && !agent.problem);
  return available.some((agent) => agent.agent === preferred)
    ? preferred
    : available[0]?.agent ?? preferred;
}
