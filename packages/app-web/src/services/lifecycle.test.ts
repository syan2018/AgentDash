import { beforeEach, describe, expect, it, vi } from "vitest";

const apiGetMock = vi.hoisted(() => vi.fn());

vi.mock("../api/client", () => ({ api: { get: apiGetMock } }));

import { fetchProjectAgentRuns } from "./lifecycle";

describe("Project AgentRun list service", () => {
  beforeEach(() => apiGetMock.mockReset());

  it("使用编码后的Project route与opaque cursor读取current list projection", async () => {
    apiGetMock.mockResolvedValue({
      project_id: "project/1",
      agent_runs: [],
    });

    await fetchProjectAgentRuns("project/1", {
      limit: 30,
      cursor: "123:run/id",
    });

    expect(apiGetMock).toHaveBeenCalledWith(
      "/projects/project%2F1/agent-runs?limit=30&cursor=123%3Arun%2Fid",
    );
  });
});
