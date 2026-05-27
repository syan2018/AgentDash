import type {
  HookSessionRuntimeInfo,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
  WorkflowRun,
} from "../../types";
import type { ProjectExtensionRuntimeState } from "../extension-runtime/model/types";
import type { SessionRuntimeStateStatus } from "./model/useSessionRuntimeState";

/** WorkspacePanel 对外命令式 API */
export interface WorkspacePanelHandle {
  /** 按类型打开或激活 Tab；可选传入 URI 定位到具体目标 */
  openTab: (typeId: string, uri?: string) => void;
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

  /** Canvas Tab 所需数据 */
  activeCanvasId: string | null;
}

export interface WorkspacePanelProps {
  runtimeData: WorkspaceRuntimeData;
}
