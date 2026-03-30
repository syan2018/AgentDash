import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  HookRulePreset,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  DEFINITION_STATUS_LABEL,
  TARGET_KIND_LABEL,
} from "./shared-labels";
import { BindingEditor } from "./binding-editor";
import { ValidationPanel } from "./ui/validation-panel";
import { DetailSection } from "../../components/ui/detail-panel";

const TRIGGER_LABEL: Record<WorkflowHookTrigger, string> = {
  before_tool: "工具调用前",
  after_tool: "工具调用后",
  after_turn: "Turn 结束后",
  before_stop: "Session 结束前",
  session_terminal: "Session 终态",
  before_subagent_dispatch: "子 Agent 派发前",
  after_subagent_dispatch: "子 Agent 派发后",
  subagent_result: "子 Agent 结果回流",
};

const GATE_TRIGGERS: ReadonlySet<WorkflowHookTrigger> = new Set([
  "before_stop",
  "session_terminal",
]);

const PROCESS_TRIGGERS: ReadonlySet<WorkflowHookTrigger> = new Set([
  "before_tool",
  "after_tool",
  "after_turn",
  "before_subagent_dispatch",
  "after_subagent_dispatch",
  "subagent_result",
]);

const PROCESS_TRIGGER_OPTIONS: WorkflowHookTrigger[] = [
  "before_tool", "after_tool", "after_turn",
  "before_subagent_dispatch", "after_subagent_dispatch", "subagent_result",
];

const GATE_TRIGGER_OPTIONS: WorkflowHookTrigger[] = [
  "before_stop", "session_terminal",
];

const PROCESS_TRIGGER_ORDER: WorkflowHookTrigger[] = PROCESS_TRIGGER_OPTIONS;
const GATE_TRIGGER_ORDER: WorkflowHookTrigger[] = GATE_TRIGGER_OPTIONS;

// ─── Instruction list ──────────────────────────────────

function InstructionListEditor({
  values,
  onChange,
}: {
  values: string[];
  onChange: (next: string[]) => void;
}) {
  const [draft, setDraft] = useState("");

  const addItem = () => {
    const trimmed = draft.trim();
    if (!trimmed) return;
    onChange([...values, trimmed]);
    setDraft("");
  };

  return (
    <div>
      <label className="agentdash-form-label">注入指令 ({values.length})</label>
      <p className="mb-1.5 text-[11px] text-muted-foreground">
        Session 启动时注入给 Agent 的行为指令，按数组顺序拼接到 system prompt。
      </p>
      <div className="space-y-1.5">
        {values.map((value, index) => (
          <div key={`${value}-${index}`} className="flex items-start gap-2">
            <p className="flex-1 rounded-[8px] border border-border bg-secondary/20 px-2 py-1.5 text-xs text-foreground/80 leading-5">
              {value}
            </p>
            <button
              type="button"
              onClick={() => onChange(values.filter((_, i) => i !== index))}
              className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-xs text-destructive hover:bg-destructive/10"
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <div className="mt-2 flex gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addItem(); } }}
          className="agentdash-form-input flex-1 text-sm"
          placeholder="添加一条注入指令…"
        />
        <button type="button" onClick={addItem} className="agentdash-button-secondary shrink-0 text-sm">
          添加
        </button>
      </div>
    </div>
  );
}

// ─── Committed rule (read mode) ────────────────────────

function HookRuleItem({
  rule,
  onToggle,
  onRemove,
}: {
  rule: WorkflowHookRuleSpec;
  onToggle: () => void;
  onRemove: () => void;
}) {
  return (
    <div className={`flex items-center gap-3 rounded-[10px] border px-3 py-2.5 transition-colors ${rule.enabled ? "border-border bg-background" : "border-border/40 bg-secondary/30 opacity-60"}`}>
      <button
        type="button"
        onClick={onToggle}
        className={`shrink-0 size-4 rounded-[4px] border-2 transition-colors ${rule.enabled ? "border-primary bg-primary" : "border-muted-foreground/40 bg-transparent"}`}
        title={rule.enabled ? "点击禁用" : "点击启用"}
      >
        {rule.enabled && (
          <svg viewBox="0 0 12 12" className="size-full text-primary-foreground" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M2 6l3 3 5-5" />
          </svg>
        )}
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-foreground">{rule.description || rule.key}</span>
          {rule.preset && (
            <span className="rounded bg-secondary px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">{rule.preset}</span>
          )}
          {!rule.preset && rule.script && (
            <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-mono text-amber-600">rhai</span>
          )}
          <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary/70">{TRIGGER_LABEL[rule.trigger]}</span>
        </div>
        {rule.params && Object.keys(rule.params).length > 0 && (
          <p className="mt-0.5 text-[11px] text-muted-foreground font-mono truncate">
            params: {JSON.stringify(rule.params)}
          </p>
        )}
      </div>
      <button
        type="button"
        onClick={onRemove}
        className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-xs text-destructive hover:bg-destructive/10"
      >
        ×
      </button>
    </div>
  );
}

// ─── Inline new-rule editor (edit mode) ────────────────

function NewRuleEditor({
  triggerOptions,
  presets,
  existingKeys,
  onCommit,
  onCancel,
}: {
  triggerOptions: WorkflowHookTrigger[];
  presets: HookRulePreset[];
  existingKeys: Set<string>;
  onCommit: (rule: WorkflowHookRuleSpec) => void;
  onCancel: () => void;
}) {
  const [trigger, setTrigger] = useState<WorkflowHookTrigger>(triggerOptions[0]);
  const [mode, setMode] = useState<"preset" | "script">("preset");
  const [selectedPreset, setSelectedPreset] = useState<string>("");
  const [description, setDescription] = useState("");
  const [script, setScript] = useState("");
  const [showPresetScript, setShowPresetScript] = useState(false);

  const presetsForTrigger = useMemo(
    () => presets.filter((p) => p.trigger === trigger && !existingKeys.has(p.key)),
    [presets, trigger, existingKeys],
  );

  const activePreset = presetsForTrigger.find((p) => p.key === selectedPreset);

  const handleTriggerChange = (next: WorkflowHookTrigger) => {
    setTrigger(next);
    setSelectedPreset("");
  };

  const canCommit = mode === "preset"
    ? selectedPreset !== ""
    : description.trim() !== "" && script.trim() !== "";

  const handleCommit = () => {
    if (!canCommit) return;
    if (mode === "preset" && activePreset) {
      const defaultParams = activePreset.param_schema
        ? buildDefaultParams(activePreset.param_schema)
        : null;
      onCommit({
        key: activePreset.key,
        trigger,
        description: activePreset.label,
        preset: activePreset.key,
        params: defaultParams,
        script: null,
        enabled: true,
      });
    } else if (mode === "script") {
      onCommit({
        key: `custom_${Date.now()}`,
        trigger,
        description: description.trim(),
        preset: null,
        params: null,
        script: script.trim(),
        enabled: true,
      });
    }
  };

  return (
    <div className="rounded-[10px] border-2 border-dashed border-primary/30 bg-primary/5 p-3 space-y-2.5">
      <div className="grid gap-2 sm:grid-cols-3">
        {/* Trigger 选择 */}
        <div>
          <label className="text-[11px] font-medium text-muted-foreground">触发时机</label>
          <select
            value={trigger}
            onChange={(e) => handleTriggerChange(e.target.value as WorkflowHookTrigger)}
            className="agentdash-form-select mt-0.5 text-xs"
          >
            {triggerOptions.map((t) => (
              <option key={t} value={t}>{TRIGGER_LABEL[t]}</option>
            ))}
          </select>
        </div>

        {/* 行为类型 */}
        <div>
          <label className="text-[11px] font-medium text-muted-foreground">行为类型</label>
          <select
            value={mode}
            onChange={(e) => { setMode(e.target.value as "preset" | "script"); setSelectedPreset(""); }}
            className="agentdash-form-select mt-0.5 text-xs"
          >
            <option value="preset">预设逻辑</option>
            <option value="script">自定义脚本 (Rhai)</option>
          </select>
        </div>

        {/* Preset 或 script 内容 */}
        {mode === "preset" ? (
          <div>
            <label className="text-[11px] font-medium text-muted-foreground">选择预设</label>
            <select
              value={selectedPreset}
              onChange={(e) => setSelectedPreset(e.target.value)}
              className="agentdash-form-select mt-0.5 text-xs"
            >
              <option value="">-- 选择 --</option>
              {presetsForTrigger.map((p) => (
                <option key={p.key} value={p.key}>{p.label}</option>
              ))}
            </select>
          </div>
        ) : (
          <div>
            <label className="text-[11px] font-medium text-muted-foreground">描述</label>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="agentdash-form-input mt-0.5 text-xs"
              placeholder="这条 hook 做什么"
            />
          </div>
        )}
      </div>

      {/* Preset description + script preview */}
      {mode === "preset" && activePreset && (
        <div className="space-y-1.5">
          <p className="text-[11px] text-muted-foreground leading-4">
            {activePreset.description}
          </p>
          <div className="flex items-center gap-2">
            {activePreset.script && (
              <button
                type="button"
                onClick={() => setShowPresetScript(!showPresetScript)}
                className="text-[11px] text-primary/70 hover:text-primary underline"
              >
                {showPresetScript ? "隐藏脚本" : "查看脚本"}
              </button>
            )}
            {activePreset.script && (
              <button
                type="button"
                onClick={() => {
                  setMode("script");
                  setScript(activePreset.script || "");
                  setDescription(activePreset.label);
                  setSelectedPreset("");
                  setShowPresetScript(false);
                }}
                className="text-[11px] text-primary/70 hover:text-primary underline"
              >
                Clone 为自定义
              </button>
            )}
          </div>
          {showPresetScript && activePreset.script && (
            <pre className="max-h-48 overflow-auto rounded-md border bg-secondary/30 p-2 text-[11px] font-mono text-foreground/80 leading-[1.6]">
              {activePreset.script}
            </pre>
          )}
        </div>
      )}

      {/* Rhai script editor */}
      {mode === "script" && (
        <div>
          <label className="text-[11px] font-medium text-muted-foreground">Rhai 脚本</label>
          <textarea
            value={script}
            onChange={(e) => setScript(e.target.value)}
            rows={8}
            className="agentdash-form-textarea mt-0.5 font-mono text-xs leading-[1.6]"
            placeholder={`// 返回一个 map 表达决策效果\n// 空 map #{} 表示无操作\n\nif ctx.tool_name == "shell_exec" {\n    #{ block: "禁止执行 shell" }\n} else {\n    #{}\n}`}
            spellCheck={false}
          />
          <p className="mt-1 text-[10px] text-muted-foreground">
            使用 Rhai 语法。可用 <code className="bg-secondary/50 px-1 rounded">ctx</code> 访问触发上下文，
            <code className="bg-secondary/50 px-1 rounded">make_injection()</code>、
            <code className="bg-secondary/50 px-1 rounded">make_diagnostic()</code> 等辅助函数。
          </p>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-end gap-2">
        <button type="button" onClick={onCancel} className="agentdash-button-secondary text-xs px-3 py-1">
          取消
        </button>
        <button
          type="button"
          onClick={handleCommit}
          disabled={!canCommit}
          className="agentdash-button-primary text-xs px-3 py-1 disabled:opacity-40"
        >
          确认添加
        </button>
      </div>
    </div>
  );
}

// ─── Rule group (list + add) ───────────────────────────

function HookRuleGroup({
  rules,
  triggerOrder,
  triggerOptions,
  presets,
  existingKeys,
  onAdd,
  onToggle,
  onRemove,
}: {
  rules: WorkflowHookRuleSpec[];
  triggerOrder: WorkflowHookTrigger[];
  triggerOptions: WorkflowHookTrigger[];
  presets: HookRulePreset[];
  existingKeys: Set<string>;
  onAdd: (rule: WorkflowHookRuleSpec) => void;
  onToggle: (key: string) => void;
  onRemove: (key: string) => void;
}) {
  const [adding, setAdding] = useState(false);

  const grouped = useMemo(() => {
    const map = new Map<WorkflowHookTrigger, WorkflowHookRuleSpec[]>();
    for (const rule of rules) {
      const list = map.get(rule.trigger) ?? [];
      list.push(rule);
      map.set(rule.trigger, list);
    }
    return map;
  }, [rules]);

  const activeTriggers = triggerOrder.filter((t) => grouped.has(t));

  return (
    <div className="space-y-2.5">
      {rules.length === 0 && !adding && (
        <p className="py-3 text-center text-sm text-muted-foreground">尚未配置</p>
      )}

      {activeTriggers.map((trigger) => (
        <div key={trigger}>
          <h4 className="mb-1.5 flex items-center gap-2 text-xs font-medium text-foreground/70">
            <span className="inline-block size-1.5 rounded-full bg-primary/50" />
            {TRIGGER_LABEL[trigger]}
          </h4>
          <div className="space-y-1.5">
            {(grouped.get(trigger) ?? []).map((rule) => (
              <HookRuleItem
                key={rule.key}
                rule={rule}
                onToggle={() => onToggle(rule.key)}
                onRemove={() => onRemove(rule.key)}
              />
            ))}
          </div>
        </div>
      ))}

      {adding ? (
        <NewRuleEditor
          triggerOptions={triggerOptions}
          presets={presets}
          existingKeys={existingKeys}
          onCommit={(rule) => { onAdd(rule); setAdding(false); }}
          onCancel={() => setAdding(false)}
        />
      ) : (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="w-full rounded-[10px] border-2 border-dashed border-border/60 py-2.5 text-sm text-muted-foreground hover:border-primary/40 hover:text-primary/70 transition-colors"
        >
          + 添加 Hook 行为
        </button>
      )}
    </div>
  );
}

// ─── Helpers ───────────────────────────────────────────

function buildDefaultParams(schema: Record<string, unknown>): Record<string, unknown> | null {
  const props = schema.properties as Record<string, Record<string, unknown>> | undefined;
  if (!props) return null;
  const result: Record<string, unknown> = {};
  for (const [key, prop] of Object.entries(props)) {
    if (prop.type === "array") result[key] = [];
    else if (prop.type === "string") result[key] = "";
    else if (prop.type === "number") result[key] = 0;
    else if (prop.type === "boolean") result[key] = false;
  }
  return Object.keys(result).length > 0 ? result : null;
}

// ─── Main editor ───────────────────────────────────────

export function WorkflowEditor() {
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
    await saveDraft();
  }, [validateDraft, saveDraft]);

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
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault(); };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;

  const updateInjection = (patch: Partial<WorkflowInjectionSpec>) => {
    updateDraft({ contract: { ...draft.contract, injection: { ...draft.contract.injection, ...patch } } });
  };

  const handleToggleRule = (key: string) => {
    const rule = draft.contract.hook_rules.find((r) => r.key === key);
    if (rule) updateDraftHookRule(key, { enabled: !rule.enabled });
  };

  const existingKeys = useMemo(
    () => new Set(draft.contract.hook_rules.map((r) => r.key)),
    [draft.contract.hook_rules],
  );

  const processRules = useMemo(
    () => draft.contract.hook_rules.filter((r) => PROCESS_TRIGGERS.has(r.trigger)),
    [draft.contract.hook_rules],
  );
  const gateRules = useMemo(
    () => draft.contract.hook_rules.filter((r) => GATE_TRIGGERS.has(r.trigger)),
    [draft.contract.hook_rules],
  );

  return (
    <div className="space-y-4 p-5">
      {/* 操作栏 */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {isDirty && <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">未保存</span>}
          {currentDefinition && (
            <>
              <span className={`rounded-full border px-2 py-0.5 text-[10px] ${currentDefinition.status === "active" ? "border-emerald-300/40 bg-emerald-500/10 text-emerald-700" : "border-border bg-secondary/40 text-muted-foreground"}`}>
                {DEFINITION_STATUS_LABEL[currentDefinition.status]}
              </span>
              <span className="text-[10px] text-muted-foreground">v{currentDefinition.version}</span>
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button type="button" onClick={() => void validateDraft()} disabled={isValidating} className="agentdash-button-secondary text-sm">
            {isValidating ? "校验中…" : "校验"}
          </button>
          <button type="button" onClick={() => void handleSave()} disabled={isSaving || hasErrors} className="agentdash-button-primary text-sm">
            {isSaving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>

      {validation && <ValidationPanel issues={validation.issues} />}
      {error && <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2"><p className="text-xs text-destructive">{error}</p></div>}

      {/* 基本信息 */}
      <DetailSection title="基本信息">
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">Key</label>
            <input value={draft.key} onChange={(e) => updateDraft({ key: e.target.value })} disabled={!isNew} className="agentdash-form-input disabled:opacity-60" placeholder="unique_workflow_key" />
          </div>
          <div>
            <label className="agentdash-form-label">名称</label>
            <input value={draft.name} onChange={(e) => updateDraft({ name: e.target.value })} className="agentdash-form-input" placeholder="Workflow 显示名" />
          </div>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">描述</label>
            <textarea value={draft.description} onChange={(e) => updateDraft({ description: e.target.value })} rows={2} className="agentdash-form-textarea" placeholder="这个 Workflow 做什么" />
          </div>
          <div>
            <label className="agentdash-form-label">挂载类型</label>
            <select value={draft.target_kind} onChange={(e) => updateDraft({ target_kind: e.target.value as WorkflowTargetKind })} disabled={!isNew} className="agentdash-form-select disabled:opacity-60">
              {Object.entries(TARGET_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
            <p className="mt-1 text-[11px] text-muted-foreground">决定此 Workflow 挂载到哪类实体（Project/Story/Task）。</p>
          </div>
        </div>
      </DetailSection>

      {/* Session 注入 */}
      <DetailSection title="Session 注入" description="Session 启动或 Workflow 切换时，hook 向 Agent 上下文注入的内容。">
        <InstructionListEditor
          values={draft.contract.injection.instructions}
          onChange={(instructions) => updateInjection({ instructions })}
        />
      </DetailSection>

      {/* Context Bindings */}
      <DetailSection
        title={`上下文挂载 (${draft.contract.injection.context_bindings.length})`}
        description="Session 启动时自动挂载的外部上下文资源。"
        extra={<button type="button" onClick={addDraftBinding} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.injection.context_bindings.map((binding, idx) => (
            <BindingEditor
              key={`${binding.locator}:${idx}`}
              binding={binding}
              index={idx}
              onChange={(patch) => updateDraftBinding(idx, patch)}
              onRemove={() => removeDraftBinding(idx)}
            />
          ))}
          {draft.contract.injection.context_bindings.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">暂无上下文挂载</p>
          )}
        </div>
      </DetailSection>

      {/* 过程行为 */}
      <DetailSection
        title={`过程行为 (${processRules.length})`}
        description="工具调用、Turn 结束、子 Agent 交互等过程中触发的 hook 行为。"
      >
        <HookRuleGroup
          rules={processRules}
          triggerOrder={PROCESS_TRIGGER_ORDER}
          triggerOptions={PROCESS_TRIGGER_OPTIONS}
          presets={hookPresets}
          existingKeys={existingKeys}
          onAdd={addDraftHookRule}
          onToggle={handleToggleRule}
          onRemove={removeDraftHookRule}
        />
      </DetailSection>

      {/* 结束门禁 */}
      <DetailSection
        title={`结束门禁 (${gateRules.length})`}
        description="Session 结束前和终态判定时触发的 hook，控制完成条件和 step 推进。"
      >
        <HookRuleGroup
          rules={gateRules}
          triggerOrder={GATE_TRIGGER_ORDER}
          triggerOptions={GATE_TRIGGER_OPTIONS}
          presets={hookPresets}
          existingKeys={existingKeys}
          onAdd={addDraftHookRule}
          onToggle={handleToggleRule}
          onRemove={removeDraftHookRule}
        />
      </DetailSection>
    </div>
  );
}
