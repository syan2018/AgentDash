import type {
  ExecutionVfs,
  HookSessionRuntimeInfo,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionContextSnapshot,
  Story,
  TaskSessionExecutorSummary,
} from "../../types";

/** 右栏 Tab 类型 */
export type WorkspacePanelTab = "context" | "vfs" | "canvas" | "inspector";

/** WorkspacePanel 对外命令式 API */
export interface WorkspacePanelHandle {
  openTab: (tab: WorkspacePanelTab) => void;
}

export interface WorkspacePanelProps {
  sessionId: string | null;

  /** Context 概览 Tab 所需数据 */
  contextSnapshot: SessionContextSnapshot | null;
  ownerStory: Story | null;
  ownerProjectName: string;
  executorSummary: TaskSessionExecutorSummary | null;
  runtimeSurface: ResolvedVfsSurface | null;
  vfs: ExecutionVfs | null;
  hookRuntime: HookSessionRuntimeInfo | null;
  sessionCapabilities: SessionBaselineCapabilities | null;

  /** Canvas Tab 所需数据 */
  activeCanvasId: string | null;

  /** 当前激活的 Tab（受控） */
  activeTab: WorkspacePanelTab;
  onTabChange: (tab: WorkspacePanelTab) => void;
}
