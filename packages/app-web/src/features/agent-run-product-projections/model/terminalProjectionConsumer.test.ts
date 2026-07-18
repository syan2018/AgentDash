import { beforeEach, describe, expect, it } from "vitest";

import type {
  AgentRunTerminalChange,
  AgentRunTerminalSnapshot,
} from "../../../types/agentRunProductProjections";
import { useTerminalStore } from "../../session/model/useTerminalStore";
import {
  projectAgentRunTerminalChanges,
  projectAgentRunTerminalSnapshot,
} from "./terminalProjectionConsumer";

const target = { run_id: "run-1", agent_id: "agent-1" };
const owner = {
  terminal_owner_epoch_id: "epoch-1",
  target,
  runtime_thread_id: "thread-1",
  binding_id: "binding-1",
  binding_generation: 1,
  backend_id: "backend-1",
};

beforeEach(() => {
  useTerminalStore.setState({
    terminals: new Map(),
    outputBuffers: new Map(),
    outputBufferBaseOffsets: new Map(),
    outputBufferRevisions: new Map(),
    projectedEventKeys: new Set(),
  });
});

describe("terminal Product projection consumer", () => {
  it("hydrates retained ordered output and keeps availability orthogonal to process state", () => {
    const snapshot: AgentRunTerminalSnapshot = {
      target,
      revision: 3,
      latest_change_sequence: 3,
      captured_at_ms: 10,
      terminals: [{
        terminal_id: "terminal-1",
        owner,
        capability: "interactive",
        max_output_bytes: 262_144,
        state: "running",
        availability: "online",
        latest_source_sequence: 7,
        created_at_ms: 1,
        output: {
          next_sequence: 4,
          retained_output: "ordered output",
          truncated: false,
          omitted_bytes: 0,
        },
      }],
    };
    projectAgentRunTerminalSnapshot(snapshot);

    const offline: AgentRunTerminalChange = {
      change_id: "change-4",
      target,
      sequence: 4,
      revision: 4,
      source_sequence: 8,
      payload_digest: "sha256:offline",
      delta: {
        kind: "availability_changed",
        terminal_id: "terminal-1",
        owner,
        availability: "offline",
        changed_at_ms: 11,
      },
    };
    projectAgentRunTerminalChanges([offline]);

    const store = useTerminalStore.getState();
    expect(store.getOutput("terminal-1")).toBe("ordered output");
    expect(store.terminals.get("terminal-1")).toMatchObject({
      state: "running",
      availability: "offline",
    });
  });

  it("deduplicates replayed output by durable central change sequence", () => {
    const change: AgentRunTerminalChange = {
      change_id: "change-1",
      target,
      sequence: 1,
      revision: 1,
      source_sequence: 1,
      payload_digest: "sha256:one",
      delta: {
        kind: "output_appended",
        terminal_id: "terminal-1",
        owner,
        output_sequence: 1,
        stream: "pty",
        data: "one",
      },
    };
    projectAgentRunTerminalChanges([change, change]);
    expect(useTerminalStore.getState().getOutput("terminal-1")).toBe("one");
  });

  it("replaces the local tail from typed OutputOmitted evidence", () => {
    useTerminalStore.getState().replaceOutput("terminal-1", "stale prefix");
    projectAgentRunTerminalChanges([{
      change_id: "change-2",
      target,
      sequence: 2,
      revision: 2,
      source_sequence: 2,
      payload_digest: "sha256:omitted",
      delta: {
        kind: "output_omitted",
        terminal_id: "terminal-1",
        owner,
        output_sequence: 2,
        omitted_bytes: 4_096,
        retained_output: "retained tail",
      },
    }]);
    expect(useTerminalStore.getState().getOutput("terminal-1")).toBe("retained tail");
  });
});
