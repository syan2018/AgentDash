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

import { useCallback, useMemo, useState } from "react";

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
  InputPortItem,
  OutputPortItem,
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

  // props 快捷别名，供 handler 闭包读取最新 step ports
  const propsStepOutputPorts = step.output_ports;
  const propsStepInputPorts = step.input_ports;

  // ─── Port 双层语义 ───
  //
  // 后端 catalog.rs:312 明确：edge 引用的 port 必须存在于 step 级 ports 中。
  //   - workflow.contract.output_ports / input_ports = workflow 行为标准声明
  //   - step.output_ports / input_ports = DAG edge 真相源 / step 级拓展
  //
  // 用户决策（2026-05-11）：Overview 只改 step（DAG 视角），
  //   Detail PortsPanel 改 workflow contract 且自动合并到 step：
  //     - contract 新增 port → 追加到 step
  //     - contract 删除 port → 从 step 移除（仅删除原本来自 contract 的）
  //     - contract 修改 port → step 中同 key 的 port 跟着更新
  //     - step 独立加的（Overview 加的、不在 oldContract key 集里）保留不动
  //   反向不同步（Overview 改 step 不回流到 contract）。
  function mergeContractIntoStep<P extends { key: string }>(
    oldContract: P[],
    newContract: P[],
    currentStep: P[],
  ): P[] {
    const oldContractKeys = new Set(oldContract.map((p) => p.key));
    const stepExtras = currentStep.filter((p) => !oldContractKeys.has(p.key));
    return [...newContract, ...stepExtras];
  }

  const handleOutputPortsChange = useCallback(
    (nextContractPorts: OutputPortDefinition[]) => {
      const oldContractPorts = workflowDraft.contract.output_ports ?? [];
      onWorkflowChange({
        contract: { ...workflowDraft.contract, output_ports: nextContractPorts },
      });
      const mergedStep = mergeContractIntoStep(
        oldContractPorts,
        nextContractPorts,
        propsStepOutputPorts,
      );
      onStepChange({ output_ports: mergedStep });
    },
    [onStepChange, onWorkflowChange, workflowDraft, propsStepOutputPorts],
  );

  const handleInputPortsChange = useCallback(
    (nextContractPorts: InputPortDefinition[]) => {
      const oldContractPorts = workflowDraft.contract.input_ports ?? [];
      onWorkflowChange({
        contract: { ...workflowDraft.contract, input_ports: nextContractPorts },
      });
      const mergedStep = mergeContractIntoStep(
        oldContractPorts,
        nextContractPorts,
        propsStepInputPorts,
      );
      onStepChange({ input_ports: mergedStep });
    },
    [onStepChange, onWorkflowChange, workflowDraft, propsStepInputPorts],
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
            <p className="text-[10px] text-success">入口节点</p>
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
              workflowDraft={workflowDraft}
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

              {/* Detail PortsPanel 编辑 workflow 行为标准 ports（contract 级）；
                  保存时 handler 会同步合并到 step.ports（DAG 真相源）。 */}
              <PortsPanel
                outputPorts={workflowDraft.contract.output_ports ?? []}
                inputPorts={workflowDraft.contract.input_ports ?? []}
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
  workflowDraft,
  availableWorkflows,
  onStepChange,
  onCloneFromWorkflow,
}: {
  step: LifecycleStepDefinition;
  nodeType: LifecycleNodeType;
  isEntry: boolean;
  workflowDraft: WorkflowEditorDraft;
  availableWorkflows: WorkflowDefinition[];
  onStepChange: (patch: Partial<LifecycleStepDefinition>) => void;
  onCloneFromWorkflow?: (wf: WorkflowDefinition) => void;
}) {
  // contract 源 port 的 key 集合 —— Overview 上标记为"标准"只读，不可编辑不可删；
  // step 里不在 contract 集里的 = step-extra，Overview 可全量编辑 + 删除。
  const outputContractKeys = useMemo(
    () => new Set((workflowDraft.contract.output_ports ?? []).map((p) => p.key)),
    [workflowDraft.contract.output_ports],
  );
  const inputContractKeys = useMemo(
    () => new Set((workflowDraft.contract.input_ports ?? []).map((p) => p.key)),
    [workflowDraft.contract.input_ports],
  );

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
      </div>

      <OverviewOutputPortsSection
        ports={step.output_ports}
        contractKeys={outputContractKeys}
        onChange={(next) => onStepChange({ output_ports: next })}
      />

      <OverviewInputPortsSection
        ports={step.input_ports}
        contractKeys={inputContractKeys}
        onChange={(next) => onStepChange({ input_ports: next })}
      />

      {onCloneFromWorkflow && availableWorkflows.length > 0 && (
        <CloneFromWorkflowButton
          availableWorkflows={availableWorkflows}
          onClone={onCloneFromWorkflow}
        />
      )}
    </section>
  );
}

// ─── Overview 端口区（按来源区分 contract / step-extra）─────
//
// 默认态：contract 端口走 readOnly OutputPortItem（只读卡片 + "标准" badge，
// 不可编辑不可删）；step-extra 走带 view/edit 切换的 OutputPortItem + 删除按钮。
// 新增：追加一个空 key 的 step-extra，`OutputPortItem` 初始化时 key 为空会自动
// 进入 edit 态。

function OverviewOutputPortsSection({
  ports,
  contractKeys,
  onChange,
}: {
  ports: OutputPortDefinition[];
  contractKeys: Set<string>;
  onChange: (next: OutputPortDefinition[]) => void;
}) {
  const handleAdd = () =>
    onChange([
      ...ports,
      { key: "", description: "", gate_strategy: "existence" },
    ]);

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className="agentdash-form-label m-0">
          Output Ports ({ports.length})
        </label>
        <button
          type="button"
          onClick={handleAdd}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          + 添加
        </button>
      </div>
      <div className="space-y-1.5">
        {ports.length === 0 && (
          <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
        )}
        {ports.map((p, idx) => {
          const isContract = p.key !== "" && contractKeys.has(p.key);
          return (
            <OutputPortItem
              key={idx}
              port={p}
              readOnly={isContract}
              badge={isContract ? "标准" : undefined}
              onChange={
                isContract
                  ? undefined
                  : (next) => {
                      const n = [...ports];
                      n[idx] = next;
                      onChange(n);
                    }
              }
              onRemove={
                isContract ? undefined : () => onChange(ports.filter((_, i) => i !== idx))
              }
            />
          );
        })}
      </div>
    </div>
  );
}

function OverviewInputPortsSection({
  ports,
  contractKeys,
  onChange,
}: {
  ports: InputPortDefinition[];
  contractKeys: Set<string>;
  onChange: (next: InputPortDefinition[]) => void;
}) {
  const handleAdd = () =>
    onChange([
      ...ports,
      { key: "", description: "", context_strategy: "full" },
    ]);

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className="agentdash-form-label m-0">
          Input Ports ({ports.length})
        </label>
        <button
          type="button"
          onClick={handleAdd}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          + 添加
        </button>
      </div>
      <div className="space-y-1.5">
        {ports.length === 0 && (
          <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
        )}
        {ports.map((p, idx) => {
          const isContract = p.key !== "" && contractKeys.has(p.key);
          return (
            <InputPortItem
              key={idx}
              port={p}
              readOnly={isContract}
              badge={isContract ? "标准" : undefined}
              onChange={
                isContract
                  ? undefined
                  : (next) => {
                      const n = [...ports];
                      n[idx] = next;
                      onChange(n);
                    }
              }
              onRemove={
                isContract ? undefined : () => onChange(ports.filter((_, i) => i !== idx))
              }
            />
          );
        })}
      </div>
    </div>
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
          className="absolute right-0 left-0 z-20 mt-1 max-h-64 overflow-y-auto rounded-[8px] border border-border bg-background p-1.5 shadow-lg"
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
