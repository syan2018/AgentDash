/* eslint-disable react-refresh/only-export-components */
import { ContextInspectorPanel } from "../../session-context";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabTypeDescriptor } from "../tab-type-registry";
import { InspectorIcon } from "./icons";

function InspectorTabContent() {
  const { sessionId } = useWorkspaceData();
  if (!sessionId) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          需要先建立会话才能查看上下文审计。
        </p>
      </div>
    );
  }
  return <ContextInspectorPanel sessionId={sessionId} />;
}

export const inspectorTabType: TabTypeDescriptor = {
  typeId: "inspector",
  label: "审计",
  icon: InspectorIcon,
  allowMultiple: false,
  pinned: true,
  renderContent: () => <InspectorTabContent />,
  resolveTitle: () => "审计",
  parseUri: () => ({}),
  buildUri: () => "inspector://session",
  defaultUri: "inspector://session",
  menuOrder: 1,
};
