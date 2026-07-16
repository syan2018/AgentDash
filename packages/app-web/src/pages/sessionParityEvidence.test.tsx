import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  parseSessionEventEnvelopePayload,
} from "../features/session/model/sessionNdjsonEnvelopeValidator";
import {
  createInitialStreamState,
  reduceStreamState,
} from "../features/session/model/sessionStreamReducer";
import { SingleEntry } from "../features/session/ui/SessionEntry";
import {
  buildAgentRunConversationCommandState,
  projectAgentRunChatCommandState,
  projectAgentRunChatMailboxModel,
} from "../features/agent-run-workspace/model/conversationCommandState";
import { planAgentRunSystemEvent } from "../features/agent-run-workspace/model/controlPlaneModel";
import {
  forkAgentRunFromMessageRef,
} from "../features/agent-run-workspace/model/useAgentRunWorkspaceCommands";

const apiPostMock = vi.hoisted(() => vi.fn());
vi.mock("../api/client", () => ({
  api: {
    delete: vi.fn(),
    get: vi.fn(),
    post: apiPostMock,
    put: vi.fn(),
  },
}));

import { respondAgentRunInteraction } from "../services/agentRunRuntime";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const fixtureRoot = resolve(
  repositoryRoot,
  "crates/agentdash-agent-runtime-test-support/fixtures/session-parity/browser",
);

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function record(value: unknown, path: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${path} must be an object`);
  return value;
}

function array(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
  return value;
}

function string(value: unknown, path: string): string {
  if (typeof value !== "string") throw new Error(`${path} must be a string`);
  return value;
}

function number(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${path} must be a finite number`);
  }
  return value;
}

function boolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${path} must be a boolean`);
  return value;
}

function nullableString(value: unknown, path: string): string | null {
  if (value === null) return null;
  return string(value, path);
}

function readJson(name: string): unknown {
  const parsed: unknown = JSON.parse(readFileSync(resolve(fixtureRoot, name), "utf8"));
  return parsed;
}

type ForkCall = Parameters<typeof forkAgentRunFromMessageRef>[0];
type ForkResponse = Awaited<ReturnType<ForkCall["forkService"]>>;

interface AgentRunOuterFixture {
  conversation_input: Parameters<typeof buildAgentRunConversationCommandState>[0];
  mailbox_input: Parameters<typeof projectAgentRunChatMailboxModel>[1];
  system_event: {
    event_type: string;
    event: Parameters<typeof planAgentRunSystemEvent>[1];
  };
  fork: {
    run_id: string;
    agent_id: string;
    client_command_id: string;
    fork_point_ref: ForkCall["forkPointRef"];
    response: ForkResponse;
  };
  expected: {
    command_ids: string[];
    compact_command_id: string;
    mailbox: Record<string, unknown>;
    system_effect: ReturnType<typeof planAgentRunSystemEvent>;
    fork_request: { run_id: string; agent_id: string; request: unknown };
    redirect: { runId: string; agentId: string };
    lineage: Record<string, unknown>;
    status_target: { run_id: string; agent_id: string };
  };
}

function parseSubmitFixture() {
  const raw = record(readJson("submit-refresh.json"), "submit fixture");
  const frames = array(raw.frames, "submit fixture.frames").map((frame, index) => {
    const result = parseSessionEventEnvelopePayload(frame);
    if (result.error) throw result.error;
    if (!result.event) throw new Error(`submit fixture.frames[${index}] did not produce an event`);
    return result.event;
  });
  const expected = record(raw.expected, "submit fixture.expected");
  return {
    frames,
    expected: {
      durableEventSeqs: array(expected.durable_event_seqs, "expected.durable_event_seqs")
        .map((value, index) => number(value, `expected.durable_event_seqs[${index}]`)),
      userItemId: string(expected.user_item_id, "expected.user_item_id"),
      userText: string(expected.user_text, "expected.user_text"),
      displayEntryCount: number(expected.display_entry_count, "expected.display_entry_count"),
      toolCardCount: number(expected.tool_card_count, "expected.tool_card_count"),
    },
  };
}

function parseToolFixture() {
  const raw = record(readJson("tool-interaction.json"), "tool fixture");
  const events = array(raw.protected_events, "tool fixture.protected_events").map((event, index) => {
    const result = parseSessionEventEnvelopePayload({
      type: "event",
      session_id: "session-fixture",
      event_seq: index + 1,
      occurred_at_ms: index + 1,
      committed_at_ms: index + 1,
      session_update_type: string(record(event, `protected_events[${index}]`).type, `protected_events[${index}].type`),
      turn_id: "turn-fixture",
      entry_index: index,
      notification: {
        sessionId: "session-fixture",
        source: { connectorId: "fixture-connector", connectorType: "native" },
        trace: { turnId: "turn-fixture", entryIndex: index },
        observedAt: "2026-07-10T12:00:00Z",
        event,
      },
    });
    if (result.error) throw result.error;
    if (!result.event) throw new Error(`protected_events[${index}] did not produce an event`);
    return result.event;
  });
  const route = record(raw.interaction_route_evidence, "interaction_route_evidence");
  const target = record(route.target, "interaction_route_evidence.target");
  const response = record(route.response, "interaction_route_evidence.response");
  const responseKind = string(response.kind, "interaction_route_evidence.response.kind");
  if (responseKind !== "denied") throw new Error("interaction fixture response must be denied");
  const expected = record(raw.expected, "tool fixture.expected");
  return {
    events,
    interaction: {
      target: {
        runId: string(target.runId, "interaction_route_evidence.target.runId"),
        agentId: string(target.agentId, "interaction_route_evidence.target.agentId"),
      },
      id: string(route.interaction_id, "interaction_route_evidence.interaction_id"),
      response: {
        kind: "denied" as const,
        reason: nullableString(response.reason, "interaction_route_evidence.response.reason"),
      },
      requestPath: string(route.request_path, "interaction_route_evidence.request_path"),
    },
    expected: {
      eventTypes: array(expected.event_types, "expected.event_types")
        .map((value, index) => string(value, `expected.event_types[${index}]`)),
      toolItemId: string(expected.tool_item_id, "expected.tool_item_id"),
      displayEntryCount: number(expected.display_entry_count, "expected.display_entry_count"),
      terminalStatus: string(expected.terminal_status, "expected.terminal_status"),
      terminalSuccess: boolean(expected.terminal_success, "expected.terminal_success"),
    },
  };
}

function collectFiles(root: string): string[] {
  return readdirSync(root).flatMap((name) => {
    const path = resolve(root, name);
    return statSync(path).isDirectory() ? collectFiles(path) : [path];
  });
}

function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

describe("W11 Main frontend parity evidence", () => {
  it("keeps submitted user input stable across durable replay without a phantom tool card", () => {
    const fixture = parseSubmitFixture();
    const first = reduceStreamState(createInitialStreamState([]), fixture.frames);
    const refreshed = reduceStreamState(createInitialStreamState([]), fixture.frames);

    expect(first.rawEvents.map((event) => event.event_seq)).toEqual(fixture.expected.durableEventSeqs);
    expect(refreshed.rawEvents).toEqual(first.rawEvents);
    expect(refreshed.entries).toHaveLength(fixture.expected.displayEntryCount);
    if (refreshed.entries[0]?.event.type !== "user_input_submitted") {
      throw new Error("user input was not preserved");
    }
    expect(refreshed.entries[0].event.payload.itemId).toBe(fixture.expected.userItemId);

    const markup = renderToStaticMarkup(<SingleEntry entry={refreshed.entries[0]} />);
    expect(markup).toContain(fixture.expected.userText);
    expect(
      refreshed.entries.filter((entry) =>
        entry.event.type === "item_started"
        || entry.event.type === "item_updated"
        || entry.event.type === "item_completed"
      ),
    ).toHaveLength(fixture.expected.toolCardCount);
  });

  it("updates one tool card in place and submits interaction through the real service", async () => {
    const fixture = parseToolFixture();
    const state = reduceStreamState(createInitialStreamState([]), fixture.events);

    expect(state.rawEvents.map((event) => event.notification.event.type)).toEqual(fixture.expected.eventTypes);
    expect(state.entries).toHaveLength(fixture.expected.displayEntryCount);
    if (state.entries[0]?.event.type !== "item_completed") throw new Error("tool did not complete");
    expect(state.entries[0].event.payload.item.id).toBe(fixture.expected.toolItemId);
    const terminalItem = state.entries[0].event.payload.item;
    expect("status" in terminalItem ? terminalItem.status : null).toBe(fixture.expected.terminalStatus);
    expect("success" in terminalItem ? terminalItem.success : null).toBe(fixture.expected.terminalSuccess);

    apiPostMock.mockResolvedValue({ command_receipt: { status: "accepted" } });
    await respondAgentRunInteraction(
      fixture.interaction.target,
      fixture.interaction.id,
      fixture.interaction.response,
    );
    expect(apiPostMock).toHaveBeenCalledWith(
      fixture.interaction.requestPath,
      fixture.interaction.response,
    );
  });

  it("keeps user to multiple tools to final assistant continuous with one card per item identity", () => {
    const turnId = "turn-main-continuation";
    const protectedEvents: unknown[] = [
      {
        type: "user_input_submitted",
        payload: {
          threadId: "thread-main-continuation",
          turnId,
          itemId: "user-main-continuation",
          submissionKind: "prompt",
          source: {
            namespace: "core",
            kind: "composer",
            actor: "user",
            displayLabelKey: "mailbox.source.core.composer",
          },
          content: [{ type: "text", text: "读取工作区并继续回答", textElements: [] }],
        },
      },
      {
        type: "reasoning_text_delta",
        payload: {
          threadId: "thread-main-continuation",
          turnId,
          itemId: "reason-main-continuation",
          contentIndex: 0,
          delta: "先读取两个来源并比较结果。",
        },
      },
      {
        type: "item_completed",
        payload: {
          item: {
            type: "reasoning",
            id: "reason-main-continuation",
            summary: [],
            content: ["先读取两个来源并比较结果。"],
          },
          threadId: "thread-main-continuation",
          turnId,
          completedAtMs: 1,
        },
      },
      {
        type: "item_started",
        payload: {
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_001",
            tool: "mounts_list",
            status: "inProgress",
            arguments: {},
            contentItems: null,
            durationMs: null,
            success: null,
            namespace: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          startedAtMs: 1,
        },
      },
      {
        type: "item_updated",
        payload: {
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_001",
            tool: "mounts_list",
            status: "inProgress",
            arguments: {},
            contentItems: [{ type: "inputText", text: "workspace" }],
            durationMs: null,
            success: null,
            namespace: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          updatedAtMs: 2,
        },
      },
      {
        type: "item_completed",
        payload: {
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_001",
            tool: "mounts_list",
            status: "completed",
            arguments: {},
            contentItems: [{ type: "inputText", text: "workspace" }],
            durationMs: null,
            success: true,
            namespace: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          completedAtMs: 3,
        },
      },
      {
        type: "item_started",
        payload: {
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_002",
            tool: "workspace_module_list",
            status: "inProgress",
            arguments: {},
            contentItems: null,
            durationMs: null,
            success: null,
            namespace: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          startedAtMs: 4,
        },
      },
      {
        type: "item_completed",
        payload: {
          item: {
            type: "dynamicToolCall",
            id: "turn_001:tool_002",
            tool: "workspace_module_list",
            status: "failed",
            arguments: {},
            contentItems: [{ type: "inputText", text: "module visibility denied" }],
            durationMs: null,
            success: false,
            namespace: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          completedAtMs: 5,
        },
      },
      {
        type: "agent_message_delta",
        payload: {
          threadId: "thread-main-continuation",
          turnId,
          itemId: "answer-main-continuation",
          delta: "已经读取完成。",
        },
      },
      {
        type: "item_completed",
        payload: {
          item: {
            type: "agentMessage",
            id: "answer-main-continuation",
            text: "已经读取完成。",
            phase: null,
            memoryCitation: null,
          },
          threadId: "thread-main-continuation",
          turnId,
          completedAtMs: 6,
        },
      },
      {
        type: "user_input_submitted",
        payload: {
          threadId: "thread-main-continuation",
          turnId: "turn-main-cancelled",
          itemId: "user-main-cancelled",
          submissionKind: "prompt",
          source: {
            namespace: "core",
            kind: "composer",
            actor: "user",
            displayLabelKey: "mailbox.source.core.composer",
          },
          content: [{ type: "text", text: "停止这一轮", textElements: [] }],
        },
      },
      {
        type: "error",
        payload: {
          error: {
            message: "cancelled",
            additionalDetails: null,
            codexErrorInfo: null,
          },
          threadId: "thread-main-continuation",
          turnId: "turn-main-cancelled",
          willRetry: false,
        },
      },
      {
        type: "platform",
        payload: {
          kind: "session_meta_update",
          data: {
            key: "turn_terminal",
            value: {
              terminal_type: "turn_interrupted",
              message: "cancelled",
              diagnostic: null,
            },
          },
        },
      },
      {
        type: "platform",
        payload: {
          kind: "session_rewound",
          data: {
            discarded_turn_id: "turn-main-cancelled",
            discarded_entry_index: null,
            stable_event_seq: 10,
            stable_turn_id: turnId,
            reason: "runtime_failure",
            replacement_turn_id: null,
            message: "cancelled",
          },
        },
      },
    ];
    const events = protectedEvents.map((event, index) => {
      const eventRecord = record(event, `protectedEvents[${index}]`);
      const payload = record(eventRecord.payload, `protectedEvents[${index}].payload`);
      const eventTurnId = typeof payload.turnId === "string" ? payload.turnId : turnId;
      const item = payload.item && typeof payload.item === "object"
        ? record(payload.item, `protectedEvents[${index}].payload.item`)
        : null;
      const toolCallId = item?.type === "dynamicToolCall" && typeof item.id === "string"
        ? item.id
        : undefined;
      const result = parseSessionEventEnvelopePayload({
        type: "event",
        session_id: "session-main-continuation",
        event_seq: index + 1,
        occurred_at_ms: index + 1,
        committed_at_ms: index + 1,
        session_update_type: string(eventRecord.type, `protectedEvents[${index}].type`),
        turn_id: eventTurnId,
        entry_index: index,
        tool_call_id: toolCallId,
        notification: {
          sessionId: "session-main-continuation",
          source: { connectorId: "fixture-connector", connectorType: "native" },
          trace: { turnId: eventTurnId, entryIndex: index },
          observedAt: "2026-07-15T00:00:00Z",
          event,
        },
      });
      if (result.error) throw result.error;
      if (!result.event) throw new Error(`protectedEvents[${index}] did not produce an event`);
      expect(result.event.notification.event).toEqual(event);
      return result.event;
    });
    const continuedState = reduceStreamState(createInitialStreamState([]), events.slice(0, 10));
    const state = reduceStreamState(createInitialStreamState([]), events);

    expect(continuedState.entries.map((entry) => entry.id)).toEqual([
      `user-input:${turnId}:user-main-continuation`,
      "item:reason-main-continuation",
      "item:turn_001:tool_001",
      "item:turn_001:tool_002",
      "item:answer-main-continuation",
    ]);
    expect(continuedState.entries.filter((entry) => entry.id === "item:turn_001:tool_001"))
      .toHaveLength(1);
    expect(continuedState.entries.filter((entry) => entry.id === "item:turn_001:tool_002"))
      .toHaveLength(1);
    expect(continuedState.entries[2]?.event.type).toBe("item_completed");
    expect(continuedState.entries[3]?.event.type).toBe("item_completed");
    expect(record(record(continuedState.entries[3]?.event, "failed tool event").payload, "failed tool payload").item)
      .toMatchObject({ status: "failed", success: false });
    expect(continuedState.entries[4]?.event.type).toBe("agent_message_delta");
    expect(continuedState.entries[4]?.accumulatedText).toBe("已经读取完成。");
    expect(continuedState.entries[4]?.isStreaming).toBe(false);
    expect(state.rawEvents.map((event) => event.notification.event.type)).toEqual(
      protectedEvents.map((event, index) => string(record(event, `protectedEvents[${index}]`).type, "type")),
    );
    expect(state.entries.map((entry) => entry.id)).toEqual([
      `user-input:${turnId}:user-main-continuation`,
      "item:reason-main-continuation",
      "item:turn_001:tool_001",
      "item:turn_001:tool_002",
      "item:answer-main-continuation",
      "user-input:turn-main-cancelled:user-main-cancelled",
      "event:12",
      "event:13",
      "event:14",
    ]);
    expect(state.entries.filter((entry) => entry.id === "item:turn_001:tool_001")).toHaveLength(1);
    expect(state.entries.filter((entry) => entry.id === "item:turn_001:tool_002")).toHaveLength(1);
    expect(state.entries[2]?.event.type).toBe("item_completed");
    expect(state.entries[3]?.event.type).toBe("item_completed");
    expect(state.entries[4]?.event.type).toBe("agent_message_delta");
    expect(state.entries[4]?.accumulatedText).toBe("已经读取完成。");
    expect(state.entries[4]?.isStreaming).toBe(false);
    expect(state.entries.slice(5).map((entry) => entry.event.type)).toEqual([
      "user_input_submitted",
      "error",
      "platform",
      "platform",
    ]);
    expect(state.rawEvents.at(-1)?.notification.event).toMatchObject({
      type: "platform",
      payload: { kind: "session_rewound" },
    });
  });

  it("preserves Main AgentRun fork mailbox context lineage status and system effects", async () => {
    const fixture = readJson("agentrun-outer.json") as AgentRunOuterFixture;
    const commandState = buildAgentRunConversationCommandState(fixture.conversation_input);
    const chat = projectAgentRunChatCommandState(commandState);
    const mailbox = projectAgentRunChatMailboxModel(commandState, fixture.mailbox_input);

    expect(chat.commands.map((command) => command.command_id)).toEqual(fixture.expected.command_ids);
    expect(chat.commands.find((command) => command.kind === "compact_context")?.command_id)
      .toBe(fixture.expected.compact_command_id);
    expect({
      paused: mailbox.paused,
      user_attention: mailbox.user_attention,
      hide_system_steer_messages: mailbox.hide_system_steer_messages,
      can_resume: mailbox.can_resume,
      message_ids: mailbox.messages.map((message) => message.id),
      waiting_ids: mailbox.waiting_items.map((item) => item.wait_id),
      resume_command_id: mailbox.resumeAction?.command_id,
      promote_command_id: mailbox.promoteAction?.command_id,
      delete_command_id: mailbox.deleteAction?.command_id,
    }).toEqual(fixture.expected.mailbox);
    expect(planAgentRunSystemEvent(fixture.system_event.event_type, fixture.system_event.event))
      .toEqual(fixture.expected.system_effect);

    const forkService = vi.fn<ForkCall["forkService"]>().mockResolvedValue(fixture.fork.response);
    const fetchAndIngestLifecycleRun = vi.fn<ForkCall["fetchAndIngestLifecycleRun"]>()
      .mockResolvedValue(null);
    const onAgentRunRedirect = vi.fn<ForkCall["onAgentRunRedirect"]>();
    await forkAgentRunFromMessageRef({
      runId: fixture.fork.run_id,
      agentId: fixture.fork.agent_id,
      forkPointRef: fixture.fork.fork_point_ref,
      clientCommandId: fixture.fork.client_command_id,
      forkService,
      fetchAndIngestLifecycleRun,
      onAgentRunRedirect,
    });
    expect(forkService).toHaveBeenCalledWith(
      fixture.expected.fork_request.run_id,
      fixture.expected.fork_request.agent_id,
      fixture.expected.fork_request.request,
    );
    expect(onAgentRunRedirect).toHaveBeenCalledWith(fixture.expected.redirect);
    const lineageForkPoint = fixture.fork.response.lineage.fork_point_ref;
    expect(lineageForkPoint).toBeDefined();
    if (!lineageForkPoint) throw new Error("fork response must preserve the Main lineage coordinate");
    expect({
      parent_run_id: fixture.fork.response.lineage.parent.run_ref.run_id,
      parent_agent_id: fixture.fork.response.lineage.parent.agent_ref.agent_id,
      child_run_id: fixture.fork.response.lineage.child.run_ref.run_id,
      child_agent_id: fixture.fork.response.lineage.child.agent_ref.agent_id,
      relation_kind: fixture.fork.response.lineage.relation_kind,
      turn_id: lineageForkPoint.turn_id,
      entry_index: lineageForkPoint.entry_index,
    }).toEqual(fixture.expected.lineage);
    expect({ run_id: fixture.fork.run_id, agent_id: fixture.fork.agent_id })
      .toEqual(fixture.expected.status_target);
  });

  it("keeps the Main session UI ledger at 95 byte-identical files plus ten explicit seams", () => {
    const currentRoot = resolve(repositoryRoot, "packages/app-web/src/features/session");
    const mainRoot = "D:/Projects/AgentDash-main-reference/packages/app-web/src/features/session";
    expect(existsSync(mainRoot), `Main reference is required at ${mainRoot}`).toBe(true);
    if (!existsSync(mainRoot)) throw new Error(`Main reference is required at ${mainRoot}`);

    const allowlistedSeams = [
      "model/companionSubagentDispatch.ts",
      "model/sessionStreamReducer.test.ts",
      "model/sessionStreamReducer.ts",
      "model/useSessionFeed.test.ts",
      "model/useSessionFeed.ts",
      "model/useSessionStream.ts",
      "ui/SessionEntry.tsx",
      "ui/ToolCallCardShell.tsx",
      "ui/bodies/CompanionSubagentDispatchCardBody.tsx",
      "ui/bodies/ReadCardBody.tsx",
    ];
    const mainFiles = collectFiles(mainRoot);
    const currentFiles = collectFiles(currentRoot);
    const differing = mainFiles
      .filter((mainPath) => {
        const currentPath = resolve(currentRoot, relative(mainRoot, mainPath));
        expect(existsSync(currentPath), `Current file is missing: ${currentPath}`).toBe(true);
        return sha256(mainPath) !== sha256(currentPath);
      })
      .map((mainPath) => relative(mainRoot, mainPath).replaceAll("\\", "/"));

    expect(mainFiles).toHaveLength(105);
    expect(currentFiles).toHaveLength(105);
    expect([...differing].sort()).toEqual([...allowlistedSeams].sort());
    expect(mainFiles.length - differing.length).toBe(95);
  });
});
