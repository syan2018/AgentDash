import { describe, expect, it } from "vitest";

import {
  encodeAgentRuntimeStreamFrame,
  type AgentRuntimeStreamFrame,
} from "../../../generated/agent-runtime-codecs";
import { agentRuntimeTestFixtures } from "./agentRuntimeTestFixtures";
import { parseAgentRuntimeStreamFrame } from "./agentRuntimeUpdateTransport";

describe("Agent Runtime stream transport boundary", () => {
  const {
    conversation: _conversation,
    ...state
  } = agentRuntimeTestFixtures.snapshots.started.observation;

  it("解码 baseline、update 与 reset-required typed frame", () => {
    const frames: AgentRuntimeStreamFrame[] = [
      {
        kind: "baseline",
        connection_epoch: 7n,
        view: agentRuntimeTestFixtures.snapshots.completed,
      },
      {
        kind: "update",
        connection_epoch: 7n,
        lane_sequence: 9n,
        state,
        presentations: [],
      },
      {
        kind: "reset_required",
        connection_epoch: 7n,
        reason: "lagged",
        last_sequence: 9n,
      },
    ];

    for (const frame of frames) {
      expect(
        parseAgentRuntimeStreamFrame(encodeAgentRuntimeStreamFrame(frame)),
      ).toEqual(frame);
    }
  });

  it("拒绝没有 frame kind 的旧 update 形状", () => {
    expect(
      parseAgentRuntimeStreamFrame({
        lane_sequence: "1",
        state: null,
        presentations: [],
      }),
    ).toBeNull();
  });
});
