import { describe, expect, it } from "vitest";

import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";
import { managedRuntimeTestFixtures } from "./managedRuntimeTestFixtures";
import {
  commandIsAvailable,
  projectAgentRunRuntimeSnapshot,
} from "./useAgentRunRuntimeFeed";

describe("AgentRun Runtime projection", () => {
  it("projects canonical Turn, Item, and Interaction state into the chat view model", () => {
    const baseline = managedRuntimeTestFixtures.snapshots.completed;
    const source: ManagedRuntimeSnapshot = {
      ...baseline,
      turns: [
        {
          id: "turn-1",
          source_turn_id: "turn-1",
          status: "completed",
          item_ids: ["input-1", "tool-1", "output-1"],
        },
      ],
      items: [
        {
          id: "input-1",
          turn_id: "turn-1",
          status: "completed",
          presentation: {
            body: {
              kind: "user_message",
              content: [{ kind: "text", text: "检查工作区" }],
            },
            started_at_ms: 1n,
            updated_at_ms: 2n,
            terminal: {
              outcome: "completed",
              completed_at_ms: 2n,
              duration_ms: 1n,
              process_exit: null,
              error: null,
            },
            body_digest: "sha256:input-body",
            presentation_digest: "sha256:input-presentation",
          },
        },
        {
          id: "tool-1",
          turn_id: "turn-1",
          status: "completed",
          presentation: {
            body: {
              kind: "generic_tool_activity",
              arguments: null,
              progress: [],
              result: { files: 3 },
              name: "workspace.inspect",
            },
            started_at_ms: 2n,
            updated_at_ms: 3n,
            terminal: {
              outcome: "completed",
              completed_at_ms: 3n,
              duration_ms: 1n,
              process_exit: null,
              error: null,
            },
            body_digest: "sha256:tool-body",
            presentation_digest: "sha256:tool-presentation",
          },
        },
        {
          id: "output-1",
          turn_id: "turn-1",
          status: "completed",
          presentation: {
            body: {
              kind: "agent_message",
              content: [{ kind: "text", text: "检查完成" }],
              phase: null,
            },
            started_at_ms: 3n,
            updated_at_ms: 4n,
            terminal: {
              outcome: "completed",
              completed_at_ms: 4n,
              duration_ms: 1n,
              process_exit: null,
              error: null,
            },
            body_digest: "sha256:output-body",
            presentation_digest: "sha256:output-presentation",
          },
        },
      ],
      interactions: [
        {
          id: "interaction-1",
          turn_id: "turn-1",
          item_id: "tool-1",
          request: {
            kind: "approval",
            prompt: "允许读取工作区？",
            reason: null,
            proposed_action: null,
          },
          status: "resolved",
          resolution: { kind: "approved" },
        },
      ],
    };

    const projection = projectAgentRunRuntimeSnapshot(source);

    expect(projection.rawEntries.map((entry) => entry.presentation.body.kind)).toEqual([
      "user_message",
      "generic_tool_activity",
      "agent_message",
    ]);
    expect(projection.interactions).toBe(source.interactions);
    expect(projection.turnSegments).toHaveLength(1);
    expect(projection.turnSegments[0]).toMatchObject({
      turnId: "turn-1",
      status: "completed",
      finalOutput: { id: "output-1" },
    });
  });

  it("projects compaction started, completed, failed, and lost without inventing lifecycle", () => {
    const projections = [
      managedRuntimeTestFixtures.snapshots.started,
      managedRuntimeTestFixtures.snapshots.completed,
      managedRuntimeTestFixtures.snapshots.failed,
      managedRuntimeTestFixtures.snapshots.lost,
    ].map(projectAgentRunRuntimeSnapshot);

    expect(
      projections.map((projection) => projection.rawEntries[0]?.status),
    ).toEqual([
      "running",
      "completed",
      "failed",
      "lost",
    ]);
    expect(
      projections.map((projection) => projection.turnSegments[0]?.status),
    ).toEqual(["active", "completed", "failed", "lost"]);
    expect(
      projections.map((projection) => {
        const item = projection.rawEntries[0];
        return item?.presentation.body.kind === "context_compaction"
          ? item.presentation.body
          : null;
      }),
    ).toHaveLength(4);
  });

  it("derives control state only from canonical command availability", () => {
    const started = managedRuntimeTestFixtures.snapshots.started;
    const completed = managedRuntimeTestFixtures.snapshots.completed;

    expect(commandIsAvailable(started.command_availability.submit_input)).toBe(
      false,
    );
    expect(
      commandIsAvailable(completed.command_availability.submit_input),
    ).toBe(true);
    expect(commandIsAvailable(undefined)).toBe(false);
  });
});
