import { describe, expect, it } from "vitest";

import { agentRunJournalSessionId } from "./agentRunJournalIdentity";

describe("agentRunJournalSessionId", () => {
  it("matches the backend AgentRun journal identity", () => {
    expect(agentRunJournalSessionId({ runId: "run-1", agentId: "agent-1" }))
      .toBe("agentrun:run-1:agent-1");
  });
});
