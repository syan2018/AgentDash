import { describe, expect, it } from "vitest";

import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-contracts";
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
          status: "completed",
          item_ids: ["input-1", "tool-1", "output-1"],
        },
      ],
      items: [
        {
          id: "input-1",
          turn_id: "turn-1",
          status: "completed",
          content: {
            kind: "user_input",
            content: [{ kind: "text", text: "检查工作区" }],
          },
          content_digest: "sha256:input",
        },
        {
          id: "tool-1",
          turn_id: "turn-1",
          status: "completed",
          content: {
            kind: "tool_result",
            name: "workspace.inspect",
            result: { files: 3 },
          },
          content_digest: "sha256:tool",
        },
        {
          id: "output-1",
          turn_id: "turn-1",
          status: "completed",
          content: {
            kind: "agent_output",
            content: [{ kind: "text", text: "检查完成" }],
          },
          content_digest: "sha256:output",
        },
      ],
      interactions: [
        {
          id: "interaction-1",
          turn_id: "turn-1",
          item_id: "tool-1",
          kind: "approval",
          prompt: "允许读取工作区？",
          status: "resolved",
        },
      ],
    };

    const projection = projectAgentRunRuntimeSnapshot(source);

    expect(projection.rawEntries.map((entry) => entry.event.type)).toEqual([
      "user_input_submitted",
      "item_completed",
      "agent_message_delta",
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
      projections.map((projection) => projection.rawEntries[0]?.event.type),
    ).toEqual([
      "item_started",
      "item_completed",
      "item_completed",
      "item_completed",
    ]);
    expect(
      projections.map((projection) => projection.turnSegments[0]?.status),
    ).toEqual(["active", "completed", "failed", "failed"]);
    expect(
      projections.map((projection) => {
        const event = projection.rawEntries[0]?.event;
        if (
          event?.type !== "item_started"
          && event?.type !== "item_completed"
        ) {
          return null;
        }
        return event.payload.item.type === "contextCompaction"
          ? event.payload.item
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
