import type {
  LifecycleExecutionEventKind,
  WorkflowAgentRole,
  WorkflowDefinitionSource,
  WorkflowDefinitionStatus,
  WorkflowRecordArtifactType,
  WorkflowRunStatus,
  WorkflowStepExecutionStatus,
  WorkflowTargetKind,
} from "../../types";

export const BINDING_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
  task: "Task",
};

/** @deprecated Use BINDING_KIND_LABEL */
export const TARGET_KIND_LABEL = BINDING_KIND_LABEL;

export const ROLE_LABEL: Record<WorkflowAgentRole, string> = BINDING_KIND_LABEL;

export const ROLE_ORDER: WorkflowAgentRole[] = ["project", "story", "task"];

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
  execution_trace: "Execution Trace",
  decision_record: "Decision Record",
  context_snapshot: "Context Snapshot",
};

export const EXECUTION_EVENT_KIND_LABEL: Record<LifecycleExecutionEventKind, string> = {
  step_activated: "Step Activated",
  step_completed: "Step Completed",
  constraint_blocked: "Constraint Blocked",
  completion_evaluated: "Completion Evaluated",
  artifact_appended: "Artifact Appended",
  context_injected: "Context Injected",
};
