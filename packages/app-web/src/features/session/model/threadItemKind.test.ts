import { describe, expect, it } from "vitest";
import type { AgentDashThreadItem } from "../../../generated/backbone-protocol";
import {
  isToolBurstEligible,
  resolveDynamicToolMeta,
  resolveKind,
} from "./threadItemKind";

describe("threadItemKind", () => {
  it("keeps context compaction outside tool burst while preserving its kind", () => {
    const item: AgentDashThreadItem = {
      type: "contextCompaction",
      id: "ctx-1",
    };

    expect(resolveKind(item).kind).toBe("context");
    expect(isToolBurstEligible(item)).toBe(false);
  });

  it("marks concrete tool items as burst eligible", () => {
    const item: AgentDashThreadItem = {
      type: "dynamicToolCall",
      id: "tool-1",
      namespace: null,
      tool: "Read",
      arguments: { path: "src/App.tsx" },
      status: "completed",
      contentItems: null,
      success: true,
      durationMs: null,
    };

    expect(resolveKind(item).kind).toBe("read");
    expect(isToolBurstEligible(item)).toBe(true);
  });

  it("resolves dynamic tool families from one metadata source", () => {
    expect(resolveDynamicToolMeta("Read")).toMatchObject({
      kind: expect.objectContaining({ kind: "read" }),
      family: "read",
      fallbackLabel: "Read",
    });
    expect(resolveDynamicToolMeta("str_replace_editor")).toMatchObject({
      kind: expect.objectContaining({ kind: "edit" }),
      family: "edit",
      fallbackLabel: "Edit",
    });
    expect(resolveDynamicToolMeta("WebFetch")).toMatchObject({
      kind: expect.objectContaining({ kind: "fetch" }),
      family: "fetch",
      fallbackLabel: "WebFetch",
    });
    expect(resolveDynamicToolMeta("AskUserQuestion")).toMatchObject({
      kind: expect.objectContaining({ kind: "tool" }),
      family: "question",
      fallbackLabel: "AskQuestion",
    });
  });
});
