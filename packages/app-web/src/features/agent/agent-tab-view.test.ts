import { describe, expect, it } from "vitest";

import { projectAgentDraftSessionPath } from "./agent-tab-view";

describe("projectAgentDraftSessionPath", () => {
  it("构造 ProjectAgent Draft session route", () => {
    expect(projectAgentDraftSessionPath("project-1", "agent-1")).toBe(
      "/session/new?project_id=project-1&project_agent_id=agent-1",
    );
  });

  it("对 query 参数做 URL 编码", () => {
    expect(projectAgentDraftSessionPath("project/a", "agent b")).toBe(
      "/session/new?project_id=project%2Fa&project_agent_id=agent+b",
    );
  });
});
