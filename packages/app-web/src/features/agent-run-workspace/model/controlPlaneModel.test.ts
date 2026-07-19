import { describe, expect, it } from "vitest";

import type { ProjectEventStreamEnvelope } from "../../../generated/project-contracts";
import { managedRuntimeTestFixtures } from "../../agent-run-runtime/model/managedRuntimeTestFixtures";
import {
  planAgentRunMessageSent,
  planAgentRunProjectEvent,
  planAgentRunRuntimeChanges,
} from "./controlPlaneModel";

describe("AgentRun canonical control-plane model", () => {
  it("plans immediate Product refresh after a submitted message", () => {
    expect(planAgentRunMessageSent()).toEqual({
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "message_sent",
      hookRuntimeRefresh: { reason: "message_sent", immediate: true },
    });
  });

  it("derives refreshes from committed Managed Runtime changes", () => {
    expect(
      planAgentRunRuntimeChanges(managedRuntimeTestFixtures.changePage.changes),
    ).toEqual({
      effects: {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "managed_runtime_projection_changed",
      },
      refreshTaskPlan: true,
    });
  });

  it("uses typed project title invalidation for the active AgentRun", () => {
    const event: ProjectEventStreamEnvelope = {
      type: "ControlPlaneProjectionChanged",
      data: {
        project_id: "project-1",
        change: {
          projection: "agent_run_list",
          reason: "title_changed",
          run_id: "run-1",
          agent_id: "agent-1",
          frame_id: null,
          gate_id: null,
          mailbox_message_id: null,
        },
      },
    };

    expect(
      planAgentRunProjectEvent(event, { runId: "run-1", agentId: "agent-1" }),
    ).toEqual({
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "title_changed",
    });
  });
});
