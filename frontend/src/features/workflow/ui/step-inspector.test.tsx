/**
 * StepInspector 单元测试。
 *
 * 验证两条关键行为：
 *  1. Overview / Detail tab 切换的渲染差异
 *  2. phase_node 的 Detail tab 仍可编辑 workflow contract（领域允许）
 *
 * 项目未引入 @testing-library/react，这里用 renderToStaticMarkup 校验静态产物。
 * 对于需要交互的 tab 切换测试，直接操控 useState 的 hook 不可行，
 * 因此拆成：默认 tab 渲染 + "Detail" tab 渲染（通过直接渲染 hideTabs=false
 * 下组件并观察默认/切换后 DOM 文本），通过 renderToStaticMarkup 只能得到
 * 初始状态。所以 "切换后" 场景用 hideTabs=true 模拟 Detail-only 视图即可。
 */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { LifecycleStepDefinition } from "../../../types";
import { createStepWorkflowDraft } from "../../../stores/workflowStore";
import { StepInspector } from "./step-inspector";

function makeStep(
  overrides: Partial<LifecycleStepDefinition> = {},
): LifecycleStepDefinition {
  return {
    key: "implement",
    description: "实现该需求",
    workflow_key: "demo.implement",
    node_type: "agent_node",
    output_ports: [],
    input_ports: [],
    capability_config: { tool_directives: [], mount_directives: [] },
    ...overrides,
  };
}

describe("StepInspector tabs", () => {
  it("默认渲染 Overview tab（非 Form 模式）", () => {
    const step = makeStep();
    const draft = createStepWorkflowDraft("p1", "demo", "implement", ["story"]);
    const markup = renderToStaticMarkup(
      <StepInspector
        step={step}
        workflowDraft={draft}
        isEntry
        availableWorkflows={[]}
        hookPresets={[]}
        targetKinds={["story"]}
        projectId="p1"
        onStepChange={() => undefined}
        onWorkflowChange={() => undefined}
      />,
    );
    expect(markup).toContain("Overview");
    expect(markup).toContain("Detail");
    // Overview 内容：节点类型 select、node key 输入、port 摘要标题
    expect(markup).toContain("节点类型");
    expect(markup).toContain("Output Ports");
    // Detail-only 内容（如 Session 指引）不应出现在默认 Overview 视图
    expect(markup).not.toContain("Session 指引");
  });

  it("hideTabs 模式（Form 模式）渲染完整 contract 而非 Overview", () => {
    const step = makeStep();
    const draft = createStepWorkflowDraft("p1", "demo", "implement", ["story"]);
    const markup = renderToStaticMarkup(
      <StepInspector
        step={step}
        workflowDraft={draft}
        isEntry
        hideTabs
        availableWorkflows={[]}
        hookPresets={[]}
        targetKinds={["story"]}
        projectId="p1"
        onStepChange={() => undefined}
        onWorkflowChange={() => undefined}
      />,
    );
    // Form 模式应直接渲染所有 panel（Session 指引 / Agent 工具能力 / Ports）
    expect(markup).toContain("Session 指引");
    expect(markup).toContain("Agent 工具能力");
    expect(markup).toContain("Ports");
    // Form 模式不应渲染 tab 切换按钮（Overview/Detail 文案仍会在 select 选项中出现，
    // 但没有 tab group）；通过 role="tablist" 或自定义类名判断；这里简单检查
    // 不应出现 tab 专用的 `bg-secondary/35 p-1` flex 容器
    expect(markup).not.toContain('flex shrink-0 gap-1 border-b border-border bg-secondary/35 p-1');
  });

  it("phase_node 也能渲染完整 contract（hideTabs 模拟 Detail tab）", () => {
    const step = makeStep({ node_type: "phase_node" });
    const draft = createStepWorkflowDraft("p1", "demo", "review", ["story"]);
    // 注入一条 hook rule，验证 phase_node 也能展示 contract 内容
    draft.contract.hook_rules = [
      {
        key: "audit",
        trigger: "after_tool",
        description: "记录工具调用",
        preset: "audit_tool",
        params: null,
        script: null,
        enabled: true,
      },
    ];
    const markup = renderToStaticMarkup(
      <StepInspector
        step={step}
        workflowDraft={draft}
        isEntry={false}
        hideTabs
        availableWorkflows={[]}
        hookPresets={[]}
        targetKinds={["story"]}
        projectId="p1"
        onStepChange={() => undefined}
        onWorkflowChange={() => undefined}
      />,
    );
    expect(markup).toContain("过程行为");
    // phase_node 的 hook rule 仍应可见，而不是 "Phase Node 仅作为 lifecycle 阶段标记" 之类的拒绝文案
    expect(markup).toContain("记录工具调用");
    expect(markup).not.toContain("Phase Node 仅作为 lifecycle 阶段标记");
  });

  it("phase_node 非 entry 时 node_type select 允许切换", () => {
    const step = makeStep({ node_type: "phase_node" });
    const draft = createStepWorkflowDraft("p1", "demo", "review", ["story"]);
    const markup = renderToStaticMarkup(
      <StepInspector
        step={step}
        workflowDraft={draft}
        isEntry={false}
        availableWorkflows={[]}
        hookPresets={[]}
        targetKinds={["story"]}
        projectId="p1"
        onStepChange={() => undefined}
        onWorkflowChange={() => undefined}
      />,
    );
    // Overview tab 的 node_type select 包含 agent_node / phase_node 两个选项，
    // phase_node 当前 selected
    expect(markup).toContain("Agent Node");
    expect(markup).toContain("Phase Node");
    // 非 entry 时 Phase Node 选项不应禁用
    expect(markup).not.toContain("Phase Node（入口不可用）");
  });
});
