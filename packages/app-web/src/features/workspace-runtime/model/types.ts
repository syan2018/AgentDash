import type {
  ExtensionRuntimeProjectionResponse,
  AgentFrameHookRuntimeInfo,
  AgentFrameRuntimeView,
  LifecycleRunView,
  AgentRunView,
  LifecycleSubjectAssociationDto,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
} from "../../../types";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";

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

export interface AgentRunCanvasBridgeBase {
  run_id: string;
  agent_id: string;
  project_id: string;
}

export interface AgentRunCanvasBridgeIdentity extends AgentRunCanvasBridgeBase {
  canvas_mount_id: string;
}

export interface WorkspaceRuntimeData {
  projectId: string | null;
  traceSessionId: string | null;
  agentRunRuntimeTarget?: AgentRunRuntimeTarget | null;
  agentRunCanvasBridgeBase?: AgentRunCanvasBridgeBase | null;
  refreshAgentRunWorkspace?: (() => Promise<unknown>) | null;
  lifecycleRun: LifecycleRunView | null;
  lifecycleAgent: AgentRunView | null;
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
}
