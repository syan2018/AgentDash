/**
 * Workflow panel 共享辅助函数。
 */

import type {
  CapabilityCatalogEntryDto,
  CapabilityScopeDto,
  JsonValue,
  WorkflowHookTrigger,
  WorkflowTargetKind,
} from "../../../../types";
import { WORKFLOW_HOOK_TRIGGERS } from "../../../../generated/workflow-contracts";

// ─── Hook trigger 分类 ────────────────────────────────

export const TRIGGER_LABEL: Record<WorkflowHookTrigger, string> = {
  user_prompt_submit: "用户 Prompt 提交",
  before_tool: "工具调用前",
  after_tool: "工具调用后",
  after_turn: "Turn 结束后",
  before_stop: "Runtime 结束前",
  session_terminal: "Runtime 终态",
  before_subagent_dispatch: "子 Agent 派发前",
  after_subagent_dispatch: "子 Agent 派发后",
  companion_result: "Companion 结果回流",
  before_compact: "上下文压缩前",
  after_compact: "上下文压缩后",
  before_provider_request: "LLM 请求前",
};

export const GATE_TRIGGERS: ReadonlySet<WorkflowHookTrigger> = new Set([
  "before_stop",
  "session_terminal",
]);

export const GATE_TRIGGER_OPTIONS: WorkflowHookTrigger[] = WORKFLOW_HOOK_TRIGGERS.filter(
  (trigger) => GATE_TRIGGERS.has(trigger),
);

export const PROCESS_TRIGGER_OPTIONS: WorkflowHookTrigger[] = WORKFLOW_HOOK_TRIGGERS.filter(
  (trigger) => !GATE_TRIGGERS.has(trigger),
);

export const PROCESS_TRIGGERS: ReadonlySet<WorkflowHookTrigger> = new Set(PROCESS_TRIGGER_OPTIONS);

export const PROCESS_TRIGGER_ORDER: WorkflowHookTrigger[] = PROCESS_TRIGGER_OPTIONS;
export const GATE_TRIGGER_ORDER: WorkflowHookTrigger[] = GATE_TRIGGER_OPTIONS;

// ─── Hook preset param schema → 默认值 ─────────────────

/**
 * 根据 preset param_schema 产出 params 默认值对象。
 * 与原 workflow-editor.tsx 中的 buildDefaultParams 行为一致。
 */
export function buildDefaultParams(
  schema: Record<string, unknown>,
): Record<string, JsonValue> | null {
  const props = schema.properties as Record<string, Record<string, unknown>> | undefined;
  if (!props) return null;
  const result: Record<string, JsonValue> = {};
  for (const [key, prop] of Object.entries(props)) {
    if (prop.type === "array") result[key] = [];
    else if (prop.type === "string") result[key] = "";
    else if (prop.type === "number") result[key] = 0;
    else if (prop.type === "boolean") result[key] = false;
  }
  return Object.keys(result).length > 0 ? result : null;
}

// ─── Target kinds 相关 ─────────────────────────────────

/** 切换单个 target kind 的勾选状态；至少保留一个。 */
export function toggleTargetKind(
  current: WorkflowTargetKind[],
  value: WorkflowTargetKind,
): WorkflowTargetKind[] {
  if (current.includes(value)) {
    const next = current.filter((kind) => kind !== value);
    return next.length > 0 ? next : current;
  }
  return [...current, value];
}

// ─── Capability catalog helpers ───────────────────────

export function capabilityScopeForTargetKind(kind: WorkflowTargetKind): CapabilityScopeDto {
  switch (kind) {
    case "project":
      return "project";
    case "story":
      return "story";
  }
  const unreachable: never = kind;
  return unreachable;
}

export function capabilityVisibleForTargetKind(
  entry: CapabilityCatalogEntryDto,
  targetKind: WorkflowTargetKind,
): boolean {
  return entry.allowed_scopes.includes(capabilityScopeForTargetKind(targetKind));
}

export function capabilityAutoGrantedForTargetKind(
  entry: CapabilityCatalogEntryDto,
  targetKind: WorkflowTargetKind,
): boolean {
  return entry.auto_granted && capabilityVisibleForTargetKind(entry, targetKind);
}

export function capabilityKnownInCatalog(
  catalog: ReadonlyArray<CapabilityCatalogEntryDto>,
  key: string,
): boolean {
  return catalog.some((entry) => entry.key === key);
}

export function extractMcpPresetName(key: string): string | null {
  return key.startsWith("mcp:") ? key.slice(4) : null;
}
