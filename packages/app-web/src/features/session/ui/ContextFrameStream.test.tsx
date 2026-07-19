import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { parseContextFrame, type ContextFrame } from "../model/contextFrame";
import { ContextFrameStream } from "./ContextFrameStream";

describe("ContextFrameStream", () => {
  it("identity 与 system_guidelines 并存时同时展示且按 delivery 顺序排列", () => {
    // 后端投递时 identity(#10) 在 system_guidelines(#20) 之前；stream 需按
    // delivery phase/order 稳定排序，保证两帧同时可见且顺序正确。
    const frames = [
      readFrame(sampleGuidelinesFrame()),
      readFrame(sampleIdentityFrame()),
    ];

    const markup = renderToStaticMarkup(
      <ContextFrameStream frames={frames} defaultExpanded />,
    );

    // 两帧的外层 tab 标签都出现，说明列表未吞掉其中之一。
    expect(markup).toContain("IDENTITY");
    expect(markup).toContain("GUIDELINES");
    // 汇总行同时列出两类 frame。
    expect(markup).toContain("IDENTITY / GUIDELINES");

    // identity 在 system_guidelines 之前渲染（stable_system 早于 session_policy）。
    // 以每帧唯一的 tab 标签定位，避免与汇总行 label 子串误匹配。
    const identityIndex = markup.indexOf("identity_system_prompt");
    const guidelinesIndex = markup.indexOf("1 prefs / 1 files");
    expect(identityIndex).toBeGreaterThanOrEqual(0);
    expect(guidelinesIndex).toBeGreaterThanOrEqual(0);
    expect(identityIndex).toBeLessThan(guidelinesIndex);

    // delivery 元信息按 kind 派生：identity=stable_system #10、
    // system_guidelines=session_policy #20，均走 system model channel。
    expect(markup).toContain("stable_system #10");
    expect(markup).toContain("session_policy #20");
  });

  it("system_guidelines 展开后同时展示 User Preferences 与 Project Guidelines", () => {
    const frames = [readFrame(sampleGuidelinesFrame())];

    const markup = renderToStaticMarkup(
      <ContextFrameStream frames={frames} defaultExpanded />,
    );

    expect(markup).toContain("User Preferences");
    expect(markup).toContain("Project Guidelines");
    expect(markup).toContain("使用中文");
    expect(markup).toContain("AGENTS.md");
    expect(markup).toContain("项目约定");
  });
});

function readFrame(value: Record<string, unknown>): ContextFrame {
  const frame = parseContextFrame(value);
  if (!frame) {
    throw new Error("invalid context frame test fixture");
  }
  return frame;
}

function sampleIdentityFrame(): Record<string, unknown> {
  return {
    id: "identity-1",
    kind: "identity",
    source: "runtime_context_update",
    delivery_status: "prepared_for_connector",
    delivery_channel: "connector_context",
    message_role: "system",
    rendered_text: "## Identity\n\n你是 AgentDash 内置编码助手。",
    created_at_ms: 1,
    sections: [
      {
        kind: "identity",
        title: "Identity",
        summary: "Connector 启动时使用的稳定 system identity。",
        fragments: [
          {
            slot: "identity",
            label: "identity_system_prompt",
            source: "connector",
            content: "## System Prompt\n你是 AgentDash 内置编码助手。",
          },
        ],
      },
    ],
  };
}

function sampleGuidelinesFrame(): Record<string, unknown> {
  return {
    id: "system-guidelines-1",
    kind: "system_guidelines",
    source: "runtime_context_update",
    delivery_status: "prepared_for_connector",
    delivery_channel: "connector_context",
    message_role: "system",
    rendered_text:
      "## User Preferences\n\n- 使用中文\n\n## Project Guidelines\n\n### AGENTS.md\n\n项目约定",
    created_at_ms: 2,
    sections: [
      {
        kind: "user_preferences",
        title: "User Preferences",
        summary: "用户级偏好设置。",
        items: ["使用中文"],
      },
      {
        kind: "project_guidelines",
        title: "Project Guidelines",
        summary: "工作区中发现的项目级指引文件。",
        entries: [
          {
            path: "AGENTS.md",
            content: "项目约定",
          },
        ],
      },
    ],
  };
}
