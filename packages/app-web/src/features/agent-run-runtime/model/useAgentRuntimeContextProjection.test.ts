import { describe, expect, it } from "vitest";

import type {
  AgentRuntimeContextProjection,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import { AgentRuntimeContextProjectionFence } from "./useAgentRuntimeContextProjection";

type Coordinate = AgentRuntimeView["observation"]["context"];

function coordinate(revision: bigint, digest = `sha256:context-${revision}`): Coordinate {
  return {
    snapshot_revision: revision,
    context_revision: `context-${revision}`,
    recipe_digest: digest,
    authority: "agent_owned",
    fidelity: "exact",
  };
}

function projection(
  revision: bigint,
  digest = `sha256:context-${revision}`,
): AgentRuntimeContextProjection {
  return {
    thread_id: "thread-1",
    recipe: {
      coordinate: coordinate(revision, digest),
      contributions: [],
    },
  };
}

describe("AgentRuntimeContextProjectionFence", () => {
  it("拒绝低于 required 或已提交 revision 的响应", () => {
    const fence = new AgentRuntimeContextProjectionFence();
    fence.activate("run-1:agent-1");
    fence.commit("run-1:agent-1", projection(8n), coordinate(7n));

    expect(() => {
      fence.commit("run-1:agent-1", projection(7n), coordinate(7n));
    }).toThrow("低于当前 required revision");
  });

  it("target 切换会清空旧 revision fence，并拒绝迟到的旧 target 响应", () => {
    const fence = new AgentRuntimeContextProjectionFence();
    fence.activate("run-1:agent-1");
    fence.commit("run-1:agent-1", projection(8n), coordinate(7n));
    fence.activate("run-2:agent-2");

    expect(() => {
      fence.commit("run-1:agent-1", projection(9n), coordinate(9n));
    }).toThrow("target 已切换");
    expect(() => {
      fence.commit("run-2:agent-2", projection(1n), coordinate(1n));
    }).not.toThrow();
  });

  it("同 revision 必须逐字段匹配 coordinate", () => {
    const fence = new AgentRuntimeContextProjectionFence();
    fence.activate("run-1:agent-1");

    expect(() => {
      fence.commit(
        "run-1:agent-1",
        projection(7n, "sha256:stale"),
        coordinate(7n, "sha256:required"),
      );
    }).toThrow("与 Runtime context coordinate 不一致");
  });
});
