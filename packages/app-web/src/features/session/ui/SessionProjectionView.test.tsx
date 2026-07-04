import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { SessionProjectionViewResponse } from "../../../generated/session-contracts";
import { SessionProjectionViewPanel } from "./SessionProjectionView";

const mocks = vi.hoisted(() => ({
  fetchAgentRunRuntimeContextProjection: vi.fn(),
  fetchSessionContextProjection: vi.fn(),
}));

vi.mock("../../../services/agentRunRuntime", () => ({
  fetchAgentRunRuntimeContextProjection: mocks.fetchAgentRunRuntimeContextProjection,
}));

vi.mock("../../../services/session", () => ({
  fetchSessionContextProjection: mocks.fetchSessionContextProjection,
}));

beforeEach(() => {
  mocks.fetchAgentRunRuntimeContextProjection.mockReset();
  mocks.fetchSessionContextProjection.mockReset();
});

afterEach(() => {
  vi.doUnmock("react");
});

describe("SessionProjectionViewPanel", () => {
  it("渲染模型投影版本、压缩范围和 synthetic segment", () => {
    const markup = renderToStaticMarkup(
      <SessionProjectionViewPanel projection={sampleProjection()} />,
    );

    expect(markup).toContain("CONTEXT");
    expect(markup).toContain("v2");
    expect(markup).toContain("head #42");
    expect(markup).toContain("summary_chunk");
    expect(markup).toContain("#1-#30");
    expect(markup).toContain("synthetic");
    expect(markup).toContain("压缩后的历史摘要");
    expect(markup).toContain("System / Developer");
    expect(markup).toContain("工具调用");
  });

  it("剩余空间使用 effective window 时不重复扣除 reserve", () => {
    const markup = renderToStaticMarkup(
      <SessionProjectionViewPanel
        projection={sampleProjection()}
        tokenUsage={{
          currentContextTokens: 13_500,
          providerContextTokens: 12_600,
          pendingEstimateTokens: 900,
          cumulativeTotalTokens: 126_000,
          modelContextWindow: 200_000,
          effectiveContextWindow: 180_000,
          reserveTokens: 16_384,
          usageSource: "providerPlusEstimate",
          last: {
            inputTokens: 10_000,
            outputTokens: 500,
            totalTokens: 12_600,
            cacheReadTokens: 2_000,
            cacheCreationTokens: 0,
            reasoningTokens: 100,
          },
          total: {
            inputTokens: 100_000,
            outputTokens: 5_000,
            totalTokens: 126_000,
            cacheReadTokens: 20_000,
            cacheCreationTokens: 0,
            reasoningTokens: 1_000,
          },
        }}
      />,
    );

    expect(markup).toContain("剩余空间");
    expect(markup).toContain("166.5K");
    expect(markup).not.toContain("150.1K");
  });
});

describe("fetchSessionProjectionForTarget", () => {
  it("AgentRun target 存在时不要求 raw session id", async () => {
    const projection = sampleProjection();
    mocks.fetchAgentRunRuntimeContextProjection.mockResolvedValue(projection);
    const { SessionProjectionView } = await importProjectionViewWithImmediateEffects();

    SessionProjectionView({
      sessionId: null,
      agentRunTarget: { runId: "run-1", agentId: "agent-1" },
      refreshKey: 0,
      tokenUsage: null,
      embedded: false,
    });
    await flushPromises();

    expect(mocks.fetchAgentRunRuntimeContextProjection).toHaveBeenCalledWith({
      runId: "run-1",
      agentId: "agent-1",
    });
    expect(mocks.fetchSessionContextProjection).not.toHaveBeenCalled();
  });

  it("raw session projection 仍要求 session id 作为 fallback target", async () => {
    const projection = sampleProjection();
    mocks.fetchSessionContextProjection.mockResolvedValue(projection);
    const { SessionProjectionView } = await importProjectionViewWithImmediateEffects();

    SessionProjectionView({
      sessionId: "sess-1",
      agentRunTarget: null,
      refreshKey: 0,
      tokenUsage: null,
      embedded: false,
    });
    await flushPromises();

    expect(mocks.fetchSessionContextProjection).toHaveBeenCalledWith("sess-1");
    expect(mocks.fetchAgentRunRuntimeContextProjection).not.toHaveBeenCalled();
  });
});

async function importProjectionViewWithImmediateEffects() {
  vi.resetModules();
  vi.doMock("react", async (importOriginal) => {
    const actual = await importOriginal<typeof import("react")>();
    return {
      ...actual,
      useCallback: <T,>(callback: T, _deps?: readonly unknown[]) => callback,
      useEffect: (effect: () => void | (() => void), _deps?: readonly unknown[]) => {
        effect();
      },
      useState: <T,>(initial: T | (() => T)) => {
        const value = typeof initial === "function" ? (initial as () => T)() : initial;
        const setter = vi.fn();
        return [value, setter as (value: T | ((prev: T) => T)) => void];
      },
    };
  });
  return import("./SessionProjectionView");
}

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

function sampleProjection(): SessionProjectionViewResponse {
  return {
    session_id: "sess-1",
    projection_kind: "model_context",
    projection_version: 2,
    head_event_seq: 42,
    active_compaction_id: "compaction-1",
    token_estimate: 128,
    message_count: 2,
    context_usage: {
      categories: [
        {
          kind: "system_developer",
          label: "System / Developer",
          token_estimate: 18,
          source: "context_frame",
          deferred: false,
        },
        {
          kind: "messages",
          label: "Messages",
          token_estimate: 32,
          source: "local_estimate",
          deferred: false,
        },
        {
          kind: "compaction_summary",
          label: "Compaction Summary",
          token_estimate: 96,
          source: "projected",
          deferred: false,
        },
      ],
      items: [
        {
          kind: "system_developer",
          label: "System / Developer",
          name: "Identity",
          token_estimate: 18,
          source: "context_frame",
          deferred: false,
          source_event_seq: 2,
          turn_id: "turn-1",
        },
      ],
      messages: {
        user_message_tokens: 32,
        assistant_message_tokens: 0,
        tool_call_tokens: 0,
        tool_result_tokens: 0,
        attachment_tokens: 0,
      },
      top_tools: [],
      top_attachments: [],
    },
    segments: [
      {
        id: "segment-1",
        sort_order: 0,
        segment_type: "summary_chunk",
        role: "compaction_summary",
        origin: "projection",
        synthetic: true,
        projection_kind: "compaction_summary",
        message_ref: {
          turn_id: "_projection:segment-1",
          entry_index: 0,
        },
        source_range: {
          start_event_seq: 1,
          end_event_seq: 30,
        },
        projection_segment_id: "segment-1",
        preview: "压缩后的历史摘要",
        token_estimate: 96,
        tool_names: [],
        provenance: {
          compaction_id: "compaction-1",
          projection_version: 2,
          segment_type: "summary_chunk",
          strategy: "summary_prefix",
          trigger: "auto",
          phase: "pre_provider",
        },
      },
      {
        id: "original_event:1",
        sort_order: 1,
        segment_type: "original_event",
        role: "user",
        origin: "event",
        synthetic: false,
        projection_kind: "model_context",
        message_ref: {
          turn_id: "turn-9",
          entry_index: 0,
        },
        source_event_seq: 31,
        preview: "继续推进下一步",
        token_estimate: 32,
        tool_names: [],
        provenance: {},
      },
    ],
  };
}
