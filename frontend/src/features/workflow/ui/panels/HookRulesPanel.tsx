/**
 * Hook Rules Panel —— hook rules + presets。
 *
 * 分两段展示：
 *  - 过程行为（process trigger）：工具调用、Turn 结束、子 Agent 交互等
 *  - 结束门禁（gate trigger）：Session 终态 / 结束前
 *
 * 受控组件：不直接依赖 workflowStore；容器把 store action 串回本组件。
 */

import { useMemo, useState } from "react";

import type {
  HookRulePreset,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
} from "../../../../types";
import { DetailSection } from "../../../../components/ui/detail-panel";
import {
  GATE_TRIGGERS,
  GATE_TRIGGER_OPTIONS,
  GATE_TRIGGER_ORDER,
  PROCESS_TRIGGERS,
  PROCESS_TRIGGER_OPTIONS,
  PROCESS_TRIGGER_ORDER,
  TRIGGER_LABEL,
  buildDefaultParams,
} from "./shared";

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
    <div
      className={`flex items-center gap-3 rounded-[10px] border px-3 py-2.5 transition-colors ${
        rule.enabled
          ? "border-border bg-background"
          : "border-border/40 bg-secondary/30 opacity-60"
      }`}
    >
      <button
        type="button"
        onClick={onToggle}
        className={`shrink-0 size-4 rounded-[4px] border-2 transition-colors ${
          rule.enabled
            ? "border-primary bg-primary"
            : "border-muted-foreground/40 bg-transparent"
        }`}
        title={rule.enabled ? "点击禁用" : "点击启用"}
      >
        {rule.enabled && (
          <svg
            viewBox="0 0 12 12"
            className="size-full text-primary-foreground"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          >
            <path d="M2 6l3 3 5-5" />
          </svg>
        )}
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-foreground">
            {rule.description || rule.key}
          </span>
          {rule.preset && (
            <span className="rounded bg-secondary px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
              {rule.preset}
            </span>
          )}
          {!rule.preset && rule.script && (
            <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-mono text-amber-600">
              rhai
            </span>
          )}
          <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary/70">
            {TRIGGER_LABEL[rule.trigger]}
          </span>
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

  const canCommit =
    mode === "preset"
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
              <option key={t} value={t}>
                {TRIGGER_LABEL[t]}
              </option>
            ))}
          </select>
        </div>

        {/* 行为类型 */}
        <div>
          <label className="text-[11px] font-medium text-muted-foreground">行为类型</label>
          <select
            value={mode}
            onChange={(e) => {
              setMode(e.target.value as "preset" | "script");
              setSelectedPreset("");
            }}
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
                <option key={p.key} value={p.key}>
                  {p.label}
                </option>
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
        <button
          type="button"
          onClick={onCancel}
          className="agentdash-button-secondary text-xs px-3 py-1"
        >
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
          onCommit={(rule) => {
            onAdd(rule);
            setAdding(false);
          }}
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

// ─── Panel 主组件 ──────────────────────────────────────

export interface HookRulesPanelProps {
  /** 所有 hook rules（合并过程 + 门禁后的平铺序列）。 */
  hookRules: WorkflowHookRuleSpec[];
  /** 可选 preset 列表；来自 store 的 hookPresets。 */
  presets: HookRulePreset[];
  onAdd: (rule: WorkflowHookRuleSpec) => void;
  onToggle: (ruleKey: string) => void;
  onRemove: (ruleKey: string) => void;
}

export function HookRulesPanel({
  hookRules,
  presets,
  onAdd,
  onToggle,
  onRemove,
}: HookRulesPanelProps) {
  const existingKeys = useMemo(
    () => new Set(hookRules.map((r) => r.key)),
    [hookRules],
  );
  const processRules = useMemo(
    () => hookRules.filter((r) => PROCESS_TRIGGERS.has(r.trigger)),
    [hookRules],
  );
  const gateRules = useMemo(
    () => hookRules.filter((r) => GATE_TRIGGERS.has(r.trigger)),
    [hookRules],
  );

  return (
    <>
      <DetailSection
        title={`过程行为 (${processRules.length})`}
        description="工具调用、Turn 结束、子 Agent 交互等过程中触发的 hook 行为。"
      >
        <HookRuleGroup
          rules={processRules}
          triggerOrder={PROCESS_TRIGGER_ORDER}
          triggerOptions={PROCESS_TRIGGER_OPTIONS}
          presets={presets}
          existingKeys={existingKeys}
          onAdd={onAdd}
          onToggle={onToggle}
          onRemove={onRemove}
        />
      </DetailSection>

      <DetailSection
        title={`结束门禁 (${gateRules.length})`}
        description="Session 结束前和终态判定时触发的 hook，控制完成条件和 step 推进。"
      >
        <HookRuleGroup
          rules={gateRules}
          triggerOrder={GATE_TRIGGER_ORDER}
          triggerOptions={GATE_TRIGGER_OPTIONS}
          presets={presets}
          existingKeys={existingKeys}
          onAdd={onAdd}
          onToggle={onToggle}
          onRemove={onRemove}
        />
      </DetailSection>
    </>
  );
}
