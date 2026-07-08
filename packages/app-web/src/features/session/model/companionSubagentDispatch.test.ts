import { describe, expect, it } from "vitest";
import type { AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { renderToolCallCard } from "../ui/toolCardRegistry";
import {
  parseCompanionSubagentDispatch,
  resolveCompanionSubagentKnownRef,
  resolveCompanionSubagentOpenTarget,
} from "./companionSubagentDispatch";

describe("companion subagent dispatch presentation", () => {
  it("shows request-only companion_request target=sub as pending subagent dispatch", () => {
    const item: AgentDashThreadItem = {
      type: "dynamicToolCall",
      id: "tool-request-only",
      namespace: null,
      tool: "companion_request",
      arguments: {
        target: "sub",
        payload: {
          agent_key: "reviewer",
          message: "Review the latest patch",
        },
      },
      status: "inProgress",
      contentItems: null,
      success: null,
      durationMs: null,
    };

    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).toMatchObject({
      source: "companion_request",
      title: "reviewer",
      childAgentId: null,
      status: "running",
      summary: "Review the latest patch",
      journalUri: null,
    });
  });

  it("recognizes dynamic companion_request target=sub with dispatch details", () => {
    const item: AgentDashThreadItem = {
      type: "dynamicToolCall",
      id: "tool-1",
      namespace: null,
      tool: "companion_request",
      arguments: {
        target: "sub",
        payload: {
          agent_key: "reviewer",
          message: "Review the latest patch",
        },
      },
      status: "completed",
      contentItems: [
        {
          type: "inputText",
          text: JSON.stringify({
            details: {
              kind: "companion_subagent_dispatch",
              companion_label: "Reviewer",
              child: {
                agent_id: "agent-child-1",
              },
              wait_activity: {
                tool: "wait",
                activity_ref: "gate-1",
                activity_refs: ["gate-1"],
              },
              status: "completed",
              summary: "Reviewer launched",
              result_preview: JSON.stringify({
                status: "completed",
                payload: {
                  type: "completion",
                  summary: "Review completed cleanly",
                  confidence: "high",
                },
              }),
            },
          }),
        },
      ],
      success: true,
      durationMs: null,
    };

    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).toMatchObject({
      source: "companion_request",
      title: "Reviewer",
      childAgentId: "agent-child-1",
      status: "completed",
      summary: "Reviewer launched",
      resultSummary: "Review completed cleanly",
      resultDetails: {
        status: "completed",
        payload: {
          type: "completion",
          confidence: "high",
        },
      },
      journalUri: "lifecycle://agent-runs/agent-child-1/sessions/messages",
      waitActivityRef: "gate-1",
    });
  });

  it("deduplicates completed result summary from dispatch summary", () => {
    const item: AgentDashThreadItem = {
      type: "dynamicToolCall",
      id: "tool-dedup-summary",
      namespace: null,
      tool: "companion_request",
      arguments: {
        target: "sub",
        payload: {
          agent_key: "reviewer",
          message: "Review the latest patch",
        },
      },
      status: "completed",
      contentItems: [
        {
          type: "inputText",
          text: JSON.stringify({
            details: {
              kind: "companion_subagent_dispatch",
              child: {
                agent_id: "agent-child-dedup",
              },
              status: "completed",
              summary: "已完成 5 次只读工具调用：task_read、fs_glob。",
              result_preview: JSON.stringify({
                status: "completed",
                payload: {
                  type: "completion",
                  summary: " 已完成 5 次只读工具调用：task_read、fs_glob。 ",
                  notes: ["链路通畅"],
                },
              }),
            },
          }),
        },
      ],
      success: true,
      durationMs: null,
    };

    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).toMatchObject({
      summary: null,
      resultSummary: "已完成 5 次只读工具调用：task_read、fs_glob。",
      resultDetails: {
        status: "completed",
        payload: {
          type: "completion",
          notes: ["链路通畅"],
        },
      },
    });
  });

  it("recognizes collabAgentToolCall spawnAgent and maps receiver to child agent id", () => {
    const item: AgentDashThreadItem = {
      type: "collabAgentToolCall",
      id: "collab-1",
      tool: "spawnAgent",
      status: "inProgress",
      senderThreadId: "parent-thread",
      receiverThreadIds: ["agent-child-2"],
      prompt: "Investigate session cards",
      model: "gpt-5",
      reasoningEffort: null,
      agentsStates: {
        "agent-child-2": {
          status: "running",
          message: "Reading files",
        },
      },
    };

    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).toMatchObject({
      source: "collab_spawn_agent",
      childAgentId: "agent-child-2",
      status: "running",
      summary: "Reading files",
      journalUri: "lifecycle://agent-runs/agent-child-2/sessions/messages",
    });
    expect(presentation?.rawProtocolRefs.receiver_thread_ids).toEqual(["agent-child-2"]);
  });

  it("uses current run id plus child agent id for workspace path", () => {
    const item: AgentDashThreadItem = {
      type: "collabAgentToolCall",
      id: "collab-2",
      tool: "spawnAgent",
      status: "completed",
      senderThreadId: "parent-thread",
      receiverThreadIds: ["agent/child"],
      prompt: null,
      model: null,
      reasoningEffort: null,
      agentsStates: {},
    };
    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).not.toBeNull();
    if (!presentation) return;

    expect(resolveCompanionSubagentOpenTarget(presentation, { currentRunId: "run 1" })).toEqual({
      enabled: true,
      path: "/agent-runs/run%201/agent%2Fchild",
    });
  });

  it("uses AgentRun projection refs when current run context is absent", () => {
    const item: AgentDashThreadItem = {
      type: "collabAgentToolCall",
      id: "collab-projection",
      tool: "spawnAgent",
      status: "completed",
      senderThreadId: "parent-thread",
      receiverThreadIds: ["agent-child-projected"],
      prompt: "Follow projection",
      model: null,
      reasoningEffort: null,
      agentsStates: {},
    };
    const presentation = parseCompanionSubagentDispatch(item);

    expect(presentation).not.toBeNull();
    if (!presentation) return;

    const knownAgentRefs = [{
      run_id: "run-projected",
      agent_id: "agent-child-projected",
      display_title: "Projected child",
      delivery_status: "running",
      last_activity_at: "2026-07-08T10:00:00Z",
    }];

    expect(resolveCompanionSubagentKnownRef(presentation, knownAgentRefs)).toMatchObject({
      display_title: "Projected child",
      delivery_status: "running",
    });
    expect(resolveCompanionSubagentOpenTarget(presentation, { knownAgentRefs })).toEqual({
      enabled: true,
      path: "/agent-runs/run-projected/agent-child-projected",
    });
  });

  it("keeps raw receiver refs out of the default card header", () => {
    const item: AgentDashThreadItem = {
      type: "collabAgentToolCall",
      id: "collab-3",
      tool: "spawnAgent",
      status: "completed",
      senderThreadId: "parent-thread",
      receiverThreadIds: ["agent-child-3"],
      prompt: "Write focused tests",
      model: null,
      reasoningEffort: null,
      agentsStates: {},
    };

    const card = renderToolCallCard(item, {
      agentRunTarget: { runId: "run-1", agentId: "agent-parent" },
    });

    expect(card.header.primary).toBe("Write focused tests");
    expect(card.header.secondary).toBe("子 Agent");
    expect(card.header.secondary).not.toContain("agent-child-3");
    expect(card.header.secondary).not.toContain("receiverThreadIds");
    expect(card.header.secondary).not.toContain("目标线程");
  });
});
