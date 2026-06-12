import { describe, expect, it } from "vitest";

import { projectAgentDraftRunPath } from "./project-agent-paths";

describe("projectAgentDraftRunPath", () => {
  it("构造 ProjectAgent Draft AgentRun route", () => {
    expect(projectAgentDraftRunPath("project-1", "agent-1")).toBe(
      "/agent-runs/new?project_id=project-1&project_agent_id=agent-1",
    );
  });

  it("对 query 参数做 URL 编码", () => {
    expect(projectAgentDraftRunPath("project/a", "agent b")).toBe(
      "/agent-runs/new?project_id=project%2Fa&project_agent_id=agent+b",
    );
  });
});
