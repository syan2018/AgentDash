import { describe, expect, it } from "vitest";

import { agentRunListPresentationStatus } from "./agent-run-delivery-status";

describe("agentRunListPresentationStatus", () => {
  it("直接映射 Product Lifecycle 执行态", () => {
    expect(agentRunListPresentationStatus("running")).toBe("running");
    expect(agentRunListPresentationStatus("active")).toBe("idle");
  });

  it("映射 Product Lifecycle 终态", () => {
    expect(agentRunListPresentationStatus("completed")).toBe("completed");
    expect(agentRunListPresentationStatus("failed")).toBe("failed");
    expect(agentRunListPresentationStatus("cancelled")).toBe("interrupted");
  });

  it("映射 Product Lifecycle 非终态", () => {
    expect(agentRunListPresentationStatus("suspended")).toBe("suspended");
    expect(agentRunListPresentationStatus("cancelling")).toBe("cancelling");
    expect(agentRunListPresentationStatus("lost")).toBe("lost");
  });
});
