import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AcpSystemEventCard } from "./AcpSystemEventCard";
import { isRenderableSystemEventUpdate } from "./AcpSystemEventGuard";

describe("AcpSystemEventCard", () => {
  it("让 hook_event 进入可见事件流并展示关键摘要", () => {
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
    expect(html).toContain("Hook 事件");
    expect(html).toContain("trigger: before_stop");
    expect(html).toContain("decision: continue");
    expect(html).toContain("completion: checklist_passed");
    expect(html).toContain("命中规则：workflow_completion:checklist_pending:stop_gate");
    expect(html).toContain("诊断 before_stop_checklist_pending：需要继续执行");
  });

  it("显示 turn_started 这类 info lifecycle 事件", () => {
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

    expect(isRenderableSystemEventUpdate(update)).toBe(true);

    const html = renderToStaticMarkup(<AcpSystemEventCard update={update} />);
    expect(html).toContain("开始执行");
    expect(html).toContain("turn: turn-2");
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
    expect(html).toContain("turn: turn-3");
  });

  it("展示 companion 回流事件中的 adoption 细节", () => {
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
    expect(html).toContain("Companion 结果可用");
    expect(html).toContain("adoption_mode：blocking_review");
    expect(html).toContain("slice_mode：compact");
    expect(html).toContain("摘要：请先处理 review 结论");
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
    expect(html).toContain("Hook 事项已结案");
    expect(html).toContain("action_id：blocking_review:dispatch-1:turn-1");
    expect(html).toContain("resolution_kind：adopted");
    expect(html).toContain("说明：主 session 已确认采纳 review 结论");
  });
});
