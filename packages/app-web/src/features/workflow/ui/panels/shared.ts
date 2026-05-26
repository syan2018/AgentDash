/**
 * Workflow panel 共享常量与辅助函数
 *
 * 这些常量原本散落在 workflow-editor.tsx 中。拆分 panel 后抽到此处，
 * 保证各 panel 与容器引用同一份源。行为完全等价，只做物理位置迁移。
 */

import type {
  JsonValue,
  WorkflowHookTrigger,
  WorkflowTargetKind,
} from "../../../../types";

// ─── Hook trigger 分类 ────────────────────────────────

export const TRIGGER_LABEL: Record<WorkflowHookTrigger, string> = {
  user_prompt_submit: "用户 Prompt 提交",
  before_tool: "工具调用前",
  after_tool: "工具调用后",
  after_turn: "Turn 结束后",
  before_stop: "Session 结束前",
  session_terminal: "Session 终态",
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

export const PROCESS_TRIGGERS: ReadonlySet<WorkflowHookTrigger> = new Set([
  "before_tool",
  "after_tool",
  "after_turn",
  "before_subagent_dispatch",
  "after_subagent_dispatch",
  "companion_result",
]);

export const PROCESS_TRIGGER_OPTIONS: WorkflowHookTrigger[] = [
  "before_tool",
  "after_tool",
  "after_turn",
  "before_subagent_dispatch",
  "after_subagent_dispatch",
  "companion_result",
];

export const GATE_TRIGGER_OPTIONS: WorkflowHookTrigger[] = [
  "before_stop",
  "session_terminal",
];

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

// ─── Capability 编辑器常量 ────────────────────────────

export const CAP_EDITOR_WELL_KNOWN_KEYS = [
  "file_read",
  "file_write",
  "shell_execute",
  "canvas",
  "workflow",
  "collaboration",
  "story_management",
  "task_management",
  "relay_management",
  "workflow_management",
] as const;

export type WellKnownCapabilityKey = (typeof CAP_EDITOR_WELL_KNOWN_KEYS)[number];

export const WELL_KNOWN_CAPABILITY_LABEL: Record<WellKnownCapabilityKey, string> = {
  file_read: "文件读取",
  file_write: "文件写入",
  shell_execute: "Shell 执行",
  canvas: "画布",
  workflow: "工作流",
  collaboration: "协作",
  story_management: "Story 管理",
  task_management: "Task 管理",
  relay_management: "Relay 管理",
  workflow_management: "工作流管理",
};

export const WELL_KNOWN_CAPABILITY_DESCRIPTION: Record<WellKnownCapabilityKey, string> = {
  file_read: "只读文件系统访问（fs_read、fs_glob、fs_grep 等）",
  file_write: "文件写入操作（fs_apply_patch）",
  shell_execute: "执行 shell 命令（shell_exec）",
  canvas: "画布 / 白板操作",
  workflow: "工作流汇报与推进",
  collaboration: "多 agent 协作通道",
  story_management: "创建 / 调整 Story",
  task_management: "创建 / 调整 Task",
  relay_management: "Relay 后端管理",
  workflow_management: "MCP workflow 管理工具",
};

/**
 * 各 target_kinds 下 auto_granted=true 的能力基线。
 * 镜像自后端 `crates/agentdash-spi/src/tool_capability.rs::default_visibility_rules`。
 * 若后端 visibility rule 调整，此处需同步更新。
 */
export const AUTO_GRANTED_BASELINE: Record<WorkflowTargetKind, WellKnownCapabilityKey[]> = {
  project: ["file_read", "file_write", "shell_execute", "canvas", "collaboration", "relay_management"],
  story: ["file_read", "file_write", "shell_execute", "story_management"],
};

export function isWellKnownCapability(key: string): key is WellKnownCapabilityKey {
  return (CAP_EDITOR_WELL_KNOWN_KEYS as readonly string[]).includes(key);
}

export function extractMcpPresetName(key: string): string | null {
  return key.startsWith("mcp:") ? key.slice(4) : null;
}
