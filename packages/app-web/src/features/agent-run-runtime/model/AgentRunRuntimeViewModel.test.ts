import { describe, expect, it } from "vitest";

import type { ManagedRuntimePlatformChange } from "../../../generated/agent-runtime-validators";
import {
  computeAgentRunRuntimeProjectionRefreshKey,
  isAgentRunWorkspaceActionRunning,
  resolveExecutorFromHint,
} from "./AgentRunRuntimeViewModel";

function change(
  sequence: bigint,
  delta: ManagedRuntimePlatformChange["delta"],
): ManagedRuntimePlatformChange {
  return {
    thread_id: "runtime-thread",
    sequence,
    revision: sequence,
    delta,
  };
}

describe("AgentRunRuntimeViewModel", () => {
  it("只以 committed Runtime projection change 推进刷新键", () => {
    const changes = [
      change(1n, {
        kind: "command_availability_changed",
        command: "fork",
        availability: {
          status: "available",
          evidence: {
            decided_at_revision: 1n,
            blocking_operation_id: null,
            bound_surface_revision: null,
            applied_surface_revision: null,
          },
        },
      }),
      change(2n, {
        kind: "runtime_lifecycle_changed",
        lifecycle: "active",
      }),
    ];

    expect(computeAgentRunRuntimeProjectionRefreshKey(changes)).toBe(2n);
  });

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
