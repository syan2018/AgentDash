import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { renderToStaticMarkup } from "react-dom/server";
import React from "react";
import { describe, expect, it, vi } from "vitest";

import { parseSessionEventEnvelopePayload } from "../../../AgentDash-main-reference/packages/app-web/src/features/session/model/sessionNdjsonEnvelopeValidator";
import {
  createInitialStreamState,
  reduceStreamState,
} from "../../../AgentDash-main-reference/packages/app-web/src/features/session/model/sessionStreamReducer";
import { SingleEntry } from "../../../AgentDash-main-reference/packages/app-web/src/features/session/ui/SessionEntry";
import {
  buildAgentRunConversationCommandState,
  projectAgentRunChatCommandState,
  projectAgentRunChatMailboxModel,
} from "../../../AgentDash-main-reference/packages/app-web/src/features/agent-run-workspace/model/conversationCommandState";
import { planAgentRunSystemEvent } from "../../../AgentDash-main-reference/packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel";
import {
  forkAgentRunFromMessageRef,
} from "../../../AgentDash-main-reference/packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fixtureRoot = resolve(
  repoRoot,
  "crates/agentdash-agent-runtime-test-support/fixtures/session-parity/browser",
);

function readFixture(name: string): Record<string, unknown> {
  return JSON.parse(readFileSync(resolve(fixtureRoot, name), "utf8")) as Record<string, unknown>;
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

function parseFrame(frame: unknown) {
  const parsed = parseSessionEventEnvelopePayload(frame);
  if (parsed.error) throw parsed.error;
  if (!parsed.event) throw new Error("Main frontend fixture frame did not produce an event");
  return parsed.event;
}

describe("pinned Main frontend production behavior", () => {
  it("replays submitted user input without creating a tool entry", () => {
    const fixture = readFixture("submit-refresh.json");
    const frames = (fixture.frames as unknown[]).map(parseFrame);
    const first = reduceStreamState(createInitialStreamState([]), frames);
    const refreshed = reduceStreamState(createInitialStreamState([]), frames);

    expect(refreshed.rawEvents).toEqual(first.rawEvents);
    expect(refreshed.entries).toHaveLength(1);
    expect(refreshed.entries[0]?.event.type).toBe("user_input_submitted");
    expect(
      refreshed.entries.filter((entry) =>
        entry.event.type === "item_started"
        || entry.event.type === "item_updated"
        || entry.event.type === "item_completed"
      ),
    ).toHaveLength(0);
    expect(renderToStaticMarkup(<SingleEntry entry={refreshed.entries[0]!} />)).toContain("hello");
  });

  it("folds started updated completed into one terminal tool card", () => {
    const fixture = readFixture("tool-interaction.json");
    const protectedEvents = fixture.protected_events as Array<Record<string, unknown>>;
    const events = protectedEvents.map((event, index) => parseFrame({
      type: "event",
      session_id: "session-fixture",
      event_seq: index + 1,
      occurred_at_ms: index + 1,
      committed_at_ms: index + 1,
      session_update_type: event.type,
      turn_id: "turn-fixture",
      entry_index: index,
      notification: {
        sessionId: "session-fixture",
        source: { connectorId: "fixture-connector", connectorType: "native" },
        trace: { turnId: "turn-fixture", entryIndex: index },
        observedAt: "2026-07-10T12:00:00Z",
        event,
      },
    }));
    const state = reduceStreamState(createInitialStreamState([]), events);

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]?.event.type).toBe("item_completed");
    if (state.entries[0]?.event.type !== "item_completed") return;
    expect(state.entries[0].event.payload.item.id).toBe("turn_001:tool_001");
    expect(renderToStaticMarkup(<SingleEntry entry={state.entries[0]} />)).toContain("README.md");
  });

  it("preserves AgentRun fork mailbox context lineage status and system effects", async () => {
    const fixture = readFixture("agentrun-outer.json") as unknown as AgentRunOuterFixture;
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
});
