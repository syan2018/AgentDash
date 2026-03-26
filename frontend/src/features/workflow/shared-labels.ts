import type {
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
  project: "Project",
  story: "Story",
  task: "Task",
};

export const ROLE_ORDER: WorkflowAgentRole[] = ["project", "story", "task"];

export const DEFAULT_ROLE_BY_TARGET: Record<WorkflowTargetKind, WorkflowAgentRole> = {
  project: "project",
  story: "story",
  task: "task",
};

export const BINDING_KIND_LABEL: Record<WorkflowContextBindingKind, string> = {
  document_path: "Document Path",
  runtime_context: "Runtime Context",
  checklist: "Checklist",
  journal_target: "Journal Target",
  action_ref: "Action Ref",
  artifact_ref: "Artifact Ref",
};

export const RUN_STATUS_LABEL: Record<WorkflowRunStatus, string> = {
  draft: "Draft",
  ready: "Ready",
  running: "Running",
  blocked: "Blocked",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export const STEP_STATUS_LABEL: Record<WorkflowStepExecutionStatus, string> = {
  pending: "Pending",
  ready: "Ready",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
  skipped: "Skipped",
};

export const DEFINITION_SOURCE_LABEL: Record<WorkflowDefinitionSource, string> = {
  builtin_seed: "Built-in",
  user_authored: "User Authored",
  cloned: "Cloned",
};

export const DEFINITION_STATUS_LABEL: Record<WorkflowDefinitionStatus, string> = {
  draft: "Draft",
  active: "Active",
  disabled: "Disabled",
};

export const ARTIFACT_TYPE_LABEL: Record<WorkflowRecordArtifactType, string> = {
  session_summary: "Session Summary",
  journal_update: "Journal Update",
  archive_suggestion: "Archive Suggestion",
  phase_note: "Phase Note",
  checklist_evidence: "Checklist Evidence",
};
