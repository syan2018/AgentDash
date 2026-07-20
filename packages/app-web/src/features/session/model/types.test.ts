import { describe, expect, it } from "vitest";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { AggregatedContextFrameGroup } from "./types";
import {
  deriveSessionInputSourceView,
  extractTokenUsageFromEvent,
  isDisplayEntry,
} from "./types";

describe("extractTokenUsageFromEvent", () => {
  it("区分当前上下文、pending estimate 和累计用量", () => {
    const event: BackboneEvent = {
      type: "token_usage_updated",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        tokenUsage: {
          modelContextWindow: 200_000,
          last: {
            inputTokens: 10_000,
            cachedInputTokens: 2_000,
            outputTokens: 500,
            reasoningOutputTokens: 100,
            totalTokens: 12_600,
          },
          total: {
            inputTokens: 100_000,
            cachedInputTokens: 20_000,
            outputTokens: 5_000,
            reasoningOutputTokens: 1_000,
            totalTokens: 126_000,
          },
          context: {
            providerContextTokens: 12_600,
            pendingEstimateTokens: 900,
            currentContextTokens: 13_500,
            cumulativeTotalTokens: 126_000,
            modelContextWindow: 200_000,
            effectiveContextWindow: 180_000,
            reserveTokens: 16_384,
            source: "providerPlusEstimate",
          },
        },
      },
    };

    const usage = extractTokenUsageFromEvent(event);

    expect(usage?.currentContextTokens).toBe(13_500);
    expect(usage?.providerContextTokens).toBe(12_600);
    expect(usage?.pendingEstimateTokens).toBe(900);
    expect(usage?.cumulativeTotalTokens).toBe(126_000);
    expect(usage?.effectiveContextWindow).toBe(180_000);
    expect(usage?.last.totalTokens).toBe(12_600);
    expect(usage?.total.totalTokens).toBe(126_000);
  });
});

describe("isDisplayEntry", () => {
  it("不会把 context frame 聚合组误判成普通 entry", () => {
    const group: AggregatedContextFrameGroup = {
      type: "aggregated_context_frames",
      id: "ctx-group",
      groupKey: "context-frame-ctx-group",
      entries: [],
    };

    expect(isDisplayEntry(group)).toBe(false);
  });
});

describe("deriveSessionInputSourceView", () => {
  it("缺少 source 的历史输入事件按普通用户输入展示", () => {
    const source = deriveSessionInputSourceView(undefined);

    expect(source).toEqual({
      namespace: "core",
      kind: "composer",
      actor: "user",
      route: null,
      label: "用户输入",
      presentation: "user",
    });
  });

  it("Companion source 按 channel 输入展示", () => {
    const source = deriveSessionInputSourceView({
      namespace: "companion",
      kind: "result",
      actor: "agent",
      route: "parent",
      displayLabelKey: "agent_input.source.companion.result",
    });

    expect(source.presentation).toBe("companion");
    expect(source.label).toBe("Companion 结果");
    expect(source.route).toBe("parent");
  });
});
