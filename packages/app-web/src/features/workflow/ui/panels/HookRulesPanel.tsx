/**
 * Hook Rules Panel —— hook rules + presets。
 *
 * 视觉语言对齐 Overview：线性 section heading + 控件；无 DetailSection 边框灰底，
 * 无平铺 description 注释。
 *
 * 分两段：
 *  - 过程行为（process trigger）
 *  - 结束门禁（gate trigger）
 */

import { useMemo, useState } from "react";

import type {
  HookRulePreset,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
} from "../../../../types";
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
  const isPreset = !!rule.preset;
  const isScript = !rule.preset && !!rule.script;

  return (
    <div
      className={`rounded-[10px] border px-3 py-2 transition-colors ${
        rule.enabled ? "border-border bg-background" : "border-border/40 bg-secondary/20 opacity-60"
      }`}
    >
      <div className="flex items-start gap-2.5">
        <button
          type="button"
          onClick={onToggle}
          className={`mt-0.5 shrink-0 size-4 rounded-[4px] border-2 transition-colors ${
            rule.enabled ? "border-primary bg-primary" : "border-muted-foreground/40 bg-transparent"
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
        <div className="min-w-0 flex-1 space-y-1">
          <p className="break-words text-xs font-medium leading-[1.4] text-foreground">
            {rule.description || rule.key}
          </p>
          <div className="flex flex-wrap items-center gap-1.5">
            {isPreset && (
              <code
                className="break-all rounded bg-secondary px-1.5 py-0.5 text-[10px] font-mono leading-[1.3] text-muted-foreground"
                title={rule.preset ?? undefined}
              >
                {rule.preset}
              </code>
            )}
            {isScript && (
              <span className="rounded bg-warning/10 px-1.5 py-0.5 text-[10px] font-mono text-warning">
                rhai
              </span>
            )}
          </div>
          {rule.params && Object.keys(rule.params).length > 0 && (
            <p className="break-all text-[10px] font-mono leading-[1.4] text-muted-foreground">
              {JSON.stringify(rule.params)}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={onRemove}
          className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-xs text-destructive/70 hover:bg-destructive/10 hover:text-destructive"
          aria-label="删除"
        >
          ×
        </button>
      </div>
    </div>
  );
}

// ─── Inline new-rule editor ────────────────────────────

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
        script: undefined,
        enabled: true,
      });
    } else if (mode === "script") {
      onCommit({
        key: `custom_${Date.now()}`,
        trigger,
        description: description.trim(),
        preset: undefined,
        params: null,
        script: script.trim(),
        enabled: true,
      });
    }
  };

  return (
    <div className="space-y-2.5 rounded-[8px] border border-dashed border-primary/30 bg-primary/5 p-3">
      <div>
        <label className="agentdash-form-label">触发时机</label>
        <select
          value={trigger}
          onChange={(e) => handleTriggerChange(e.target.value as WorkflowHookTrigger)}
          className="agentdash-form-select"
        >
          {triggerOptions.map((t) => (
            <option key={t} value={t}>
              {TRIGGER_LABEL[t]}
            </option>
          ))}
        </select>
      </div>

      <div>
        <label className="agentdash-form-label">行为类型</label>
        <select
          value={mode}
          onChange={(e) => {
            setMode(e.target.value as "preset" | "script");
            setSelectedPreset("");
          }}
          className="agentdash-form-select"
        >
          <option value="preset">预设逻辑</option>
          <option value="script">自定义脚本 (Rhai)</option>
        </select>
      </div>

      {mode === "preset" ? (
        <div>
          <label className="agentdash-form-label">选择预设</label>
          <select
            value={selectedPreset}
            onChange={(e) => setSelectedPreset(e.target.value)}
            className="agentdash-form-select"
          >
            <option value="">-- 选择 --</option>
            {presetsForTrigger.map((p) => (
              <option key={p.key} value={p.key}>
                {p.label}
              </option>
            ))}
          </select>
          {activePreset && (
            <div className="mt-2 space-y-1.5">
              <p className="break-words text-[11px] leading-[1.5] text-muted-foreground">
                {activePreset.description}
              </p>
              {activePreset.script && (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <button
                    type="button"
                    onClick={() => setShowPresetScript(!showPresetScript)}
                    className="text-[11px] text-primary/70 underline hover:text-primary"
                  >
                    {showPresetScript ? "隐藏脚本" : "查看脚本"}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setMode("script");
                      setScript(activePreset.script || "");
                      setDescription(activePreset.label);
                      setSelectedPreset("");
                      setShowPresetScript(false);
                    }}
                    className="text-[11px] text-primary/70 underline hover:text-primary"
                  >
                    Clone 为自定义
                  </button>
                </div>
              )}
              {showPresetScript && activePreset.script && (
                <pre className="max-h-40 overflow-auto rounded-md border bg-secondary/30 p-2 text-[11px] font-mono leading-[1.6] text-foreground/80">
                  {activePreset.script}
                </pre>
              )}
            </div>
          )}
        </div>
      ) : (
        <>
          <div>
            <label className="agentdash-form-label">描述</label>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="agentdash-form-input"
              placeholder="这条 hook 做什么"
            />
          </div>
          <div>
            <label className="agentdash-form-label">Rhai 脚本</label>
            <textarea
              value={script}
              onChange={(e) => setScript(e.target.value)}
              rows={7}
              className="agentdash-form-textarea font-mono text-xs leading-[1.6]"
              placeholder={'if ctx.tool_name == "shell_exec" {\n    #{ block: "禁止执行 shell" }\n} else {\n    #{}\n}'}
              spellCheck={false}
            />
          </div>
        </>
      )}

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground hover:bg-secondary"
        >
          取消
        </button>
        <button
          type="button"
          onClick={handleCommit}
          disabled={!canCommit}
          className="rounded-[8px] border border-primary bg-primary px-2.5 py-1 text-xs text-primary-foreground hover:opacity-95 disabled:opacity-40"
        >
          确认添加
        </button>
      </div>
    </div>
  );
}

// ─── Rule group ────────────────────────────────────────

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
    <div className="space-y-2">
      {activeTriggers.map((trigger) => (
        <div key={trigger} className="space-y-1.5">
          <p className="text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/80">
            {TRIGGER_LABEL[trigger]}
          </p>
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
      {rules.length === 0 && !adding && (
        <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
      )}

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
          className="w-full rounded-[8px] border border-dashed border-border/70 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-primary/70"
        >
          + 添加
        </button>
      )}
    </div>
  );
}

// ─── Panel 主组件 ──────────────────────────────────────

export interface HookRulesPanelProps {
  hookRules: WorkflowHookRuleSpec[];
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
  const existingKeys = useMemo(() => new Set(hookRules.map((r) => r.key)), [hookRules]);
  const processRules = useMemo(
    () => hookRules.filter((r) => PROCESS_TRIGGERS.has(r.trigger)),
    [hookRules],
  );
  const gateRules = useMemo(
    () => hookRules.filter((r) => GATE_TRIGGERS.has(r.trigger)),
    [hookRules],
  );

  return (
    <section className="space-y-4">
      <div>
        <label className="agentdash-form-label">过程行为 ({processRules.length})</label>
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
      </div>

      <div>
        <label className="agentdash-form-label">结束门禁 ({gateRules.length})</label>
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
      </div>
    </section>
  );
}
