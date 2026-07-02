import { createContext, useContext } from "react";

export interface SessionWorkspacePanelTarget {
  typeId: string;
  uri?: string;
  options?: { refreshContent?: boolean };
}

export type OpenSessionWorkspacePanel = (target: SessionWorkspacePanelTarget) => void;

export const SessionWorkspacePanelActionContext = createContext<OpenSessionWorkspacePanel | null>(null);

export function useSessionWorkspacePanelAction(): OpenSessionWorkspacePanel | null {
  return useContext(SessionWorkspacePanelActionContext);
}
