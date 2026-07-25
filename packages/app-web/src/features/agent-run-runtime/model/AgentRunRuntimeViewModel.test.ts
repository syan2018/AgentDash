import { describe, expect, it } from "vitest";

import {
  isAgentRunWorkspaceActionRunning,
  resolveExecutorFromHint,
} from "./AgentRunRuntimeViewModel";

describe("AgentRunRuntimeViewModel", () => {
  it("不会从未知产品状态推断运行中", () => {
    expect(
      isAgentRunWorkspaceActionRunning({ executionStatus: "running_active" }),
    ).toBe(true);
    expect(
      isAgentRunWorkspaceActionRunning({ executionStatus: "succeeded" }),
    ).toBe(false);
  });

  it("executor hint 只做配置标识归一", () => {
    expect(resolveExecutorFromHint("codex-app", [{ id: "CODEX_APP" }])).toBe(
      "CODEX_APP",
    );
  });
});
