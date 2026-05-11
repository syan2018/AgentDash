/**
 * StepInspector —— 单个 Lifecycle Step 的 inline inspector。
 *
 * DAG 模式下选中节点后在右侧栏直接渲染（替代原 `DagSidePanel` + `DetailPanel` 的抽屉嵌套）。
 * Form 模式下则平铺作为唯一 step 的完整表单。
 *
 * 本组件受控：只接受 step + 对应 step 的 workflow draft 和回调；不依赖 workflowStore。
 *
 * ## 顶部 Tab
 *
 * DAG 模式（默认）下顶部有两个 tab：
 *   - **Overview**：节点外部接口视图（key/name/description/node_type/ports），
 *     对应 DAG 画布上一眼看到的标注信息，是编排者视角。
 *   - **Detail**：完整 5 panel workflow contract 编辑（Injection / Capability /
 *     Hooks / Ports），是 step 行为约束的细节视角。
 *
 * Form 模式下传 `hideTabs`，只渲染 Detail 内容 + 顶部 step 基础信息。
 *
 * ## node_type 约束
 *
 * 领域层 `LifecycleStepDefinition.workflow_key` 不限定 node_type，phase_node
 * 与 agent_node 一样可以绑定 workflow contract。唯一硬约束是 entry step 必须是
 * agent_node（`validate_lifecycle_definition`）。因此本组件对 phase_node 一视同仁
 * 展示 5 panel 编辑器。
 */

import { useCallback, useState } from "react";

import type {
  CapabilityDirective,
  HookRulePreset,
  InputPortDefinition,
  LifecycleNodeType,
  LifecycleStepDefinition,
  OutputPortDefinition,
  WorkflowContextBinding,
  WorkflowDefinition,
  WorkflowHookRuleSpec,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../../types";
import type { WorkflowEditorDraft } from "../../../stores/workflowStore";
import {
  CapabilityPanel,
  HookRulesPanel,
  InjectionPanel,
  PortsPanel,
} from "./panels";

type InspectorTab = "overview" | "detail";

export interface StepInspectorProps {
  step: LifecycleStepDefinition;
  /** 当前 step 对应的 workflow draft —— 每个 step 都有（包含 phase_node）。 */
  workflowDraft: WorkflowEditorDraft;
  /** 是否是入口节点 —— 入口不允许 phase_node。 */
  isEntry: boolean;
  /** 是否隐藏"设为入口"/"删除"按钮（Form 模式下隐藏）。 */
  hideStepActions?: boolean;
  /** 隐藏 Overview/Detail tab（Form 模式只展示 Detail 内容）。 */
  hideTabs?: boolean;
  /** 项目级所有 workflow —— 克隆 popover 的数据源。 */
  availableWorkflows: WorkflowDefinition[];
  /** Hook preset 列表。 */
  hookPresets: HookRulePreset[];
  /** 当前 target kinds（来自 lifecycle 顶层，影响 capability baseline 展示）。 */
  targetKinds: WorkflowTargetKind[];
  /** Project id（CapabilityPanel 需要）。 */
  projectId: string;
  /** 回调：更新 step 顶层字段（key / description / node_type / ports / capability_config）。 */
  onStepChange: (patch: Partial<LifecycleStepDefinition>) => void;
  /** 回调:更新 step 对应的 workflow draft。 */
  onWorkflowChange: (patch: Partial<WorkflowEditorDraft>) => void;
  /** 入口切换。 */
  onSetEntry?: () => void;
  /** 删除此 step。 */
  onRemove?: () => void;
  /** 关闭 inspector（取消选中）。 */
  onClose?: () => void;
  /** 从已有 Workflow 克隆 contract。 */
  onCloneFromWorkflow?: (wf: WorkflowDefinition) => void;
}

export function StepInspector(props: StepInspectorProps) {
  const {
    step,
    workflowDraft,
    isEntry,
    hideStepActions = false,
    hideTabs = false,
    availableWorkflows,
    hookPresets,
    targetKinds,
    projectId,
    onStepChange,
    onWorkflowChange,
    onSetEntry,
    onRemove,
    onClose,
    onCloneFromWorkflow,
  } = props;

  const [activeTab, setActiveTab] = useState<InspectorTab>("overview");
  const nodeType = step.node_type ?? "agent_node";

  // ─── Workflow contract onChange 适配器 ───
  const updateInjection = useCallback(
    (patch: Partial<WorkflowInjectionSpec>) => {
      onWorkflowChange({
        contract: {
          ...workflowDraft.contract,
          injection: { ...workflowDraft.contract.injection, ...patch },
        },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  const handleBindingChange = useCallback(
    (idx: number, patch: Partial<WorkflowContextBinding>) => {
      const next = workflowDraft.contract.injection.context_bindings.map((b, i) =>
        i === idx ? { ...b, ...patch } : b,
      );
      updateInjection({ context_bindings: next });
    },
    [workflowDraft, updateInjection],
  );

  const handleBindingAdd = useCallback(() => {
    const next: WorkflowContextBinding[] = [
      ...workflowDraft.contract.injection.context_bindings,
      { locator: "", reason: "", required: true, title: null },
    ];
    updateInjection({ context_bindings: next });
  }, [workflowDraft, updateInjection]);

  const handleBindingRemove = useCallback(
    (idx: number) => {
      const next = workflowDraft.contract.injection.context_bindings.filter((_, i) => i !== idx);
      updateInjection({ context_bindings: next });
    },
    [workflowDraft, updateInjection],
  );

  const handleAddHookRule = useCallback(
    (rule: WorkflowHookRuleSpec) => {
      const exists = workflowDraft.contract.hook_rules.some((r) => r.key === rule.key);
      if (exists) return;
      onWorkflowChange({
        contract: {
          ...workflowDraft.contract,
          hook_rules: [...workflowDraft.contract.hook_rules, rule],
        },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  const handleToggleHookRule = useCallback(
    (key: string) => {
      const next = workflowDraft.contract.hook_rules.map((r) =>
        r.key === key ? { ...r, enabled: !r.enabled } : r,
      );
      onWorkflowChange({
        contract: { ...workflowDraft.contract, hook_rules: next },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  const handleRemoveHookRule = useCallback(
    (key: string) => {
      const next = workflowDraft.contract.hook_rules.filter((r) => r.key !== key);
      onWorkflowChange({
        contract: { ...workflowDraft.contract, hook_rules: next },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  const handleDirectivesChange = useCallback(
    (next: CapabilityDirective[]) => {
      onWorkflowChange({
        contract: {
          ...workflowDraft.contract,
          capability_config: {
            ...workflowDraft.contract.capability_config,
            tool_directives: next,
          },
        },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  const handleOutputPortsChange = useCallback(
    (ports: OutputPortDefinition[]) => {
      // Port 以 step 为真相源
      onStepChange({ output_ports: ports });
      onWorkflowChange({
        contract: { ...workflowDraft.contract, output_ports: ports },
      });
    },
    [onStepChange, onWorkflowChange, workflowDraft],
  );

  const handleInputPortsChange = useCallback(
    (ports: InputPortDefinition[]) => {
      onStepChange({ input_ports: ports });
      onWorkflowChange({
        contract: { ...workflowDraft.contract, input_ports: ports },
      });
    },
    [onStepChange, onWorkflowChange, workflowDraft],
  );

  const showDetail = hideTabs || activeTab === "detail";
  const showOverview = !hideTabs && activeTab === "overview";
  // Detail 编辑用 compact 模式（侧栏窄宽度）；Form 模式不 compact
  const compact = !hideTabs;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <div className="overflow-hidden">
          <p className="truncate text-sm font-semibold text-foreground">
            {step.key || "(no key)"}
          </p>
          {isEntry ? (
            <p className="text-[10px] text-emerald-600">入口节点</p>
          ) : onSetEntry ? (
            <button
              type="button"
              onClick={onSetEntry}
              className="mt-0.5 rounded-[6px] px-1.5 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/10"
            >
              设为入口
            </button>
          ) : null}
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            title="关闭面板"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
          </button>
        )}
      </div>

      {/* Tabs */}
      {!hideTabs && (
        <div className="flex shrink-0 gap-1 border-b border-border bg-secondary/35 p-1">
          {(["overview", "detail"] as const).map((key) => {
            const label = key === "overview" ? "Overview" : "Detail";
            const active = activeTab === key;
            return (
              <button
                key={key}
                type="button"
                onClick={() => setActiveTab(key)}
                className={`flex-1 rounded-[8px] px-2 py-1.5 text-xs font-medium transition-colors ${
                  active
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {label}
              </button>
            );
          })}
        </div>
      )}

      {/* 滚动内容 */}
      <div className="flex-1 overflow-y-auto">
        <div className="space-y-4 p-4">
          {showOverview && (
            <OverviewTab
              step={step}
              nodeType={nodeType}
              isEntry={isEntry}
              availableWorkflows={availableWorkflows}
              onStepChange={onStepChange}
              onCloneFromWorkflow={onCloneFromWorkflow}
            />
          )}

          {showDetail && (
            <>
              {/* Form 模式（hideTabs）下在 Detail 前面额外展示 step key + 描述
                  （Overview tab 里的基础信息） */}
              {hideTabs && (
                <section className="space-y-3">
                  <div>
                    <label className="agentdash-form-label">Node Key</label>
                    <input
                      value={step.key}
                      onChange={(e) => onStepChange({ key: e.target.value })}
                      className="agentdash-form-input"
                      placeholder="implement"
                    />
                  </div>
                  <div>
                    <label className="agentdash-form-label">描述</label>
                    <textarea
                      value={step.description}
                      onChange={(e) => onStepChange({ description: e.target.value })}
                      rows={2}
                      className="agentdash-form-textarea"
                      placeholder="当前节点的职责与边界"
                    />
                  </div>
                  {onCloneFromWorkflow && availableWorkflows.length > 0 && (
                    <CloneFromWorkflowButton
                      availableWorkflows={availableWorkflows}
                      onClone={onCloneFromWorkflow}
                    />
                  )}
                </section>
              )}

              <InjectionPanel
                injection={workflowDraft.contract.injection}
                compact={compact}
                onGuidanceChange={(guidance) => updateInjection({ guidance })}
                onBindingChange={handleBindingChange}
                onBindingAdd={handleBindingAdd}
                onBindingRemove={handleBindingRemove}
              />

              <CapabilityPanel
                projectId={projectId}
                targetKinds={targetKinds}
                directives={workflowDraft.contract.capability_config.tool_directives}
                compact={compact}
                onDirectivesChange={handleDirectivesChange}
              />

              <HookRulesPanel
                hookRules={workflowDraft.contract.hook_rules}
                presets={hookPresets}
                compact={compact}
                onAdd={handleAddHookRule}
                onToggle={handleToggleHookRule}
                onRemove={handleRemoveHookRule}
              />

              <PortsPanel
                outputPorts={step.output_ports}
                inputPorts={step.input_ports}
                compact={compact}
                onOutputChange={handleOutputPortsChange}
                onInputChange={handleInputPortsChange}
              />
            </>
          )}
        </div>
      </div>

      {/* Footer */}
      {!hideStepActions && onRemove && (
        <div className="border-t border-border px-4 py-3">
          <button
            type="button"
            onClick={onRemove}
            className="w-full rounded-[8px] border border-destructive/30 px-3 py-2 text-xs text-destructive transition-colors hover:bg-destructive/5"
          >
            删除此节点
          </button>
        </div>
      )}
    </div>
  );
}

// ─── Overview Tab ──────────────────────────────────────

function OverviewTab({
  step,
  nodeType,
  isEntry,
  availableWorkflows,
  onStepChange,
  onCloneFromWorkflow,
}: {
  step: LifecycleStepDefinition;
  nodeType: LifecycleNodeType;
  isEntry: boolean;
  availableWorkflows: WorkflowDefinition[];
  onStepChange: (patch: Partial<LifecycleStepDefinition>) => void;
  onCloneFromWorkflow?: (wf: WorkflowDefinition) => void;
}) {
  return (
    <section className="space-y-3">
      <div>
        <label className="agentdash-form-label">Node Key</label>
        <input
          value={step.key}
          onChange={(e) => onStepChange({ key: e.target.value })}
          className="agentdash-form-input"
          placeholder="implement"
        />
        <p className="mt-1 text-[10px] text-muted-foreground">
          lifecycle 内唯一标识，用作 edge 连接引用
        </p>
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={step.description}
          onChange={(e) => onStepChange({ description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea"
          placeholder="当前节点的职责与边界"
        />
      </div>

      <div>
        <label className="agentdash-form-label">节点类型</label>
        <select
          value={nodeType}
          onChange={(e) => onStepChange({ node_type: e.target.value as LifecycleNodeType })}
          className="agentdash-form-select"
        >
          <option value="agent_node">Agent Node</option>
          <option value="phase_node" disabled={isEntry}>
            Phase Node{isEntry ? "（入口不可用）" : ""}
          </option>
        </select>
        <p className="mt-1 text-[10px] text-muted-foreground">
          Agent / Phase 均可绑定 workflow contract；切换到 Detail tab 编辑。
        </p>
      </div>

      {/* Ports 只读摘要（编排者视角） */}
      <div className="space-y-1.5">
        <label className="agentdash-form-label">Output Ports ({step.output_ports.length})</label>
        {step.output_ports.length === 0 ? (
          <p className="rounded-[8px] border border-dashed border-border px-2 py-1.5 text-[11px] text-muted-foreground">
            暂无 output port
          </p>
        ) : (
          <div className="space-y-1">
            {step.output_ports.map((p, idx) => (
              <div
                key={idx}
                className="rounded-[8px] border border-border bg-secondary/20 px-2 py-1.5"
              >
                <p className="font-mono text-[11px] text-foreground">{p.key || "(未命名)"}</p>
                {p.description && (
                  <p className="mt-0.5 text-[10px] text-muted-foreground">{p.description}</p>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="space-y-1.5">
        <label className="agentdash-form-label">Input Ports ({step.input_ports.length})</label>
        {step.input_ports.length === 0 ? (
          <p className="rounded-[8px] border border-dashed border-border px-2 py-1.5 text-[11px] text-muted-foreground">
            暂无 input port
          </p>
        ) : (
          <div className="space-y-1">
            {step.input_ports.map((p, idx) => (
              <div
                key={idx}
                className="rounded-[8px] border border-border bg-secondary/20 px-2 py-1.5"
              >
                <p className="font-mono text-[11px] text-foreground">{p.key || "(未命名)"}</p>
                {p.description && (
                  <p className="mt-0.5 text-[10px] text-muted-foreground">{p.description}</p>
                )}
              </div>
            ))}
          </div>
        )}
        <p className="text-[10px] text-muted-foreground">
          Port 详细配置（gate / context strategy）请在 Detail → Ports 编辑
        </p>
      </div>

      {onCloneFromWorkflow && availableWorkflows.length > 0 && (
        <CloneFromWorkflowButton
          availableWorkflows={availableWorkflows}
          onClone={onCloneFromWorkflow}
        />
      )}
    </section>
  );
}

// ─── Clone from Workflow popover ───────────────────────

function CloneFromWorkflowButton({
  availableWorkflows,
  onClone,
}: {
  availableWorkflows: WorkflowDefinition[];
  onClone: (wf: WorkflowDefinition) => void;
}) {
  const [open, setOpen] = useState(false);
  const sorted = availableWorkflows
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
      >
        从已有 Workflow 克隆
      </button>
      {open && (
        <div
          className="absolute right-0 left-0 z-20 mt-1 max-h-64 overflow-y-auto rounded-[10px] border border-border bg-background p-1.5 shadow-lg"
        >
          {sorted.length === 0 ? (
            <p className="px-2 py-2 text-center text-xs text-muted-foreground">暂无可克隆的 Workflow</p>
          ) : (
            sorted.map((wf) => (
              <button
                key={wf.id}
                type="button"
                onClick={() => {
                  onClone(wf);
                  setOpen(false);
                }}
                className="block w-full rounded-[6px] px-2 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary"
              >
                <p className="font-medium">{wf.name}</p>
                <p className="truncate text-[10px] text-muted-foreground">{wf.key}</p>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
