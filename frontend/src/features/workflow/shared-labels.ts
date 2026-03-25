import type {
  LifecycleTransitionPolicyKind,
  WorkflowAgentRole,
  WorkflowContextBindingKind,
  WorkflowDefinitionSource,
  WorkflowDefinitionStatus,
  WorkflowRecordArtifactType,
  WorkflowRunStatus,
  WorkflowStepExecutionStatus,
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

export const TRANSITION_POLICY_LABEL: Record<LifecycleTransitionPolicyKind, string> = {
  manual: "手动推进",
  all_checks_pass: "全部检查通过后推进",
  any_checks_pass: "任一检查通过后推进",
  session_terminal_matches: "会话进入终态后推进",
  explicit_action: "收到显式动作后推进",
};

export const BINDING_KIND_LABEL: Record<WorkflowContextBindingKind, string> = {
  document_path: "文档",
  runtime_context: "运行时上下文",
  checklist: "检查清单",
  journal_target: "记录目标",
  action_ref: "动作引用",
  artifact_ref: "产物引用",
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

export const STEP_STATUS_LABEL: Record<WorkflowStepExecutionStatus, string> = {
  pending: "待执行",
  ready: "就绪",
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  skipped: "已跳过",
};

export const DEFINITION_SOURCE_LABEL: Record<WorkflowDefinitionSource, string> = {
  builtin_seed: "内置模板",
  user_authored: "用户创建",
  cloned: "克隆",
};

export const DEFINITION_STATUS_LABEL: Record<WorkflowDefinitionStatus, string> = {
  draft: "草稿",
  active: "已激活",
  disabled: "已停用",
};

export const ARTIFACT_TYPE_LABEL: Record<WorkflowRecordArtifactType, string> = {
  session_summary: "会话总结",
  journal_update: "日志更新",
  archive_suggestion: "归档建议",
  phase_note: "阶段笔记",
  checklist_evidence: "检查清单证据",
};
