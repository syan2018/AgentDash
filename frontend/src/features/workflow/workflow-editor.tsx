/**
 * WorkflowEditor —— Workflow contract 表单容器。
 *
 * 本组件不再承载具体表单渲染逻辑；5 个受控 panel（BasicInfo / Injection /
 * HookRules / Capability / Ports）住在 `ui/panels/` 下。容器职责：
 *   - 从 workflowStore 读取 draft / validation / hookPresets
 *   - 把 panel 的 onChange 串回 store action（updateDraft / updateDraftBinding …）
 *   - 承载操作栏（校验 / 保存）、快捷键与 beforeunload 副作用
 *
 * 行为与拆分前完全等价；所有旧入口（Lifecycle DAG editor 的 DetailPanel 抽屉、
 * Assets 面板、workflow-tab-view 等）透明沿用。
 */

import { useCallback, useEffect, useMemo } from "react";

import type { WorkflowDefinition, WorkflowInjectionSpec } from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { ValidationPanel } from "./ui/validation-panel";
import {
  BasicInfoPanel,
  CapabilityPanel,
  HookRulesPanel,
  InjectionPanel,
  PortsPanel,
} from "./ui/panels";

export interface WorkflowEditorProps {
  onSaved?: (definition: WorkflowDefinition) => void;
}

export function WorkflowEditor({ onSaved }: WorkflowEditorProps = {}) {
  const draft = useWorkflowStore((s) => s.wfEditor.draft);
  const originalId = useWorkflowStore((s) => s.wfEditor.originalId);
  const validation = useWorkflowStore((s) => s.wfEditor.validation);
  const isSaving = useWorkflowStore((s) => s.wfEditor.isSaving);
  const isValidating = useWorkflowStore((s) => s.wfEditor.isValidating);
  const isDirty = useWorkflowStore((s) => s.wfEditor.dirty);
  const error = useWorkflowStore((s) => s.wfEditor.error);

  const hookPresets = useWorkflowStore((s) => s.hookPresets);
  const fetchHookPresets = useWorkflowStore((s) => s.fetchHookPresets);
  const updateDraft = useWorkflowStore((s) => s.updateDraft);
  const updateDraftBinding = useWorkflowStore((s) => s.updateDraftBinding);
  const addDraftBinding = useWorkflowStore((s) => s.addDraftBinding);
  const removeDraftBinding = useWorkflowStore((s) => s.removeDraftBinding);
  const addDraftHookRule = useWorkflowStore((s) => s.addDraftHookRule);
  const removeDraftHookRule = useWorkflowStore((s) => s.removeDraftHookRule);
  const updateDraftHookRule = useWorkflowStore((s) => s.updateDraftHookRule);
  const validateDraft = useWorkflowStore((s) => s.validateDraft);
  const saveDraft = useWorkflowStore((s) => s.saveDraft);

  const definitions = useWorkflowStore((s) => s.definitions);
  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return definitions.find((d) => d.id === originalId) ?? null;
  }, [originalId, definitions]);

  useEffect(() => {
    if (hookPresets.length === 0) void fetchHookPresets();
  }, [hookPresets.length, fetchHookPresets]);

  const handleSave = useCallback(async () => {
    const result = await validateDraft();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    const saved = await saveDraft();
    if (saved) onSaved?.(saved);
  }, [onSaved, validateDraft, saveDraft]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (!isSaving) void handleSave();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleSave, isSaving]);

  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;

  // ─── 面向各 panel 的 onChange adapter ────────────────

  const updateInjection = (patch: Partial<WorkflowInjectionSpec>) => {
    updateDraft({
      contract: {
        ...draft.contract,
        injection: { ...draft.contract.injection, ...patch },
      },
    });
  };

  const handleToggleRule = (key: string) => {
    const rule = draft.contract.hook_rules.find((r) => r.key === key);
    if (rule) updateDraftHookRule(key, { enabled: !rule.enabled });
  };

  return (
    <div className="space-y-4 p-5">
      {/* 操作栏 */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {isDirty && (
            <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">
              未保存
            </span>
          )}
          {currentDefinition && (
            <span className="text-[10px] text-muted-foreground">v{currentDefinition.version}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void validateDraft()}
            disabled={isValidating}
            className="agentdash-button-secondary text-sm"
          >
            {isValidating ? "校验中…" : "校验"}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={isSaving || hasErrors}
            className="agentdash-button-primary text-sm"
          >
            {isSaving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>

      {validation && <ValidationPanel issues={validation.issues} />}
      {error && (
        <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2">
          <p className="text-xs text-destructive">{error}</p>
        </div>
      )}

      {/* 基本信息 */}
      <BasicInfoPanel
        draftKey={draft.key}
        name={draft.name}
        description={draft.description}
        targetKinds={draft.target_kinds}
        keyDisabled={!isNew}
        onKeyChange={(key) => updateDraft({ key })}
        onNameChange={(name) => updateDraft({ name })}
        onDescriptionChange={(description) => updateDraft({ description })}
        onTargetKindsChange={(target_kinds) => updateDraft({ target_kinds })}
      />

      {/* Session 注入（guidance + bindings） */}
      <InjectionPanel
        injection={draft.contract.injection}
        onGuidanceChange={(guidance) => updateInjection({ guidance })}
        onBindingChange={(idx, patch) => updateDraftBinding(idx, patch)}
        onBindingAdd={addDraftBinding}
        onBindingRemove={removeDraftBinding}
      />

      {/* Agent 工具能力 */}
      <CapabilityPanel
        projectId={draft.project_id}
        targetKinds={draft.target_kinds}
        directives={draft.contract.capability_config.tool_directives}
        onDirectivesChange={(tool_directives) =>
          updateDraft({
            contract: {
              ...draft.contract,
              capability_config: {
                ...draft.contract.capability_config,
                tool_directives,
              },
            },
          })
        }
      />

      {/* Hook 规则（过程行为 + 结束门禁） */}
      <HookRulesPanel
        hookRules={draft.contract.hook_rules}
        presets={hookPresets}
        onAdd={addDraftHookRule}
        onToggle={handleToggleRule}
        onRemove={removeDraftHookRule}
      />

      {/* I/O Ports */}
      <PortsPanel
        outputPorts={draft.contract.output_ports ?? []}
        inputPorts={draft.contract.input_ports ?? []}
        onOutputChange={(output_ports) =>
          updateDraft({ contract: { ...draft.contract, output_ports } })
        }
        onInputChange={(input_ports) =>
          updateDraft({ contract: { ...draft.contract, input_ports } })
        }
      />
    </div>
  );
}
