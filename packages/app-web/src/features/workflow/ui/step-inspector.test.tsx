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

import type { ActivityDefinition } from "../../../types";
import { createActivityWorkflowDraft } from "../../../stores/workflowStore";
import { StepInspector } from "./step-inspector";

function makeStep(
  overrides: Partial<ActivityDefinition> = {},
): ActivityDefinition {
  return {
    key: "implement",
    description: "实现该需求",
    executor: { kind: "agent", workflow_key: "demo.implement", session_policy: "spawn_child" },
    output_ports: [],
    input_ports: [],
    completion_policy: { kind: "executor_terminal" },
    iteration_policy: { max_attempts: 1, artifact_alias: "latest" },
    join_policy: "all",
    ...overrides,
  };
}

describe("StepInspector tabs", () => {
  it("默认渲染 Overview tab（非 Form 模式）", () => {
    const step = makeStep();
    const draft = createActivityWorkflowDraft("p1", "demo", "implement", ["story"]);
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
    // Overview 内容：executor select、node key 输入、port 摘要标题
    expect(markup).toContain("Executor");
    expect(markup).toContain("Output Ports");
    // Detail-only 内容（如 Session 指引）不应出现在默认 Overview 视图
    expect(markup).not.toContain("Session 指引");
  });

  it("hideTabs 模式（Form 模式）渲染完整 contract 而非 Overview", () => {
    const step = makeStep();
    const draft = createActivityWorkflowDraft("p1", "demo", "implement", ["story"]);
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

  it("continue_root agent activity 也能渲染完整 contract（hideTabs 模拟 Detail tab）", () => {
    const step = makeStep({
      executor: { kind: "agent", workflow_key: "demo.review", session_policy: "continue_root" },
    });
    const draft = createActivityWorkflowDraft("p1", "demo", "review", ["story"]);
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
    // ContinueRoot 的 hook rule 仍应可见，而不是被当作纯阶段标记。
    expect(markup).toContain("记录工具调用");
    expect(markup).not.toContain("仅作为 lifecycle 阶段标记");
  });

  it("非 entry 时 executor select 允许切到 Function", () => {
    const step = makeStep({
      executor: { kind: "agent", workflow_key: "demo.review", session_policy: "continue_root" },
    });
    const draft = createActivityWorkflowDraft("p1", "demo", "review", ["story"]);
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
    expect(markup).toContain("Agent");
    expect(markup).toContain("Human Approval");
    expect(markup).toContain("Function");
    expect(markup).not.toContain("Function（入口暂不用）");
  });
});
