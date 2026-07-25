import { describe, expect, it } from "vitest";

import {
  executorSourceFromExecutionProfile,
  resolveAgentRunClientCommandId,
} from "./workspaceCommandState";

describe("AgentRun workspace command state", () => {
  it("hydrates executor selector source from frame execution profile", () => {
    expect(executorSourceFromExecutionProfile({
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      agent_id: "agent-profile",
      thinking_level: "high",
    })).toEqual({
      executor: "CODEX",
      provider_id: "openai",
      model_id: "gpt-test",
      agent_id: "agent-profile",
      thinking_level: "high",
    });
  });

  it("keeps authoritative empty executor fields instead of reviving stale local values", () => {
    expect(executorSourceFromExecutionProfile({
      executor: "",
      provider_id: "",
      model_id: "",
      thinking_level: "",
    })).toEqual({
      executor: "",
      provider_id: "",
      model_id: "",
      agent_id: undefined,
      thinking_level: undefined,
    });
  });

  it("reuses client command id for the same in-flight payload", () => {
    const current = { key: "same-payload", id: "command-1" };

    expect(resolveAgentRunClientCommandId(
      current,
      "same-payload",
      () => "command-2",
    )).toEqual({
      clientCommandId: "command-1",
      inFlightCommand: current,
    });
  });

  it("creates a new client command id for a changed payload", () => {
    expect(resolveAgentRunClientCommandId(
      { key: "old-payload", id: "command-1" },
      "new-payload",
      () => "command-2",
    )).toEqual({
      clientCommandId: "command-2",
      inFlightCommand: { key: "new-payload", id: "command-2" },
    });
  });
});
