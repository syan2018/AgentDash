import type {
  ExtensionRuntimeProjectionResponse,
  AgentFrameHookRuntimeInfo,
  AgentFrameRuntimeView,
  LifecycleRunView,
  LifecycleAgentView,
  LifecycleSubjectAssociationDto,
  RuntimeSessionExecutionAnchorDto,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  SessionShellDto,
  Story,
  TaskSessionExecutorSummary,
} from "../../../types";

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
  runtimeSessionId: string | null;
  sessionMeta: SessionShellDto | null;
  controlAnchor: RuntimeSessionExecutionAnchorDto | null;
  lifecycleRun: LifecycleRunView | null;
  lifecycleAgent: LifecycleAgentView | null;
  frameRuntime: AgentFrameRuntimeView | null;
  subjectAssociations: LifecycleSubjectAssociationDto[];
  runtimeStatus: SessionRuntimeStateStatus;
  runtimeError: string | null;
  extensionRuntime: ProjectExtensionRuntimeState;
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  workspaceBackend: WorkspaceBackendTarget | null;
  hookRuntime: AgentFrameHookRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
  activeCanvasId: string | null;
}
