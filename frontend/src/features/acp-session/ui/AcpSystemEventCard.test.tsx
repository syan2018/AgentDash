import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AcpSystemEventCard } from "./AcpSystemEventCard";
import { isRenderableSystemEventUpdate } from "./AcpSystemEventGuard";

describe("AcpSystemEventCard", () => {
  it("让 hook_event 进入可见事件流并展示用户级摘要", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-1" },
          event: {
            type: "hook_event",
            severity: "warning",
            code: "hook:before_stop:continue",
            message: "Hook 阻止了当前结束并要求继续执行",
            data: {
              trigger: "before_stop",
              decision: "continue",
              sequence: 3,
              revision: 7,
              matched_rule_keys: ["workflow_completion:checklist_pending:stop_gate"],
              completion: {
                mode: "checklist_passed",
                satisfied: false,
                advanced: false,
                reason: "需要继续补齐验证结论",
              },
              diagnostic_codes: ["before_stop_checklist_pending"],
              diagnostics: [
                {
                  code: "before_stop_checklist_pending",
                  summary: "需要继续执行",
                },
              ],
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(true);

    const html = renderToStaticMarkup(<AcpSystemEventCard update={update} />);
    // 用户级信息：消息、完成判定
    expect(html).toContain("Hook 阻止了当前结束并要求继续执行");
    expect(html).toContain("完成判定：未满足");
    // 调试详情默认折叠——不在初始渲染中
    expect(html).not.toContain("trigger: before_stop");
    expect(html).not.toContain("decision: continue");
    // 调试内容默认折叠，不直接进入首屏摘要
    expect(html).not.toContain("命中规则");
  });

  it("静默 turn_started 这类 info lifecycle 事件", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-2" },
          event: {
            type: "turn_started",
            severity: "info",
            message: "开始执行",
            data: null,
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(false);
  });

  it("未知 system event 即使标记 warning 也不进入可见事件流", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-unknown-1" },
          event: {
            type: "unexpected_warning",
            severity: "warning",
            message: "后端发来了未注册事件",
            data: null,
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(false);
  });

  it("显示 turn_interrupted 并保留中断原因", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-3" },
          event: {
            type: "turn_interrupted",
            severity: "warning",
            message: "执行已取消",
            data: null,
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(true);

    const html = renderToStaticMarkup(<AcpSystemEventCard update={update} />);
    expect(html).toContain("执行已中断");
    expect(html).toContain("执行已取消");
  });

  it("展示 companion 回流事件中的用户级摘要", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-parent-1" },
          event: {
            type: "companion_result_available",
            severity: "warning",
            message: "Companion 已回传结果，等待主 session 采纳",
            data: {
              companion_label: "reviewer",
              dispatch_id: "dispatch-1",
              companion_session_id: "sess-companion-1",
              adoption_mode: "blocking_review",
              slice_mode: "compact",
              status: "completed",
              summary: "请先处理 review 结论",
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    const html = renderToStaticMarkup(<AcpSystemEventCard update={update} />);
    expect(html).toContain("协作结果可用");
    // 用户级信息：companion 名称、状态、摘要
    expect(html).toContain("协作 Agent：reviewer");
    expect(html).toContain("摘要：请先处理 review 结论");
    // 内部字段不在用户级详情中
    expect(html).not.toContain("adoption_mode");
    expect(html).not.toContain("slice_mode");
  });

  it("静默 allow / effects_applied / noop 等高频无信息量 hook 决策", () => {
    for (const [trigger, decision] of [
      ["before_tool", "allow"],
      ["after_tool", "effects_applied"],
      ["after_turn", "noop"],
      ["before_compact", "noop"],
      ["after_compact", "notified"],
      ["session_start", "baseline_initialized"],
      ["session_start", "baseline_refreshed"],
    ] as const) {
      const update = {
        sessionUpdate: "session_info_update",
        _meta: {
          agentdash: {
            v: 1,
            trace: { turnId: "turn-silent-1" },
            event: {
              type: "hook_event",
              severity: "info",
              code: `hook:${trigger}:${decision}`,
              message: `Hook 在 ${trigger} 阶段产生了 ${decision} 决策`,
              data: {
                trigger,
                decision,
                sequence: 1,
                revision: 1,
                matched_rule_keys: ["some_rule:key"],
                diagnostics: [],
                injections: [],
              },
            },
          },
        },
      } as unknown as SessionUpdate;

      expect(
        isRenderableSystemEventUpdate(update),
        `${trigger}:${decision} should be silent`,
      ).toBe(false);
    }
  });

  it("静默决策若携带 diagnostics 或 injections 则仍显示", () => {
    const withDiagnostics = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-diag-1" },
          event: {
            type: "hook_event",
            severity: "info",
            code: "hook:after_tool:effects_applied",
            message: "Hook 在 after_tool 阶段产生了 effects_applied 决策",
            data: {
              trigger: "after_tool",
              decision: "effects_applied",
              sequence: 1,
              revision: 1,
              matched_rule_keys: [],
              diagnostics: [{ code: "lint_warning", message: "发现潜在问题" }],
              injections: [],
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(withDiagnostics)).toBe(true);

    const withInjections = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-inj-1" },
          event: {
            type: "hook_event",
            severity: "info",
            code: "hook:before_tool:allow",
            message: "Hook 放行了当前工具调用",
            data: {
              trigger: "before_tool",
              decision: "allow",
              sequence: 1,
              revision: 1,
              matched_rule_keys: [],
              diagnostics: [],
              injections: [{ slot: "context", source: "rule:x", content: "额外上下文" }],
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(withInjections)).toBe(true);
  });

  it("静默决策仅携带 session_binding_found 诊断时仍应静默", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-noise-diag-1" },
          event: {
            type: "hook_event",
            severity: "info",
            code: "hook:before_tool:allow",
            message: "Hook 放行了当前工具调用",
            data: {
              trigger: "before_tool",
              decision: "allow",
              sequence: 1,
              revision: 1,
              matched_rule_keys: [],
              diagnostics: [
                { code: "session_binding_found", message: "命中会话绑定 project:xxx" },
              ],
              injections: [],
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(false);
  });

  it("展示 hook action 显式结案事件", () => {
    const update = {
      sessionUpdate: "session_info_update",
      _meta: {
        agentdash: {
          v: 1,
          trace: { turnId: "turn-parent-2" },
          event: {
            type: "hook_action_resolved",
            severity: "info",
            message: "Hook action `Companion review` 已显式结案",
            data: {
              action_id: "blocking_review:dispatch-1:turn-1",
              action_type: "blocking_review",
              status: "resolved",
              resolution_kind: "adopted",
              resolution_turn_id: "turn-parent-2",
              summary: "status=completed, dispatch_id=dispatch-1, summary=请先处理 review 结论",
              resolution_note: "主 session 已确认采纳 review 结论",
            },
          },
        },
      },
    } as unknown as SessionUpdate;

    expect(isRenderableSystemEventUpdate(update)).toBe(true);

    const html = renderToStaticMarkup(<AcpSystemEventCard update={update} />);
    expect(html).toContain("事项已结案");
    // 用户级：摘要和说明
    expect(html).toContain("说明：主 session 已确认采纳 review 结论");
    // 内部字段不在用户级详情中
    expect(html).not.toContain("action_id");
    expect(html).not.toContain("resolution_kind");
  });
});
