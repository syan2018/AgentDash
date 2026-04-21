import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import {
  HookRuntimePendingActionsCard,
  HookRuntimeDiagnosticsCard,
  HookRuntimeSurfaceCard,
  HookRuntimeTraceCard,
} from "../features/session-context";
import type { HookSessionRuntimeInfo } from "../types";

const hookRuntime: HookSessionRuntimeInfo = {
  session_id: "sess-hook-test",
  revision: 7,
  snapshot: {
    session_id: "sess-hook-test",
    sources: [
      "builtin:global",
      "workflow:demo_lifecycle:check",
    ],
    owners: [
      {
        owner_type: "task",
        owner_id: "task-1",
        label: "Task A",
      },
    ],
    tags: ["workflow:demo_lifecycle", "workflow_step:check"],
    injections: [
      {
        slot: "context",
        content: "当前在 Check phase",
        source: "workflow:demo_lifecycle:check",
      },
      {
        slot: "constraint",
        content: "先给出验证结论，再结束 session。",
        source: "workflow:demo_lifecycle:check",
      },
      {
        slot: "workflow",
        content: "Check phase 需要先更新 Task 状态。",
        source: "workflow:demo_lifecycle:check",
      },
    ],
    diagnostics: [],
    metadata: {
      active_workflow: {
        lifecycle_id: "lc-1",
        lifecycle_key: "demo_lifecycle",
        lifecycle_name: "Demo Lifecycle / Task",
        run_id: "run-1",
        run_status: "running",
        step_key: "check",
        step_title: "Check",
        primary_workflow_id: "wf-1",
        workflow_key: "demo_lifecycle_check",
        primary_workflow_name: "Demo Task / Check",
      },
    },
  },
  diagnostics: [
    {
      code: "before_stop_checklist_pending",
      message: "当前 workflow phase 尚未满足 checklist completion 条件。",
    },
  ],
  trace: [
    {
      sequence: 3,
      timestamp_ms: 1_710_000_000_000,
      revision: 7,
      trigger: "before_stop",
      decision: "continue",
      subagent_type: "companion",
      matched_rule_keys: ["workflow_completion:checklist_pending:stop_gate"],
      refresh_snapshot: false,
      diagnostics: [
        {
          code: "before_stop_checklist_pending",
          message: "Hook 阻止当前 session 结束，要求先补齐验证。",
        },
      ],
      completion: {
        mode: "checklist_passed",
        satisfied: false,
        advanced: false,
        reason: "Task 还没有进入 awaiting_verification/completed。",
      },
    },
  ],
  pending_actions: [
    {
      id: "blocking_review:dispatch-1:turn-1",
      created_at_ms: 1_710_000_100_000,
      title: "Companion `reviewer` 结果需要阻塞式 review",
      summary: "status=completed, dispatch_id=dispatch-1, summary=请先处理 review 结论",
      action_type: "blocking_review",
      turn_id: "turn-parent-1",
      source_trigger: "subagent_result",
      status: "pending",
      last_injected_at_ms: 1_710_000_120_000,
      resolved_at_ms: null,
      resolution_kind: null,
      resolution_note: null,
      resolution_turn_id: null,
      injections: [
        {
          slot: "workflow",
          content: "请先处理 companion review 结论。",
          source: "workflow:demo_lifecycle:check",
        },
        {
          slot: "constraint",
          content: "主 session 必须先处理这份 companion review。",
          source: "workflow:demo_lifecycle:check",
        },
      ],
    },
    {
      id: "follow_up_required:dispatch-2:turn-2",
      created_at_ms: 1_710_000_200_000,
      title: "Companion `planner` 结果需要主 session 跟进",
      summary: "status=completed, dispatch_id=dispatch-2, summary=已经吸收 plan",
      action_type: "follow_up_required",
      turn_id: "turn-parent-2",
      source_trigger: "subagent_result",
      status: "resolved",
      last_injected_at_ms: 1_710_000_210_000,
      resolved_at_ms: 1_710_000_260_000,
      resolution_kind: "adopted",
      resolution_note: "主 session 已吸收并继续推进",
      resolution_turn_id: "turn-parent-2",
      injections: [],
    },
  ],
};

describe("SessionPage hook runtime cards", () => {
  it("渲染 runtime surface 中的 workflow / step metadata", () => {
    const html = renderToStaticMarkup(<HookRuntimeSurfaceCard hookRuntime={hookRuntime} />);

    expect(html).toContain("运行中 Hook Runtime");
    expect(html).toContain("sources: 2");
    expect(html).toContain("injections: 3");
    expect(html).toContain("actions: 2");
    expect(html).toContain("open: 1");
    expect(html).toContain("resolved: 1");
    expect(html).toContain("Demo Lifecycle / Task / Check");
    expect(html).toContain("builtin:global");
    expect(html).toContain("workflow:demo_lifecycle:check");
    expect(html).toContain("step: check");
    expect(html).toContain("workflow: demo_lifecycle_check");
  });

  it("渲染 diagnostics 与 trace 细节", () => {
    const diagnosticsHtml = renderToStaticMarkup(
      <HookRuntimeDiagnosticsCard hookRuntime={hookRuntime} />,
    );
    const traceHtml = renderToStaticMarkup(<HookRuntimeTraceCard hookRuntime={hookRuntime} />);

    expect(diagnosticsHtml).toContain("before_stop_checklist_pending");
    expect(diagnosticsHtml).toContain("当前 workflow phase 尚未满足 checklist completion 条件");
    expect(traceHtml).toContain("workflow_completion:checklist_pending:stop_gate");
    expect(traceHtml).toContain("subagent: companion");
    expect(traceHtml).toContain("completion: checklist_passed");
    expect(traceHtml).toContain("Task 还没有进入 awaiting_verification/completed。");
  });

  it("渲染 pending hook actions", () => {
    const html = renderToStaticMarkup(<HookRuntimePendingActionsCard hookRuntime={hookRuntime} />);

    expect(html).toContain("干预项状态");
    expect(html).toContain("blocking_review");
    expect(html).toContain("pending");
    expect(html).toContain("Companion `reviewer` 结果需要阻塞式 review");
    expect(html).toContain("主 session 必须先处理这份 companion review");
    expect(html).toContain("resolved");
    expect(html).toContain("resolution: adopted");
    expect(html).toContain("结案说明：主 session 已吸收并继续推进");
  });
});
