import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGetMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGetMock,
  },
}));

import { fetchAgentRunTerminalSnapshot } from "./agentRunProductProjections";

const target = { run_id: "run-1", agent_id: "agent-1" };
describe("AgentRun Product projection service", () => {
  beforeEach(() => {
    mocks.apiGetMock.mockReset();
  });

  it("loads terminal process state independently from backend availability", async () => {
    const snapshot = {
      target,
      revision: 2,
      latest_change_sequence: 2,
      captured_at_ms: 10,
      terminals: [{
        terminal_id: "terminal-1",
        owner: {
          terminal_owner_epoch_id: "epoch-1",
          target,
          runtime_thread_id: "thread-1",
          source_binding: {
            source_ref: "source-1",
            committed_at_revision: 1,
            applied_surface_revision: 3,
            activated_at_revision: 2,
          },
          backend_id: "backend-1",
        },
        mount_id: null,
        cwd: ".",
        capability: "interactive",
        max_output_bytes: 262_144,
        state: "running",
        availability: "offline",
        latest_source_sequence: 8,
        exit_code: null,
        process_id: 42,
        created_at_ms: 1,
        exited_at_ms: null,
        output: {
          next_sequence: 3,
          retained_output: "tail",
          truncated: false,
          omitted_bytes: 0,
        },
      }],
    };
    mocks.apiGetMock.mockResolvedValue(snapshot);

    await expect(fetchAgentRunTerminalSnapshot({
      runId: "run-1",
      agentId: "agent-1",
    })).resolves.toBe(snapshot);
  });
});
