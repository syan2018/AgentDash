import { describe, expect, it, vi } from "vitest";

import { api } from "../api/client";
import { deleteAgentRun } from "./agentRun";

vi.mock("../api/client", () => ({
  api: {
    delete: vi.fn(),
  },
}));

const mockedDelete = vi.mocked(api.delete);

describe("agentRun service", () => {
  it("调用 Project-scoped AgentRun 删除入口并返回 generated response", async () => {
    mockedDelete.mockResolvedValueOnce({
      deleted: true,
      project_id: "project 1",
      run_id: "run/1",
    });

    const response = await deleteAgentRun("project 1", "run/1");

    expect(mockedDelete).toHaveBeenCalledWith("/projects/project%201/agent-runs/run%2F1");
    expect(response).toEqual({
      deleted: true,
      project_id: "project 1",
      run_id: "run/1",
    });
  });
});
