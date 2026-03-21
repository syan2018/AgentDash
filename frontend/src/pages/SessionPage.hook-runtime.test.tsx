import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import {
  HookRuntimeDiagnosticsCard,
  HookRuntimeSurfaceCard,
  HookRuntimeTraceCard,
} from "./SessionPage";
import type { HookSessionRuntimeInfo } from "../types";

const hookRuntime: HookSessionRuntimeInfo = {
  session_id: "sess-hook-test",
  revision: 7,
  snapshot: {
    session_id: "sess-hook-test",
    owners: [
      {
        owner_type: "task",
        owner_id: "task-1",
        label: "Task A",
      },
    ],
    tags: ["workflow:trellis_dev_task", "workflow_phase:check"],
    context_fragments: [
      {
        slot: "workflow",
        label: "active_workflow_phase",
        content: "当前在 Check phase",
        source_summary: ["workflow:trellis_dev_task"],
      },
    ],
    constraints: [
      {
        key: "before_stop:checklist_pending",
        description: "先给出验证结论，再结束 session。",
        source_summary: ["workflow_phase:check"],
      },
    ],
    policies: [
      {
        key: "workflow:*:*:task_status_gate",
        description: "Check phase 需要先更新 Task 状态。",
      },
    ],
    diagnostics: [],
    metadata: {
      active_workflow: {
        workflow_id: "wf-1",
        workflow_key: "trellis_dev_task",
        workflow_name: "Trellis Dev Workflow / Task",
        run_id: "run-1",
        run_status: "running",
        phase_key: "check",
        phase_title: "Check",
        completion_mode: "checklist_passed",
        requires_session: true,
      },
    },
  },
  diagnostics: [
    {
      code: "before_stop_checklist_pending",
      summary: "当前 workflow phase 尚未满足 checklist completion 条件。",
      detail: "current_task_status=running",
      source_summary: ["workflow_phase:check"],
    },
  ],
  trace: [
    {
      sequence: 3,
      timestamp_ms: 1_710_000_000_000,
      revision: 7,
      trigger: "before_stop",
      decision: "continue",
      matched_rule_keys: ["workflow_completion:checklist_pending:stop_gate"],
      refresh_snapshot: false,
      diagnostics: [
        {
          code: "before_stop_checklist_pending",
          summary: "Hook 阻止当前 session 结束，要求先补齐验证。",
          detail: null,
          source_summary: ["workflow_phase:check"],
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
};

describe("SessionPage hook runtime cards", () => {
  it("渲染 runtime surface 中的 policy 和 workflow metadata", () => {
    const html = renderToStaticMarkup(<HookRuntimeSurfaceCard hookRuntime={hookRuntime} />);

    expect(html).toContain("运行中 Hook Runtime");
    expect(html).toContain("policies: 1");
    expect(html).toContain("workflow:*:*:task_status_gate");
    expect(html).toContain("Trellis Dev Workflow / Task / Check");
    expect(html).toContain("checklist_passed");
  });

  it("渲染 diagnostics 与 trace 细节", () => {
    const diagnosticsHtml = renderToStaticMarkup(
      <HookRuntimeDiagnosticsCard hookRuntime={hookRuntime} />,
    );
    const traceHtml = renderToStaticMarkup(<HookRuntimeTraceCard hookRuntime={hookRuntime} />);

    expect(diagnosticsHtml).toContain("before_stop_checklist_pending");
    expect(diagnosticsHtml).toContain("当前 workflow phase 尚未满足 checklist completion 条件");
    expect(traceHtml).toContain("workflow_completion:checklist_pending:stop_gate");
    expect(traceHtml).toContain("completion: checklist_passed");
    expect(traceHtml).toContain("Task 还没有进入 awaiting_verification/completed。");
  });
});
