import { describe, expect, it, vi } from "vitest";
import type {
  AgentRunForkResponse,
  AgentRunMessageCommandResponse,
} from "../../../generated/agent-run-mailbox-contracts";
import {
  forkAgentRunFromMessageRef,
  resolveAgentRunCommandRedirect,
} from "./useAgentRunWorkspaceCommands";

function forkCommandResponse(): AgentRunMessageCommandResponse {
  return {
    command_receipt: {
      client_command_id: "cmd-submit",
      status: "accepted",
      duplicate: false,
    },
    outcome: "launched",
    accepted_refs: {
      run_ref: { run_id: "child-run" },
      agent_ref: { run_id: "child-run", agent_id: "child-agent" },
    },
    fork: {
      outcome: "forked",
      parent_refs: {
        run_ref: { run_id: "parent-run" },
        agent_ref: { run_id: "parent-run", agent_id: "parent-agent" },
      },
      child_refs: {
        run_ref: { run_id: "child-run" },
        agent_ref: { run_id: "child-run", agent_id: "child-agent" },
      },
      lineage: {
        id: "lineage-1",
        parent: {
          run_ref: { run_id: "parent-run" },
          agent_ref: { run_id: "parent-run", agent_id: "parent-agent" },
        },
        child: {
          run_ref: { run_id: "child-run" },
          agent_ref: { run_id: "child-run", agent_id: "child-agent" },
        },
        relation_kind: "fork",
        fork_point_ref: { turn_id: "turn-1", entry_index: 2 },
        forked_by_user_id: "user-1",
        created_at: "2026-07-03T00:00:00.000Z",
      },
      redirect: { run_id: "child-run", agent_id: "child-agent" },
    },
  };
}

function explicitForkResponse(): AgentRunForkResponse {
  const commandResponse = forkCommandResponse();
  const fork = commandResponse.fork;
  if (!fork) {
    throw new Error("test fixture must include fork outcome");
  }
  return {
    command_receipt: commandResponse.command_receipt,
    outcome: fork.outcome,
    parent_refs: fork.parent_refs,
    child_refs: fork.child_refs,
    lineage: fork.lineage,
    redirect: fork.redirect,
  };
}

describe("AgentRun workspace command fork handling", () => {
  it("resolves composer fork outcome redirect to child AgentRun", () => {
    expect(resolveAgentRunCommandRedirect(forkCommandResponse())).toEqual({
      runId: "child-run",
      agentId: "child-agent",
    });
  });

  it("forks from stable MessageRef through AgentRun fork service and navigates child", async () => {
    const forkService = vi.fn<Parameters<typeof forkAgentRunFromMessageRef>[0]["forkService"]>()
      .mockResolvedValue(explicitForkResponse());
    const fetchAndIngestLifecycleRun = vi.fn<Parameters<typeof forkAgentRunFromMessageRef>[0]["fetchAndIngestLifecycleRun"]>()
      .mockResolvedValue(null);
    const onAgentRunRedirect = vi.fn<Parameters<typeof forkAgentRunFromMessageRef>[0]["onAgentRunRedirect"]>();

    await forkAgentRunFromMessageRef({
      runId: "parent-run",
      agentId: "parent-agent",
      forkPointRef: { turn_id: "turn-1", entry_index: 2 },
      clientCommandId: "cmd-fork",
      forkService,
      fetchAndIngestLifecycleRun,
      onAgentRunRedirect,
    });

    expect(forkService).toHaveBeenCalledWith("parent-run", "parent-agent", {
      client_command_id: "cmd-fork",
      fork_point_ref: { turn_id: "turn-1", entry_index: 2 },
    });
    expect(fetchAndIngestLifecycleRun).toHaveBeenCalledWith("child-run");
    expect(onAgentRunRedirect).toHaveBeenCalledWith({
      runId: "child-run",
      agentId: "child-agent",
    });
  });
});
