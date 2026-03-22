import type {
  WorkflowAgentRole,
  WorkflowContextBindingKind,
  WorkflowPhaseCompletionMode,
  WorkflowPhaseExecutionStatus,
  WorkflowRunStatus,
  WorkflowTargetKind,
} from "../../types";

export const TARGET_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
  task: "Task",
};

export const ROLE_LABEL: Record<WorkflowAgentRole, string> = {
  project_context_maintainer: "Project 上下文维护",
  story_lifecycle_companion: "Story 生命周期协作",
  task_execution_worker: "Task 执行",
  review_agent: "Review",
  record_agent: "Record",
};

export const ROLE_ORDER: WorkflowAgentRole[] = [
  "project_context_maintainer",
  "story_lifecycle_companion",
  "task_execution_worker",
  "review_agent",
  "record_agent",
];

export const DEFAULT_ROLE_BY_TARGET: Record<WorkflowTargetKind, WorkflowAgentRole> = {
  project: "project_context_maintainer",
  story: "story_lifecycle_companion",
  task: "task_execution_worker",
};

export const COMPLETION_MODE_LABEL: Record<WorkflowPhaseCompletionMode, string> = {
  manual: "手动完成",
  session_ended: "会话结束后完成",
  checklist_passed: "检查通过后完成",
};

export const BINDING_KIND_LABEL: Record<WorkflowContextBindingKind, string> = {
  document_path: "文档",
  runtime_context: "运行时上下文",
  checklist: "检查清单",
  journal_target: "记录目标",
  action_ref: "动作引用",
};

export const RUN_STATUS_LABEL: Record<WorkflowRunStatus, string> = {
  draft: "草稿",
  ready: "就绪",
  running: "运行中",
  blocked: "受阻",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

export const PHASE_STATUS_LABEL: Record<WorkflowPhaseExecutionStatus, string> = {
  pending: "待执行",
  ready: "就绪",
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  skipped: "已跳过",
};
