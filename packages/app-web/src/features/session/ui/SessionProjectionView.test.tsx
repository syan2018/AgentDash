import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { SessionProjectionViewResponse } from "../../../generated/session-contracts";
import type { ConversationCommandView } from "../../../generated/workflow-contracts";
import { SessionProjectionViewPanel } from "./SessionProjectionView";
import {
  commandPrecondition,
  contextCompactionOutcomeMessage,
} from "./sessionProjectionCompactionAction";

const mocks = vi.hoisted(() => ({
  compactAgentRunContext: vi.fn(),
  fetchAgentRunRuntimeContextProjection: vi.fn(),
}));

vi.mock("../../../services/agentRunRuntime", () => ({
  compactAgentRunContext: mocks.compactAgentRunContext,
  fetchAgentRunRuntimeContextProjection: mocks.fetchAgentRunRuntimeContextProjection,
}));

beforeEach(() => {
  mocks.compactAgentRunContext.mockReset();
  mocks.fetchAgentRunRuntimeContextProjection.mockReset();
});

afterEach(() => {
  vi.doUnmock("react");
  vi.unstubAllGlobals();
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

  it("按 generated command 状态渲染手动压缩按钮", () => {
    const markup = renderToStaticMarkup(
      <SessionProjectionViewPanel
        projection={sampleProjection()}
        agentRunTarget={{ runId: "run-1", agentId: "agent-1" }}
        compactContextCommand={sampleCompactCommand({
          enabled: false,
          unavailable_reason: "缺少 runtime session",
          disabled_code: "frame_missing",
        })}
      />,
    );

    expect(markup).toContain("手动压缩上下文");
    expect(markup).toContain("缺少 runtime session");
    expect(markup).toContain("disabled");
  });

  it("点击手动压缩只提交 command-only request", async () => {
    vi.stubGlobal("crypto", { randomUUID: () => "command-compact-1" });
    mocks.compactAgentRunContext.mockResolvedValue({
      operation_id: "operation-compact-1",
      operation_sequence: 1n,
      thread_id: "thread-1",
      accepted_revision: 4n,
      duplicate: false,
    });
    const { SessionProjectionViewPanel: Panel } = await importProjectionViewWithImmediateEffects();
    const element = Panel({
      projection: sampleProjection(),
      agentRunTarget: { runId: "run/1", agentId: "agent/1" },
      compactContextCommand: sampleCompactCommand(),
      embedded: true,
    });
    const button = findButtonByAriaLabel(element, "手动压缩上下文");
    if (!button) {
      throw new Error("compact context button should render");
    }
    const onClick = button.props.onClick;
    if (typeof onClick !== "function") {
      throw new Error("compact context button should have click handler");
    }

    onClick();
    await flushPromises();

    expect(mocks.compactAgentRunContext).toHaveBeenCalledWith(
      "run/1",
      "agent/1",
      {
        client_command_id: "command-compact-1",
        command: commandPrecondition(sampleCompactCommand()),
      },
    );
  });
});

describe("context compaction helpers", () => {
  it("renders canonical Runtime operation acceptance and duplicate replay", () => {
    expect(contextCompactionOutcomeMessage({
      operation_id: "operation-1",
      operation_sequence: 1n,
      thread_id: "thread-1",
      accepted_revision: 4n,
      duplicate: false,
    })).toBe("压缩操作已接受 · operation-1");
    expect(contextCompactionOutcomeMessage({
      operation_id: "operation-1",
      operation_sequence: 1n,
      thread_id: "thread-1",
      accepted_revision: 4n,
      duplicate: true,
    })).toBe("压缩操作已存在 · operation-1");
  });
});

describe("fetchSessionProjectionForTarget", () => {
  it("AgentRun target 存在时不要求 raw session id", async () => {
    const projection = sampleProjection();
    mocks.fetchAgentRunRuntimeContextProjection.mockResolvedValue(projection);
    const { SessionProjectionView } = await importProjectionViewWithImmediateEffects();

    SessionProjectionView({
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object";
}

function elementProps(value: unknown): Record<string, unknown> | null {
  if (!isRecord(value)) return null;
  const props = value.props;
  return isRecord(props) ? props : null;
}

function childNodes(props: Record<string, unknown>): unknown[] {
  const children = props.children;
  if (Array.isArray(children)) return children;
  return children == null ? [] : [children];
}

function findButtonByAriaLabel(
  node: unknown,
  label: string,
): { props: Record<string, unknown> } | null {
  if (Array.isArray(node)) {
    for (const child of node) {
      const found = findButtonByAriaLabel(child, label);
      if (found) return found;
    }
    return null;
  }
  const props = elementProps(node);
  if (!props) return null;
  if (props["aria-label"] === label) return { props };
  for (const child of childNodes(props)) {
    const found = findButtonByAriaLabel(child, label);
    if (found) return found;
  }
  return null;
}

function sampleCompactCommand(overrides: Partial<ConversationCommandView> = {}): ConversationCommandView {
  return {
    kind: "compact_context",
    command_id: "compact_context",
    enabled: true,
    requires_input: false,
    executor_config_policy: "forbidden",
    placement: ["header"],
    stale_guard: {
      snapshot_id: "snapshot-1",
      run_id: "run/1",
      agent_id: "agent/1",
      frame_id: "frame-1",
      active_turn_id: "turn-1",
    },
    ...overrides,
  };
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
