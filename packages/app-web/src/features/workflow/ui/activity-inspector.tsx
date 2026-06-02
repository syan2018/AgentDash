/**
 * ActivityInspector —— 选中 lifecycle activity 节点时右侧栏的编辑面板。
 *
 * 受控组件：接收 activity + 对应 workflowDraft + 一组细粒度 callback；不直接调
 * workflowStore（shell 在装配时把 store actions 包装传入）。
 *
 * 结构（双 tab 信息架构）：
 *   - sticky header: key 标题 + 入口切换 + 关闭
 *   - 顶部 tab 切换：Activity（外层编排） / Contract（仅 Agent 可见）
 *   - reset toast 槽：Executor 切换触发 completion_policy 重置时显示
 *   - **Activity tab**：
 *       §1 Identity（key / description；iteration / join 折在「高级」）
 *       §2 Executor（kind + 主字段；可选项折在「高级」）
 *       §3 Ports & Policy（input/output ports + completion_policy）
 *   - **Contract tab**（Agent only）：Injection / Capability / HookRules / Contract Ports
 *   - footer: 删除按钮
 */

import { useCallback, useState } from "react";

import type {
  ActivityCompletionPolicy,
  ActivityDefinition,
  ActivityExecutorSpec,
  ActivityJoinPolicy,
  CapabilityDirective,
  HookRulePreset,
  InputPortDefinition,
  OutputPortDefinition,
  WorkflowContextBinding,
  AgentProcedure,
  WorkflowHookRuleSpec,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../../types";
import type { WorkflowEditorDraft } from "../../../stores/workflowStore";
import {
  ExecutorSection,
  Header,
  IdentitySection,
  PortsAndPolicySection,
  TabButton,
  AgentProcedureContractTabContent,
} from "./activity-inspector-sections";

// ─── Props ──────────────────────────────────────────────

export interface ActivityInspectorProps {
  activity: ActivityDefinition;
  workflowDraft: WorkflowEditorDraft;
  isEntry: boolean;
  availableWorkflows: AgentProcedure[];
  hookPresets: HookRulePreset[];
  targetKinds: WorkflowTargetKind[];
  projectId: string;

  // 细粒度回调（与 workflowStore actions 对应）
  onActivityChange: (patch: Partial<ActivityDefinition>) => void;
  onWorkflowChange: (patch: Partial<WorkflowEditorDraft>) => void;
  onSetExecutor: (
    executor: ActivityExecutorSpec,
  ) => { reset: boolean; previous: ActivityCompletionPolicy } | null;
  onSetCompletionPolicy: (policy: ActivityCompletionPolicy) => void;
  onSetIterationPolicy: (patch: Partial<ActivityDefinition["iteration_policy"]>) => void;
  onSetJoinPolicy: (policy: ActivityJoinPolicy) => void;
  onSetEntry: () => void;
  onRemove: () => void;
  onClose: () => void;
}

// ─── 主组件 ─────────────────────────────────────────────

type InspectorTab = "activity" | "contract";

export function ActivityInspector(props: ActivityInspectorProps) {
  const {
    activity,
    workflowDraft,
    isEntry,
    availableWorkflows,
    hookPresets,
    targetKinds,
    projectId,
    onActivityChange,
    onWorkflowChange,
    onSetExecutor,
    onSetCompletionPolicy,
    onSetIterationPolicy,
    onSetJoinPolicy,
    onSetEntry,
    onRemove,
    onClose,
  } = props;

  const isAgent = activity.executor.kind === "agent";
  const [tab, setTab] = useState<InspectorTab>("activity");
  // 非 Agent 时强制只显示 activity tab（contract tab 不存在）
  const activeTab: InspectorTab = isAgent ? tab : "activity";
  const [resetNotice, setResetNotice] = useState<string | null>(null);

  // ─── §4 Workflow Contract 同步 helpers（沿用 step-inspector 双层 ports 同步） ───
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
      { locator: "", reason: "", required: true, title: undefined },
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
      if (workflowDraft.contract.hook_rules.some((r) => r.key === rule.key)) return;
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
          capability_config: { ...workflowDraft.contract.capability_config, tool_directives: next },
        },
      });
    },
    [workflowDraft, onWorkflowChange],
  );

  // Port 双层同步：contract → activity 自动合并（保留 step-extra）
  const propsActivityOutputPorts = activity.output_ports;
  const propsActivityInputPorts = activity.input_ports;

  const handleContractOutputPortsChange = useCallback(
    (nextContractPorts: OutputPortDefinition[]) => {
      const oldContractPorts = workflowDraft.contract.output_ports ?? [];
      onWorkflowChange({
        contract: { ...workflowDraft.contract, output_ports: nextContractPorts },
      });
      const merged = mergeContractIntoStep(oldContractPorts, nextContractPorts, propsActivityOutputPorts);
      onActivityChange({ output_ports: merged });
    },
    [workflowDraft, onWorkflowChange, propsActivityOutputPorts, onActivityChange],
  );

  const handleContractInputPortsChange = useCallback(
    (nextContractPorts: InputPortDefinition[]) => {
      const oldContractPorts = workflowDraft.contract.input_ports ?? [];
      onWorkflowChange({
        contract: { ...workflowDraft.contract, input_ports: nextContractPorts },
      });
      const merged = mergeContractIntoStep(oldContractPorts, nextContractPorts, propsActivityInputPorts);
      onActivityChange({ input_ports: merged });
    },
    [workflowDraft, onWorkflowChange, propsActivityInputPorts, onActivityChange],
  );

  // ─── Executor 切换：调 store action 拿 reset 反馈，触发 toast ───
  const handleExecutorChange = useCallback(
    (next: ActivityExecutorSpec) => {
      const result = onSetExecutor(next);
      if (result?.reset) {
        setResetNotice(
          `executor 切到 ${next.kind} 后 completion_policy 已自动调整为兼容值（原: ${result.previous.kind}）`,
        );
      } else {
        setResetNotice(null);
      }
    },
    [onSetExecutor],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Header activity={activity} isEntry={isEntry} onSetEntry={onSetEntry} onClose={onClose} />

      {/* Tab bar：仅 Agent activity 时显示 Contract tab */}
      {isAgent && (
        <div className="flex shrink-0 gap-1 border-b border-border bg-secondary/35 p-1">
          <TabButton active={activeTab === "activity"} onClick={() => setTab("activity")}>
            Activity
          </TabButton>
          <TabButton active={activeTab === "contract"} onClick={() => setTab("contract")}>
            Contract
          </TabButton>
        </div>
      )}

      {resetNotice && (
        <div className="shrink-0 border-b border-warning/30 bg-warning/10 px-4 py-2">
          <div className="flex items-start justify-between gap-2">
            <p className="text-[11px] text-warning">{resetNotice}</p>
            <button
              type="button"
              onClick={() => setResetNotice(null)}
              className="text-[11px] text-muted-foreground hover:text-foreground"
            >
              知道了
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        {activeTab === "activity" ? (
          <div className="space-y-5 p-4">
            <IdentitySection
              activity={activity}
              onActivityChange={onActivityChange}
              onSetIterationPolicy={onSetIterationPolicy}
              onSetJoinPolicy={onSetJoinPolicy}
            />

            <ExecutorSection
              activity={activity}
              workflowDraft={workflowDraft}
              availableWorkflows={availableWorkflows}
              isEntry={isEntry}
              onExecutorChange={handleExecutorChange}
            />

            <PortsAndPolicySection
              activity={activity}
              contractOutputKeys={
                new Set((workflowDraft.contract.output_ports ?? []).map((p) => p.key))
              }
              contractInputKeys={
                new Set((workflowDraft.contract.input_ports ?? []).map((p) => p.key))
              }
              onActivityChange={onActivityChange}
              onSetCompletionPolicy={onSetCompletionPolicy}
            />
          </div>
        ) : (
          <div className="p-4">
            <AgentProcedureContractTabContent
              workflowDraft={workflowDraft}
              hookPresets={hookPresets}
              targetKinds={targetKinds}
              projectId={projectId}
              onUpdateInjection={updateInjection}
              onBindingChange={handleBindingChange}
              onBindingAdd={handleBindingAdd}
              onBindingRemove={handleBindingRemove}
              onAddHookRule={handleAddHookRule}
              onToggleHookRule={handleToggleHookRule}
              onRemoveHookRule={handleRemoveHookRule}
              onDirectivesChange={handleDirectivesChange}
              onContractOutputPortsChange={handleContractOutputPortsChange}
              onContractInputPortsChange={handleContractInputPortsChange}
            />
          </div>
        )}
      </div>

      <footer className="shrink-0 border-t border-border px-4 py-3">
        <button
          type="button"
          onClick={onRemove}
          className="w-full rounded-[8px] border border-destructive/30 px-3 py-2 text-xs text-destructive transition-colors hover:bg-destructive/5"
        >
          删除此 Activity
        </button>
      </footer>
    </div>
  );
}

function mergeContractIntoStep<P extends { key: string }>(
  oldContract: P[],
  newContract: P[],
  currentActivity: P[],
): P[] {
  const oldContractKeys = new Set(oldContract.map((p) => p.key));
  const stepExtras = currentActivity.filter((p) => !oldContractKeys.has(p.key));
  return [...newContract, ...stepExtras];
}
