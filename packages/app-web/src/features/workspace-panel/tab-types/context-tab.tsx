/* eslint-disable react-refresh/only-export-components */
import { ContextOverviewTab } from "../ContextOverviewTab";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabTypeDescriptor } from "../tab-type-registry";
import { ContextIcon } from "./icons";

function ContextTabContent() {
  const data = useWorkspaceData();
  return (
    <ContextOverviewTab
      contextSnapshot={data.contextSnapshot}
      ownerStory={data.ownerStory}
      ownerProjectName={data.ownerProjectName}
      executorSummary={data.executorSummary}
      runtimeSurface={data.runtimeSurface}
      hookRuntime={data.hookRuntime}
      sessionCapabilities={data.sessionCapabilities}
      lifecycleRun={data.lifecycleRun}
    />
  );
}

export const contextTabType: TabTypeDescriptor = {
  typeId: "context",
  label: "上下文",
  icon: ContextIcon,
  allowMultiple: false,
  pinned: true,
  renderContent: () => <ContextTabContent />,
  resolveTitle: () => "上下文",
  parseUri: () => ({}),
  buildUri: () => "context://overview",
  defaultUri: "context://overview",
  menuOrder: 0,
};
