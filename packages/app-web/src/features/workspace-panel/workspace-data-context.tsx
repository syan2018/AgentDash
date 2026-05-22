/* eslint-disable react-refresh/only-export-components */
/**
 * Session 工作空间数据上下文
 *
 * 为 WorkspacePanel 内的 Tab 内容组件提供 SessionPage 加载的会话相关数据，
 * 避免 TabTypeDescriptor.renderContent 的 props 接口膨胀。
 */

import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import type { WorkspaceRuntimeData } from "./workspace-panel-types";

export type WorkspaceData = WorkspaceRuntimeData;

const WorkspaceDataContext = createContext<WorkspaceData | null>(null);

export function WorkspaceDataProvider({
  value,
  children,
}: {
  value: WorkspaceData;
  children: ReactNode;
}) {
  return (
    <WorkspaceDataContext.Provider value={value}>
      {children}
    </WorkspaceDataContext.Provider>
  );
}

export function useWorkspaceData(): WorkspaceData {
  const ctx = useContext(WorkspaceDataContext);
  if (!ctx) {
    throw new Error("useWorkspaceData 必须在 WorkspaceDataProvider 内使用");
  }
  return ctx;
}
