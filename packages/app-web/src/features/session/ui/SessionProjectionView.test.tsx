import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ContextFrame } from "../../../generated/backbone-protocol";
import type { AgentRuntimeContextProjection } from "../../../generated/agent-runtime-codecs";
import {
  fetchAgentRuntimeContextProjectionWithRetry,
  validateAgentRuntimeContextProjectionCommit,
} from "../../agent-run-runtime/model/useAgentRuntimeContextProjection";
import {
  SessionProjectionViewPanel,
} from "./SessionProjectionView";
import type { SessionChatCommandModel } from "./SessionChatViewTypes";

const mocks = vi.hoisted(() => ({
  fetchAgentRunRuntimeContextProjection: vi.fn(),
}));

vi.mock("../../../services/agentRunRuntime", () => ({
  fetchAgentRunRuntimeContextProjection: mocks.fetchAgentRunRuntimeContextProjection,
}));

beforeEach(() => {
  mocks.fetchAgentRunRuntimeContextProjection.mockReset();
});

afterEach(() => {
  vi.doUnmock("react");
  vi.unstubAllGlobals();
});

describe("SessionProjectionViewPanel", () => {
  it("完整渲染 Agent 权威配方、ContextFrame 和保留消息", () => {
    const markup = renderToStaticMarkup(
      <SessionProjectionViewPanel projection={sampleProjection()} />,
    );

    expect(markup).toContain("snapshot #42");
    expect(markup).toContain("agent_owned · exact");
    expect(markup).toContain("context-revision-2");
    expect(markup).toContain("ContextFrame");
    expect(markup).toContain("compaction_summary");
    expect(markup).toContain("压缩后的完整摘要");
    expect(markup).toContain("完整结构 · 1 sections");
    expect(markup).toContain("保留的用户消息");
    expect(markup).toContain("模型输入估算");
    expect(markup).toContain("主要段落");
    expect(markup).toContain("消息明细");
    expect(markup).toContain("Top Tools · 1");
    expect(markup).toContain("完整模型输入 · 3 项");
    expect(markup).toContain("Tool");
    expect(markup).toContain("apply_patch");
    expect(markup).toContain("recipe sha256:recipe");
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

  it("点击手动压缩委托给 AgentRuntimeConnection action", async () => {
    const onCompactContext = vi.fn().mockResolvedValue(undefined);
    const { SessionProjectionViewPanel: Panel } = await importProjectionViewWithImmediateEffects();
    const element = Panel({
      projection: sampleProjection(),
      agentRunTarget: { runId: "run/1", agentId: "agent/1" },
      compactContextCommand: sampleCompactCommand(),
      onCompactContext,
      embedded: true,
    });
    const button = findButtonByAriaLabel(element, "手动压缩上下文");
    if (!button || typeof button.props.onClick !== "function") {
      throw new Error("compact context button should render");
    }

    button.props.onClick();
    await flushPromises();

    expect(onCompactContext).toHaveBeenCalledTimes(1);
  });
});

describe("SessionProjectionView", () => {
  it("AgentRun target 存在时读取 Agent context snapshot", async () => {
    mocks.fetchAgentRunRuntimeContextProjection.mockResolvedValue(sampleProjection());
    const { SessionProjectionView } = await importProjectionViewWithImmediateEffects();

    SessionProjectionView({
      agentRunTarget: { runId: "run-1", agentId: "agent-1" },
      contextCoordinate: {
        snapshot_revision: 42n,
        context_revision: "context-revision-2",
        recipe_digest: "sha256:recipe",
        authority: "agent_owned",
        fidelity: "exact",
      },
      tokenUsage: null,
      embedded: false,
    });
    await flushPromises();

    expect(mocks.fetchAgentRunRuntimeContextProjection).toHaveBeenCalledWith({
      runId: "run-1",
      agentId: "agent-1",
    }, sampleContextCoordinate(), expect.any(AbortSignal));
  });

  it("拒绝低于 required revision 的 context snapshot", () => {
    const projection = sampleProjection();
    projection.recipe.coordinate.snapshot_revision = 41n;
    expect(() => validateAgentRuntimeContextProjectionCommit(
      projection,
      sampleContextCoordinate(),
      null,
    )).toThrow("低于当前 required revision");
  });

  it("拒绝覆盖已提交 revision 的旧 context snapshot", () => {
    expect(() => validateAgentRuntimeContextProjectionCommit(
      sampleProjection(),
      sampleContextCoordinate(),
      43n,
    )).toThrow("低于当前 required revision");
  });

  it("拒绝同 revision 但 recipe coordinate 不一致的 snapshot", () => {
    const projection = sampleProjection();
    projection.recipe.coordinate.recipe_digest = "sha256:stale";
    expect(() => validateAgentRuntimeContextProjectionCommit(
      projection,
      sampleContextCoordinate(),
      null,
    )).toThrow("与 Runtime context coordinate 不一致");
  });

  it("允许高于 required revision 的新 context snapshot", () => {
    const projection = sampleProjection();
    projection.recipe.coordinate.snapshot_revision = 43n;
    expect(validateAgentRuntimeContextProjectionCommit(
      projection,
      sampleContextCoordinate(),
      42n,
    )).toBe(43n);
  });

  it("context projection 暂时落后时有限重试", async () => {
    const conflict = Object.assign(new Error("behind"), { status: 409 });
    mocks.fetchAgentRunRuntimeContextProjection
      .mockRejectedValueOnce(conflict)
      .mockResolvedValueOnce(sampleProjection());
    const controller = new AbortController();

    await expect(fetchAgentRuntimeContextProjectionWithRetry(
      { runId: "run-1", agentId: "agent-1" },
      sampleContextCoordinate(),
      controller.signal,
      [0],
    )).resolves.toEqual(sampleProjection());
    expect(mocks.fetchAgentRunRuntimeContextProjection).toHaveBeenCalledTimes(2);
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
      useRef: <T,>(initial: T) => ({ current: initial }),
      useState: <T,>(initial: T | (() => T)) => {
        const value = typeof initial === "function" ? (initial as () => T)() : initial;
        return [value, vi.fn() as (value: T | ((previous: T) => T)) => void];
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
  if (!isRecord(node) || !isRecord(node.props)) return null;
  if (node.props["aria-label"] === label) return { props: node.props };
  const children = Array.isArray(node.props.children)
    ? node.props.children
    : node.props.children == null
      ? []
      : [node.props.children];
  return findButtonByAriaLabel(children, label);
}

function sampleCompactCommand(
  overrides: Partial<SessionChatCommandModel> = {},
): SessionChatCommandModel {
  return {
    kind: "compact_context",
    command_id: "compact_context",
    runtimeCommand: "request_compaction",
    enabled: true,
    requires_input: false,
    executor_config_policy: "forbidden",
    ...overrides,
  };
}

function sampleProjection(): AgentRuntimeContextProjection {
  return {
    thread_id: "runtime-thread-1",
    recipe: {
      coordinate: {
        snapshot_revision: 42n,
        context_revision: "context-revision-2",
        recipe_digest: "sha256:recipe",
        authority: "agent_owned",
        fidelity: "exact",
      },
      usage: {
        estimated_total_tokens: 1_500n,
        categories: [
          {
            kind: "compaction_summary",
            label: "压缩摘要",
            source: "compaction",
            estimated_tokens: 900n,
          },
          {
            kind: "user_messages",
            label: "用户消息",
            source: "history",
            estimated_tokens: 600n,
          },
        ],
        messages: {
          user_message_tokens: 600n,
          assistant_message_tokens: 0n,
          tool_call_tokens: 0n,
          tool_result_tokens: 0n,
        },
        top_tools: [{
          name: "apply_patch",
          definition_tokens: 120n,
          call_tokens: 20n,
          result_tokens: 10n,
        }],
      },
      contributions: [
      {
        kind: "frame",
        frame: {
          id: "frame-1",
          kind: "compaction_summary",
          delivery_status: "applied_to_compacted_context",
          rendered_text: "压缩后的完整摘要",
          sections: [{
            kind: "compaction_summary",
            title: "摘要",
            summary: "压缩后的完整摘要",
            tokens_before: 1000,
            messages_compacted: 4,
            compaction_id: "compaction-1",
            projection_version: 2,
            strategy: "summary",
            trigger: "manual",
            phase: "completed",
            source_start_event_seq: 1n,
            source_end_event_seq: 30n,
            first_kept_event_seq: 31n,
            compacted_until_ref: null,
            timestamp_ms: 100,
          }],
          created_at_ms: 100,
        } as unknown as ContextFrame,
      },
      {
        kind: "tool",
        tool: {
          name: "apply_patch",
          description: "修改工作区文件",
          input_schema: { type: "object" },
          capability_key: "workspace.write",
          source: "native",
          tool_path: "workspace/apply_patch",
          context_usage_kind: "native_tools",
        },
      },
      {
        kind: "message",
        source_entry_id: "entry-31",
        role: "user",
        content: "保留的用户消息",
        tool_call_id: null,
        tool_calls: [],
        is_error: false,
      },
      ],
    },
  };
}

function sampleContextCoordinate() {
  return {
    snapshot_revision: 42n,
    context_revision: "context-revision-2",
    recipe_digest: "sha256:recipe",
    authority: "agent_owned" as const,
    fidelity: "exact" as const,
  };
}
