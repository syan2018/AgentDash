import type {
  ExtensionRuntimeProjectionResponse,
  HookSessionRuntimeInfo,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
  WorkflowRun,
} from "../../types";

export type SessionRuntimeStateStatus = "idle" | "loading" | "ready" | "refreshing" | "error";

export type ProjectExtensionRuntimeStatus = "idle" | "loading" | "ready" | "refreshing" | "error";

export interface ProjectExtensionRuntimeState {
  project_id: string | null;
  status: ProjectExtensionRuntimeStatus;
  projection: ExtensionRuntimeProjectionResponse;
  error: string | null;
}

export interface WorkspaceBackendTarget {
  backend_id: string;
  label: string;
  online: boolean;
}

export interface WorkspaceRuntimeData {
  projectId: string | null;
  sessionId: string | null;
  runtimeStatus: SessionRuntimeStateStatus;
  runtimeError: string | null;
  extensionRuntime: ProjectExtensionRuntimeState;
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  workspaceBackend: WorkspaceBackendTarget | null;
  hookRuntime: HookSessionRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
  workflowRuns: WorkflowRun[];
  activeCanvasId: string | null;
}
