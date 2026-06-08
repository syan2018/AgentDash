import type {
  LifecycleExecutionEventKind,
  DefinitionSource,
  WorkflowRunStatus,
  WorkflowTargetKind,
} from "../../types";

export const TARGET_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
};

export const TARGET_KIND_OPTIONS: WorkflowTargetKind[] = ["project", "story"];

export function formatTargetKinds(targetKinds: WorkflowTargetKind[]): string {
  return targetKinds.map((kind) => TARGET_KIND_LABEL[kind]).join(" / ");
}

export const RUN_STATUS_LABEL: Record<WorkflowRunStatus, string> = {
  draft: "Draft",
  ready: "Ready",
  running: "Running",
  blocked: "Blocked",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export const RUNTIME_NODE_STATUS_LABEL: Record<string, string> = {
  pending: "Pending",
  ready: "Ready",
  claiming: "Claiming",
  running: "Running",
  blocked: "Blocked",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
  skipped: "Skipped",
};

export const DEFINITION_SOURCE_LABEL: Record<DefinitionSource, string> = {
  builtin_seed: "Built-in",
  user_authored: "User Authored",
  cloned: "Cloned",
};

export const EXECUTION_EVENT_KIND_LABEL: Record<LifecycleExecutionEventKind, string> = {
  activity_activated: "Activity Activated",
  activity_completed: "Activity Completed",
  constraint_blocked: "Constraint Blocked",
  completion_evaluated: "Completion Evaluated",
  artifact_appended: "Artifact Appended",
  context_injected: "Context Injected",
};
