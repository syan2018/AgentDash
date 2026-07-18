import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGetMock: vi.fn(),
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGetMock,
    post: mocks.apiPostMock,
  },
}));

import {
  acknowledgeWorkspacePresentation,
  fetchAgentRunTerminalSnapshot,
  fetchWorkspacePresentationSnapshot,
} from "./agentRunProductProjections";

const target = { run_id: "run-1", agent_id: "agent-1" };
const presentationIntent = {
  intent_id: "intent-1",
  effect_id: "effect-1",
  target,
  actor: { kind: "agent_tool", actor_id: "agent-1" },
  cause: {
    runtime_thread_id: "thread-1",
    runtime_operation_id: null,
    runtime_turn_id: "turn-1",
    runtime_item_id: "item-1",
  },
  currentness_fence: {
    binding_id: "binding-1",
    binding_generation: 1,
    surface_revision: 3,
    module_id: "canvas:one",
    view_key: "preview",
    renderer_kind: "canvas",
    presentation_uri: "canvas://one",
  },
  presentation_digest: "sha256:presentation",
  presentation: {
    module_id: "canvas:one",
    view_key: "preview",
    renderer_kind: "canvas",
    presentation_uri: "canvas://one",
    title: "Canvas",
    payload: null,
    diagnostics: null,
  },
  committed_at_ms: 1,
} as const;

describe("AgentRun Product projection service", () => {
  beforeEach(() => {
    mocks.apiGetMock.mockReset();
    mocks.apiPostMock.mockReset();
  });

  it("loads only durable pending Workspace presentation intents", async () => {
    const snapshot = {
      target,
      revision: 4,
      latest_change_sequence: 4,
      captured_at_ms: 10,
      pending_intents: [presentationIntent],
    };
    mocks.apiGetMock.mockResolvedValue(snapshot);

    await expect(fetchWorkspacePresentationSnapshot({
      runId: "run/1",
      agentId: "agent/1",
    })).resolves.toBe(snapshot);
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/workspace-presentations/snapshot",
    );
  });

  it("acks the exact observed pending intent after UI fulfillment", async () => {
    const fulfilled = {
      change_id: "change-5",
      target,
      sequence: 5,
      revision: 5,
      status: "fulfilled",
      intent: presentationIntent,
      acknowledgement: {
        ack_id: "ui-ack:intent-1",
        target,
        intent_id: "intent-1",
        effect_id: "effect-1",
        acknowledged_change_sequence: 4,
        fulfilled_at_ms: 12,
      },
    };
    mocks.apiPostMock.mockResolvedValue(fulfilled);

    await expect(acknowledgeWorkspacePresentation(
      { runId: "run/1", agentId: "agent/1" },
      "intent/1",
      4,
    )).resolves.toBe(fulfilled);
    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/workspace-presentations/intent%2F1/ack",
      { observed_change_sequence: 4 },
    );
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
          binding_id: "binding-1",
          binding_generation: 1,
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
