import type { ComponentType, ReactNode } from "react";

export type TabURI = string;

export interface TabContentRenderProps {
  uri: string;
  tabId: string;
  sessionId: string | null;
  isActive: boolean;
}

export interface TabTypeDescriptor {
  typeId: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
  allowMultiple: boolean;
  pinned: boolean;
  renderContent: (props: TabContentRenderProps) => ReactNode;
  resolveTitle: (uri: string) => string;
  parseUri: (uri: string) => Record<string, string> | null;
  buildUri: (params: Record<string, string>) => string;
  defaultUri?: string;
  menuOrder?: number;
}

export interface TabInstance {
  id: string;
  typeId: string;
  uri: string;
  title: string;
  pinned: boolean;
}

export interface SessionTabLayout {
  tabs: Array<{
    type_id: string;
    uri: string;
    title: string;
    pinned: boolean;
  }>;
  active_tab_uri: string | null;
}
