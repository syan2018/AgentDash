import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import type { WorkspaceRuntimeData } from "./types";

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

// eslint-disable-next-line react-refresh/only-export-components
export function useWorkspaceData(): WorkspaceData {
  const ctx = useContext(WorkspaceDataContext);
  if (!ctx) {
    throw new Error("useWorkspaceData 必须在 WorkspaceDataProvider 内使用");
  }
  return ctx;
}
