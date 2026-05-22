import type {
  HookSessionRuntimeInfo,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
  WorkflowRun,
} from "../../types";

/** WorkspacePanel 对外命令式 API */
export interface WorkspacePanelHandle {
  /** 按类型打开或激活 Tab；可选传入 URI 定位到具体目标 */
  openTab: (typeId: string, uri?: string) => void;
}

export interface WorkspacePanelProps {
  sessionId: string | null;

  /** Context 概览 Tab 所需数据 */
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  hookRuntime: HookSessionRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;
  workflowRuns: WorkflowRun[];

  /** Canvas Tab 所需数据 */
  activeCanvasId: string | null;
}
