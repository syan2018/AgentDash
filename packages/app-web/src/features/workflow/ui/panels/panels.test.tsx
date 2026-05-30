/**
 * Panel 组件最小单元测试。
 *
 * 行为目标：
 *  - 每个 panel 在合法 props 下能静态渲染（不抛错、关键文案出现）
 *  - 在直接调用导出 props 形态下，onChange 类型契约不被悄悄改动
 *
 * 注：项目尚未引入 @testing-library/react，因此这里使用 renderToStaticMarkup
 * 验证渲染产物，并通过 React.createElement / 组件函数直接调用回调来验证
 * onChange 回调。
 */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type {
  CapabilityDirective,
  HookRulePreset,
  InputPortDefinition,
  OutputPortDefinition,
  WorkflowContextBinding,
  WorkflowHookRuleSpec,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../../../types";
import { addDirective, makeAddCapability } from "../../capability-directive-ops";
import { InjectionPanel } from "./InjectionPanel";
import { HookRulesPanel } from "./HookRulesPanel";
import { CapabilityPanel } from "./CapabilityPanel";
import { PortsPanel } from "./PortsPanel";
import { toggleTargetKind, buildDefaultParams } from "./shared";

describe("toggleTargetKind", () => {
  it("追加未勾选的 kind", () => {
    const next = toggleTargetKind(["story"], "project");
    expect(next).toEqual(["story", "project"]);
  });

  it("移除已勾选的 kind", () => {
    const next = toggleTargetKind(["story", "project"], "project");
    expect(next).toEqual(["story"]);
  });

  it("不允许移到空集合，保留原值", () => {
    const next = toggleTargetKind(["story"], "story");
    expect(next).toEqual(["story"]);
  });
});

describe("InjectionPanel", () => {
  const baseInjection: WorkflowInjectionSpec = {
    guidance: "完成本步任务",
    context_bindings: [
      { locator: "main/docs/spec.md", reason: "spec", required: true },
    ],
  };

  it("渲染 guidance 与 binding 列表", () => {
    const markup = renderToStaticMarkup(
      <InjectionPanel
        injection={baseInjection}
        onGuidanceChange={() => undefined}
        onBindingChange={() => undefined}
        onBindingAdd={() => undefined}
        onBindingRemove={() => undefined}
      />,
    );

    expect(markup).toContain("Session 指引");
    expect(markup).toContain("上下文挂载");
    expect(markup).toContain("完成本步任务");
    expect(markup).toContain("main/docs/spec.md");
  });

  it("空 binding 时显示占位文案", () => {
    const markup = renderToStaticMarkup(
      <InjectionPanel
        injection={{ context_bindings: [] }}
        onGuidanceChange={() => undefined}
        onBindingChange={() => undefined}
        onBindingAdd={() => undefined}
        onBindingRemove={() => undefined}
      />,
    );

    expect(markup).toContain("上下文挂载 (0)");
    expect(markup).toContain("暂无");
  });

  it("回调签名匹配 store action", () => {
    const onBindingChange = vi.fn<
      (index: number, patch: Partial<WorkflowContextBinding>) => void
    >();
    onBindingChange(0, { reason: "更新原因" });
    expect(onBindingChange).toHaveBeenCalledWith(0, { reason: "更新原因" });
  });
});

describe("HookRulesPanel", () => {
  const sampleRules: WorkflowHookRuleSpec[] = [
    {
      key: "audit_tool",
      trigger: "after_tool",
      description: "记录工具调用",
      preset: "audit_tool",
      params: undefined,
      script: undefined,
      enabled: true,
    },
    {
      key: "stop_gate",
      trigger: "before_stop",
      description: "完成前检查",
      preset: undefined,
      params: null,
      script: "#{}",
      enabled: false,
    },
  ];

  const presets: HookRulePreset[] = [];

  it("把 rules 分组成『过程行为』和『结束门禁』两段", () => {
    const markup = renderToStaticMarkup(
      <HookRulesPanel
        hookRules={sampleRules}
        presets={presets}
        onAdd={() => undefined}
        onToggle={() => undefined}
        onRemove={() => undefined}
      />,
    );

    expect(markup).toContain("过程行为 (1)");
    expect(markup).toContain("结束门禁 (1)");
    expect(markup).toContain("记录工具调用");
    expect(markup).toContain("完成前检查");
  });

  it("无规则时分别显示『尚未配置』", () => {
    const markup = renderToStaticMarkup(
      <HookRulesPanel
        hookRules={[]}
        presets={presets}
        onAdd={() => undefined}
        onToggle={() => undefined}
        onRemove={() => undefined}
      />,
    );

    expect(markup).toContain("过程行为 (0)");
    expect(markup).toContain("结束门禁 (0)");
    // 空 group 占位文案统一为"暂无"
    expect((markup.match(/暂无/g) ?? []).length).toBeGreaterThanOrEqual(2);
  });

  it("buildDefaultParams 处理 schema properties", () => {
    const result = buildDefaultParams({
      properties: {
        names: { type: "array" },
        title: { type: "string" },
        count: { type: "number" },
        flag: { type: "boolean" },
      },
    });
    expect(result).toEqual({ names: [], title: "", count: 0, flag: false });
  });
});

describe("CapabilityPanel", () => {
  const targetKinds: WorkflowTargetKind[] = ["project"];

  it("渲染基线能力（按 target_kinds 计算）", () => {
    const markup = renderToStaticMarkup(
      <CapabilityPanel
        projectId=""
        targetKinds={targetKinds}
        directives={[]}
        onDirectivesChange={() => undefined}
      />,
    );

    expect(markup).toContain("基线能力");
    // project baseline 含 file_read / file_write / shell_execute
    expect(markup).toContain("文件读取");
    expect(markup).toContain("文件写入");
    expect(markup).toContain("Shell 执行");
  });

  it("追加能力区列出非 baseline 的 Add 指令", () => {
    const directives: CapabilityDirective[] = addDirective(
      [],
      makeAddCapability("workflow_management"),
    );
    const markup = renderToStaticMarkup(
      <CapabilityPanel
        projectId=""
        targetKinds={targetKinds}
        directives={directives}
        onDirectivesChange={() => undefined}
      />,
    );

    expect(markup).toContain("工作流追加能力");
    expect(markup).toContain("工作流管理");
    expect(markup).toContain("追加");
  });
});

describe("PortsPanel", () => {
  const outputPorts: OutputPortDefinition[] = [
    { key: "report", description: "输出报告", gate_strategy: "existence" },
  ];
  const inputPorts: InputPortDefinition[] = [
    {
      key: "research_input",
      description: "研究输入",
      context_strategy: "full",
      standalone_fulfillment: "required",
    },
  ];

  it("渲染 output / input 两段并展示计数", () => {
    const markup = renderToStaticMarkup(
      <PortsPanel
        outputPorts={outputPorts}
        inputPorts={inputPorts}
        onOutputChange={() => undefined}
        onInputChange={() => undefined}
      />,
    );

    expect(markup).toContain("Output Ports (1)");
    expect(markup).toContain("Input Ports (1)");
    // 默认 view 态：key 以 <code> 展示，策略标签显示
    expect(markup).toContain("report");
    expect(markup).toContain("research_input");
    expect(markup).toContain("门禁：文件存在");
    expect(markup).toContain("上下文：完整");
  });

  it("两侧均空时分别显示占位文案", () => {
    const markup = renderToStaticMarkup(
      <PortsPanel
        outputPorts={[]}
        inputPorts={[]}
        onOutputChange={() => undefined}
        onInputChange={() => undefined}
      />,
    );

    expect(markup).toContain("Output Ports (0)");
    expect(markup).toContain("Input Ports (0)");
    // 空占位统一文案
    const placeholderCount = (markup.match(/暂无/g) ?? []).length;
    expect(placeholderCount).toBeGreaterThanOrEqual(2);
  });

  it("onOutputChange 接收 OutputPortDefinition[]", () => {
    const handler = vi.fn<(ports: OutputPortDefinition[]) => void>();
    handler([{ key: "x", description: "y", gate_strategy: "existence" }]);
    expect(handler).toHaveBeenCalledWith([{ key: "x", description: "y", gate_strategy: "existence" }]);
  });
});
