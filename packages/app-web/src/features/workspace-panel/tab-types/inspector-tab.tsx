/* eslint-disable react-refresh/only-export-components */
import {
  ContextInspectorPanel,
  HookRuntimePendingActionsCard,
  HookRuntimeSurfaceCard,
  HookRuntimeTraceCard,
} from "../../session-context";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabTypeDescriptor } from "../tab-type-registry";
import { InspectorIcon } from "./icons";

function InspectorTabContent() {
  const { agentRunRuntimeTarget, hookRuntime } = useWorkspaceData();
  if (!agentRunRuntimeTarget) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          需要先进入 AgentRun workspace 才能查看上下文审计。
        </p>
      </div>
    );
  }
  return (
    <div className="flex h-full flex-col overflow-hidden">
      {hookRuntime && (
        <div className="max-h-[45vh] shrink-0 space-y-3 overflow-y-auto border-b border-border bg-secondary/10 p-3">
          <HookRuntimeSurfaceCard hookRuntime={hookRuntime} />
          <HookRuntimePendingActionsCard hookRuntime={hookRuntime} />
          <HookRuntimeTraceCard hookRuntime={hookRuntime} />
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-hidden">
        <ContextInspectorPanel
          agentRunTarget={agentRunRuntimeTarget}
        />
      </div>
    </div>
  );
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
