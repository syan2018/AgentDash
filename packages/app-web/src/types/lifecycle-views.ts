export type {
  AgentFrameRefDto,
  AgentRunRefDto,
  LifecycleRunRefDto,
} from "../generated/agent-run-mailbox-contracts";
export type {
  SubjectRefDto,
} from "../generated/project-agent-contracts";
export type {
  ActiveRuntimeNodeRefDto,
  AgentFrameRuntimeView,
  AgentRunLineageRef,
  AgentRunListChildView,
  AgentRunListEntryView,
  LifecycleAgentExecutionView,
  LifecycleAgentRuntimeBindingView,
  LifecycleExecutionAttemptView,
  LifecycleNodePortValueView,
  AgentRunView,
  AgentRunWorkspaceControlPlaneStatus,
  AgentRunWorkspaceControlPlaneView,
  AgentRunWorkspaceView,
  LifecycleRunView,
  LifecycleRuntimeExecutionTraceView,
  LifecycleRuntimeNodeErrorView,
  LifecycleRuntimeNodeKind,
  LifecycleRuntimeNodeStatus,
  LifecycleRuntimeNodeView,
  LifecycleRuntimeTraceAbsenceReason,
  LifecycleRuntimeTraceFenceEvidenceView,
  LifecycleRuntimeTraceRefView,
  LifecycleRuntimeTraceStaleReason,
  LifecycleSubjectAssociationDto,
  OrchestrationInstanceView,
  ProjectActiveAgentsView,
  ProjectAgentRunListView,
  RuntimeNodeView,
  RuntimeThreadRefDto,
  SubjectExecutionAttemptView,
  SubjectExecutionView,
} from "../generated/workflow-contracts";

import type {
  AgentRunListChildView,
  AgentRunListEntryView,
  LifecycleAgentExecutionView,
  LifecycleRunView,
} from "../generated/workflow-contracts";
import type { RuntimeThreadId } from "../generated/agent-runtime-contracts";

export type AgentRunListChild = AgentRunListChildView;
export type AgentRunWorkspaceListEntry = AgentRunListEntryView;

export interface LifecycleRuntimeTraceSummary {
  agent: LifecycleAgentExecutionView["agent"];
  state: LifecycleAgentExecutionView["runtime"]["state"];
  runtimeThreadId: RuntimeThreadId | null;
  reason: string | null;
}

export function lifecycleRuntimeTraceSummaries(
  run: LifecycleRunView,
): LifecycleRuntimeTraceSummary[] {
  return run.agents.map(({ agent, runtime }) => {
    switch (runtime.state) {
      case "absent":
        return {
          agent,
          state: runtime.state,
          runtimeThreadId: null,
          reason: runtime.reason,
        };
      case "current":
        return {
          agent,
          state: runtime.state,
          runtimeThreadId: runtime.binding.runtime_thread_id,
          reason: null,
        };
      case "stale":
        return {
          agent,
          state: runtime.state,
          runtimeThreadId:
            runtime.evidence.observed_runtime_thread_id
            ?? runtime.evidence.expected_runtime_thread_id,
          reason: runtime.reason,
        };
    }
  });
}

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
