/**
 * ConditionEditor —— `TransitionCondition` 4 种 kind 的统一受控编辑器。
 *
 * - always: 无字段（仅 select）
 * - artifact_field_equals: activity / port / path / value
 * - human_decision_equals: activity / decision_port / value（默认下拉 approved/rejected）
 * - agent_signal_equals: activity / signal_key / value
 *
 * value 字段统一文本输入；artifact_field_equals 与 agent_signal_equals 的 value 类型为
 * `unknown`，编辑器把它当作 JSON 字面量解析（解析失败回落为字符串）。
 */

import { useMemo } from "react";

import type { ActivityDefinition, TransitionCondition } from "../../../../types";

export interface ConditionEditorProps {
  condition: TransitionCondition;
  activities: ActivityDefinition[];
  onChange: (next: TransitionCondition) => void;
}

const KINDS: TransitionCondition["kind"][] = [
  "always",
  "artifact_field_equals",
  "human_decision_equals",
  "agent_signal_equals",
];

const KIND_LABEL: Record<TransitionCondition["kind"], string> = {
  always: "Always",
  artifact_field_equals: "Artifact Field Equals",
  human_decision_equals: "Human Decision Equals",
  agent_signal_equals: "Agent Signal Equals",
};

function defaultConditionForKind(
  kind: TransitionCondition["kind"],
  fallbackActivity: string,
): TransitionCondition {
  switch (kind) {
    case "always":
      return { kind };
    case "artifact_field_equals":
      return {
        kind,
        activity: fallbackActivity,
        port: "",
        path: "",
        value: "",
      };
    case "human_decision_equals":
      return {
        kind,
        activity: fallbackActivity,
        decision_port: "decision",
        value: "approved",
      };
    case "agent_signal_equals":
      return {
        kind,
        activity: fallbackActivity,
        signal_key: "",
        value: "",
      };
  }
}

function stringifyValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function parseValue(input: string): unknown {
  try {
    return JSON.parse(input);
  } catch {
    return input;
  }
}

export function ConditionEditor({ condition, activities, onChange }: ConditionEditorProps) {
  const activityKeys = useMemo(() => activities.map((a) => a.key), [activities]);
  const fallbackActivity = activityKeys[0] ?? "";

  const handleKindChange = (nextKind: TransitionCondition["kind"]) => {
    if (nextKind === condition.kind) return;
    onChange(defaultConditionForKind(nextKind, fallbackActivity));
  };

  return (
    <div className="space-y-2">
      <select
        value={condition.kind}
        onChange={(e) => handleKindChange(e.target.value as TransitionCondition["kind"])}
        className="agentdash-form-select"
      >
        {KINDS.map((k) => (
          <option key={k} value={k}>
            {KIND_LABEL[k]}
          </option>
        ))}
      </select>

      {condition.kind === "artifact_field_equals" && (
        <div className="grid gap-2">
          <ActivitySelect
            label="来源 Activity"
            value={condition.activity}
            keys={activityKeys}
            onChange={(activity) => onChange({ ...condition, activity })}
          />
          <PortSelect
            label="Output Port"
            value={condition.port}
            ports={
              activities.find((a) => a.key === condition.activity)?.output_ports.map((p) => p.key) ?? []
            }
            onChange={(port) => onChange({ ...condition, port })}
          />
          <LabeledInput
            label="JSON Path"
            value={condition.path}
            onChange={(path) => onChange({ ...condition, path })}
            placeholder="$.status"
          />
          <LabeledInput
            label="Expected Value (JSON 字面量)"
            value={stringifyValue(condition.value)}
            onChange={(raw) => onChange({ ...condition, value: parseValue(raw) })}
            placeholder='"approved" 或 42'
          />
        </div>
      )}

      {condition.kind === "human_decision_equals" && (
        <div className="grid gap-2">
          <ActivitySelect
            label="来源 Activity"
            value={condition.activity}
            keys={activityKeys}
            onChange={(activity) => onChange({ ...condition, activity })}
          />
          <LabeledInput
            label="Decision Port"
            value={condition.decision_port}
            onChange={(decision_port) => onChange({ ...condition, decision_port })}
            placeholder="decision"
          />
          <div>
            <label className="agentdash-form-label">Expected Value</label>
            <select
              value={condition.value}
              onChange={(e) => onChange({ ...condition, value: e.target.value })}
              className="agentdash-form-select"
            >
              <option value="approved">approved</option>
              <option value="rejected">rejected</option>
            </select>
          </div>
        </div>
      )}

      {condition.kind === "agent_signal_equals" && (
        <div className="grid gap-2">
          <ActivitySelect
            label="来源 Activity"
            value={condition.activity}
            keys={activityKeys}
            onChange={(activity) => onChange({ ...condition, activity })}
          />
          <LabeledInput
            label="Signal Key（即 output port key）"
            value={condition.signal_key}
            onChange={(signal_key) => onChange({ ...condition, signal_key })}
            placeholder="status"
          />
          <LabeledInput
            label="Expected Value (JSON 字面量)"
            value={stringifyValue(condition.value)}
            onChange={(raw) => onChange({ ...condition, value: parseValue(raw) })}
            placeholder='"completed"'
          />
        </div>
      )}
    </div>
  );
}

function ActivitySelect({
  label,
  value,
  keys,
  onChange,
}: {
  label: string;
  value: string;
  keys: string[];
  onChange: (next: string) => void;
}) {
  return (
    <div>
      <label className="agentdash-form-label">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="agentdash-form-select"
      >
        {value && !keys.includes(value) && <option value={value}>{value}（已删除）</option>}
        {keys.map((k) => (
          <option key={k} value={k}>
            {k}
          </option>
        ))}
      </select>
    </div>
  );
}

function PortSelect({
  label,
  value,
  ports,
  onChange,
}: {
  label: string;
  value: string;
  ports: string[];
  onChange: (next: string) => void;
}) {
  return (
    <div>
      <label className="agentdash-form-label">{label}</label>
      <input
        value={value}
        list={`port-opts-${label}`}
        onChange={(e) => onChange(e.target.value)}
        className="agentdash-form-input"
        placeholder="output port key"
      />
      <datalist id={`port-opts-${label}`}>
        {ports.map((p) => (
          <option key={p} value={p} />
        ))}
      </datalist>
    </div>
  );
}

function LabeledInput({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
}) {
  return (
    <div>
      <label className="agentdash-form-label">{label}</label>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="agentdash-form-input"
        placeholder={placeholder}
      />
    </div>
  );
}
