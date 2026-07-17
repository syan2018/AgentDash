import { describe, expect, it } from "vitest";

import { agentRunListPresentationStatus } from "./agent-run-delivery-status";

describe("agentRunListPresentationStatus", () => {
  it("只有 active Runtime 存在 active turn 时显示 running", () => {
    expect(agentRunListPresentationStatus("active", "turn-1", "active")).toBe("running");
    expect(agentRunListPresentationStatus("active", undefined, "active")).toBe("idle");
  });

  it("closed Runtime 使用 Lifecycle 终态而不伪造 completed", () => {
    expect(agentRunListPresentationStatus("closed", undefined, "failed")).toBe("failed");
    expect(agentRunListPresentationStatus("closed", undefined, "cancelled")).toBe("interrupted");
  });

  it("异常 Runtime 状态不会降级成普通 idle", () => {
    expect(agentRunListPresentationStatus("suspended", undefined, "active")).toBe("suspended");
    expect(agentRunListPresentationStatus("desynchronized", undefined, "active")).toBe("lost");
  });
});
