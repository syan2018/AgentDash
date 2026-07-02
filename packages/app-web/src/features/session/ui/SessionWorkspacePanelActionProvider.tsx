import type { ReactNode } from "react";
import {
  SessionWorkspacePanelActionContext,
  type OpenSessionWorkspacePanel,
} from "./SessionWorkspacePanelActionContext";

export function SessionWorkspacePanelActionProvider({
  children,
  openWorkspacePanel,
}: {
  children: ReactNode;
  openWorkspacePanel?: OpenSessionWorkspacePanel;
}) {
  return (
    <SessionWorkspacePanelActionContext.Provider value={openWorkspacePanel ?? null}>
      {children}
    </SessionWorkspacePanelActionContext.Provider>
  );
}
