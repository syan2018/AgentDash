import { describe, expect, it, vi } from "vitest";

const apiPostMock = vi.hoisted(() => vi.fn());
vi.mock("../api/client", () => ({ api: { post: apiPostMock } }));

import { createProjectAgentRun } from "./project";

describe("Project Agent canonical Runtime creation", () => {
  it("creates the Product AgentRun and submits its initial Agent input", async () => {
    await createProjectAgentRun("project-1", "agent/default", {
      input: [{ kind: "text", text: "start" }],
      client_command_id: "create-run-1",
    });

    expect(apiPostMock).toHaveBeenCalledWith(
      "/projects/project-1/agents/agent%2Fdefault/agent-runs",
      {
        input: [{ kind: "text", text: "start" }],
        client_command_id: "create-run-1",
      },
    );
  });
});
