import { beforeEach, describe, expect, it, vi } from "vitest";

import { useProjectStore } from "./projectStore";
import * as projectService from "../services/project";

vi.mock("../services/project", () => ({
  createProjectAgentRun: vi.fn(),
}));

describe("projectStore AgentRun commands", () => {
  beforeEach(() => {
    useProjectStore.setState({
      agentsByProjectId: {},
      error: null,
    });
    vi.clearAllMocks();
  });

  it("propagates createProjectAgentRun API errors", async () => {
    const error = new Error("缺少模型选择");
    vi.mocked(projectService.createProjectAgentRun).mockRejectedValue(error);

    await expect(useProjectStore.getState().createProjectAgentRun("project-1", "agent-1", {
      input: [],
      client_command_id: "cmd-1",
    })).rejects.toThrow("缺少模型选择");
    expect(useProjectStore.getState().error).toBe("缺少模型选择");
  });
});
