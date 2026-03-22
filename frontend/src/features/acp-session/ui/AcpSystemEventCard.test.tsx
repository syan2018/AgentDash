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
});
