import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { SessionBaselineCapabilities } from "../../../types/context";
import type { ContentBlock } from "../model/types";
import { SessionCapabilityCard } from "./SessionCapabilityCard";

describe("SessionCapabilityCard", () => {
  it("优先按 skill_clusters 展示 provider 摘要和默认暴露 skill", () => {
    const html = renderToStaticMarkup(
      <SessionCapabilityCard
        block={capabilitiesBlock(clusterCapabilities())}
        defaultExpanded
      />,
    );

    expect(html).toContain("2 个 Provider");
    expect(html).toContain("Copilot Skills");
    expect(html).toContain("只展示默认暴露的 Copilot skill。");
    expect(html).toContain("更多 skill 可在 provider inventory 中查看。");
    expect(html).toContain("inventory 12");
    expect(html).toContain("copilot/config-edit");
    expect(html).toContain("Workspace Skills");
    expect(html).toContain("workspace/config-edit");
    expect(html).not.toContain("legacy-flat-only");
  });

  it("没有 cluster 时回退旧 flat skills", () => {
    const html = renderToStaticMarkup(
      <SessionCapabilityCard
        block={capabilitiesBlock({
          skills: [
            {
              name: "runtime-skill",
              description: "旧 flat skills 仍可展示",
              file_path: "workspace://skills/runtime-skill/SKILL.md",
            },
          ],
        })}
        defaultExpanded
      />,
    );

    expect(html).toContain("1 个默认暴露 Skill");
    expect(html).toContain("runtime-skill");
    expect(html).toContain("旧 flat skills 仍可展示");
  });

  it("跨 provider 同 local_name 时使用 capability key 区分条目", () => {
    const html = renderToStaticMarkup(
      <SessionCapabilityCard
        block={capabilitiesBlock(clusterCapabilities())}
        defaultExpanded
      />,
    );

    expect(html).toContain("copilot/config-edit");
    expect(html).toContain("workspace/config-edit");
    expect(html).toContain("Copilot Config");
    expect(html).toContain("Workspace Config");
  });
});

function capabilitiesBlock(capabilities: Partial<SessionBaselineCapabilities>): ContentBlock {
  return {
    type: "resource",
    resource: {
      uri: "agentdash://session-capabilities/baseline",
      mimeType: "application/json",
      text: JSON.stringify(capabilities),
    },
  };
}

function clusterCapabilities(): SessionBaselineCapabilities {
  return {
    skills: [
      {
        name: "legacy-flat-only",
        description: "cluster 存在时不优先展示旧 flat fallback",
        file_path: "workspace://skills/legacy-flat-only/SKILL.md",
      },
    ],
    skill_clusters: [
      {
        provider_key: "copilot",
        display_name: "Copilot Skills",
        model_summary: "Copilot provider default exposure.",
        ui_summary: "只展示默认暴露的 Copilot skill。",
        inventory_hint: "更多 skill 可在 provider inventory 中查看。",
        inventory_count: 12,
        default_exposed_skills: [
          {
            capability_key: "copilot/config-edit",
            provider_key: "copilot",
            local_name: "config-edit",
            display_name: "Copilot Config",
            description: "配置编辑 skill",
            file_path: "copilot://skills/config-edit/SKILL.md",
            exposure: "default_exposed",
          },
        ],
      },
      {
        provider_key: "workspace",
        display_name: "Workspace Skills",
        model_summary: "Workspace provider default exposure.",
        inventory_count: 1,
        default_exposed_skills: [
          {
            capability_key: "workspace/config-edit",
            provider_key: "workspace",
            local_name: "config-edit",
            display_name: "Workspace Config",
            description: "workspace 配置编辑 skill",
            file_path: "workspace://skills/config-edit/SKILL.md",
            exposure: "default_exposed",
          },
        ],
      },
    ],
    skill_diagnostics: [],
  };
}
