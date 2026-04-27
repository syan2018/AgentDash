import type {
  LifecycleExecutionEventKind,
  WorkflowDefinitionSource,
  WorkflowRunStatus,
  WorkflowStepExecutionStatus,
  WorkflowTargetKind,
} from "../../types";

export const TARGET_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
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

export const EXECUTION_EVENT_KIND_LABEL: Record<LifecycleExecutionEventKind, string> = {
  step_activated: "Step Activated",
  step_completed: "Step Completed",
  artifact_appended: "Artifact Appended",
  context_injected: "Context Injected",
};
