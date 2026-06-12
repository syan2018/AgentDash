import type {
  WorkspaceBackendTarget,
  WorkspaceRuntimeData,
} from "../workspace-runtime";

/** WorkspacePanel 对外命令式 API */
export interface WorkspacePanelHandle {
  /** 按类型打开或激活 Tab；可选传入 URI 定位到具体目标 */
  openTab: (typeId: string, uri?: string) => void;
}

export type { WorkspaceBackendTarget, WorkspaceRuntimeData };

export interface WorkspacePanelProps {
  runtimeData: WorkspaceRuntimeData;
  onWorkspaceModuleOpened?: () => void;
}
