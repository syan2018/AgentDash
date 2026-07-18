import type {
  AgentRunTerminalChange,
  AgentRunTerminalProjection,
  AgentRunTerminalSnapshot,
} from "../../../types/agentRunProductProjections";
import { useTerminalStore } from "../../session/model/useTerminalStore";

function registerTerminal(terminal: AgentRunTerminalProjection): void {
  useTerminalStore.getState().registerTerminal({
    id: terminal.terminal_id,
    runId: terminal.owner.target.run_id,
    agentId: terminal.owner.target.agent_id,
    capability: terminal.capability,
    backendId: terminal.owner.backend_id,
    mountRootRef: terminal.mount_id ?? undefined,
    cwd: terminal.cwd ?? "",
    processId: terminal.process_id ?? undefined,
    state: terminal.state,
    availability: terminal.availability,
    exitCode: terminal.exit_code ?? undefined,
    createdAt: terminal.created_at_ms,
    exitedAt: terminal.exited_at_ms ?? undefined,
  });
}

export function projectAgentRunTerminalSnapshot(
  snapshot: AgentRunTerminalSnapshot,
): void {
  const store = useTerminalStore.getState();
  const snapshotIds = new Set(snapshot.terminals.map((terminal) => terminal.terminal_id));
  for (const terminal of store.getTerminalsForAgentRun(
    snapshot.target.run_id,
    snapshot.target.agent_id,
  )) {
    if (!snapshotIds.has(terminal.id)) store.removeTerminal(terminal.id);
  }
  for (const terminal of snapshot.terminals) {
    registerTerminal(terminal);
    useTerminalStore
      .getState()
      .replaceOutput(terminal.terminal_id, terminal.output.retained_output);
  }
}

export function projectAgentRunTerminalChanges(
  changes: readonly AgentRunTerminalChange[],
): void {
  for (const change of changes) {
    const delta = change.delta;
    const streamIdentity = `agent-run-terminal:${change.target.run_id}:${change.target.agent_id}`;
    switch (delta.kind) {
      case "registered":
        registerTerminal(delta.terminal);
        useTerminalStore
          .getState()
          .replaceOutput(delta.terminal.terminal_id, delta.terminal.output.retained_output);
        break;
      case "output_appended":
        useTerminalStore
          .getState()
          .projectOutputEvent(
            streamIdentity,
            change.sequence,
            delta.terminal_id,
            delta.data,
          );
        break;
      case "output_omitted":
        useTerminalStore
          .getState()
          .replaceOutput(delta.terminal_id, delta.retained_output);
        break;
      case "state_changed":
        useTerminalStore
          .getState()
          .projectStateEvent(
            streamIdentity,
            change.sequence,
            delta.terminal_id,
            delta.state,
            delta.exit_code ?? undefined,
          );
        break;
      case "availability_changed":
        useTerminalStore
          .getState()
          .updateTerminalAvailability(delta.terminal_id, delta.availability);
        break;
      case "removed":
        useTerminalStore.getState().removeTerminal(delta.terminal_id);
        break;
      case "control_correlated":
        break;
    }
  }
}
