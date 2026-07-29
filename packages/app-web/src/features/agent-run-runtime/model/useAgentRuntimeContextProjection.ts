import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AgentRuntimeContextProjection,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import {
  fetchAgentRunRuntimeContextProjection,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";

type RuntimeContextCoordinate = AgentRuntimeView["observation"]["context"];

export type RuntimeContextProjectionState =
  | { status: "idle" }
  | { status: "loading"; targetKey: string }
  | {
      status: "ready";
      targetKey: string;
      projection: AgentRuntimeContextProjection;
    }
  | {
      status: "refreshing";
      targetKey: string;
      projection: AgentRuntimeContextProjection;
    }
  | {
      status: "error";
      targetKey: string;
      previous: AgentRuntimeContextProjection | null;
      error: Error;
    };

function targetKey(target: AgentRunRuntimeTarget): string {
  return `${target.runId}:${target.agentId}`;
}

function committedProjection(
  state: RuntimeContextProjectionState,
  key: string,
): AgentRuntimeContextProjection | null {
  if (state.status === "ready" || state.status === "refreshing") {
    return state.targetKey === key ? state.projection : null;
  }
  return state.status === "error" && state.targetKey === key
    ? state.previous
    : null;
}

export function validateAgentRuntimeContextProjectionCommit(
  projection: AgentRuntimeContextProjection,
  required: RuntimeContextCoordinate,
  committedRevision: bigint | null,
): bigint {
  const actual = projection.recipe.coordinate;
  const actualRevision = BigInt(actual.snapshot_revision);
  if (
    actualRevision < required.snapshot_revision
    || (committedRevision != null && actualRevision < committedRevision)
  ) {
    throw new Error("返回的模型上下文低于当前 required revision");
  }
  if (
    actualRevision === required.snapshot_revision
    && (
      actual.context_revision !== required.context_revision
      || actual.recipe_digest !== required.recipe_digest
      || actual.authority !== required.authority
      || actual.fidelity !== required.fidelity
    )
  ) {
    throw new Error("返回的模型上下文与 Runtime context coordinate 不一致");
  }
  return actualRevision;
}

export class AgentRuntimeContextProjectionFence {
  private targetKey: string | null = null;
  private committedRevision: bigint | null = null;

  activate(key: string): void {
    if (this.targetKey === key) return;
    this.targetKey = key;
    this.committedRevision = null;
  }

  clear(): void {
    this.targetKey = null;
    this.committedRevision = null;
  }

  commit(
    key: string,
    projection: AgentRuntimeContextProjection,
    required: RuntimeContextCoordinate,
  ): void {
    if (this.targetKey !== key) {
      throw new Error("Context projection target 已切换");
    }
    this.committedRevision = validateAgentRuntimeContextProjectionCommit(
      projection,
      required,
      this.committedRevision,
    );
  }
}

export function useAgentRuntimeContextProjection(input: {
  target: AgentRunRuntimeTarget | null;
  required: RuntimeContextCoordinate | null;
}): {
  state: RuntimeContextProjectionState;
  refresh: () => void;
} {
  const [state, setState] = useState<RuntimeContextProjectionState>({ status: "idle" });
  const [refreshGeneration, setRefreshGeneration] = useState(0);
  const requestGeneration = useRef(0);
  const fence = useRef(new AgentRuntimeContextProjectionFence());
  const runId = input.target?.runId;
  const agentId = input.target?.agentId;
  const snapshotRevision = input.required?.snapshot_revision;
  const contextRevision = input.required?.context_revision;
  const recipeDigest = input.required?.recipe_digest;
  const authority = input.required?.authority;
  const fidelity = input.required?.fidelity;

  useEffect(() => {
    if (
      runId === undefined
      || agentId === undefined
      || snapshotRevision === undefined
      || contextRevision === undefined
      || recipeDigest === undefined
      || authority === undefined
      || fidelity === undefined
    ) {
      const generation = ++requestGeneration.current;
      fence.current.clear();
      queueMicrotask(() => {
        if (requestGeneration.current === generation) {
          setState({ status: "idle" });
        }
      });
      return;
    }
    const target: AgentRunRuntimeTarget = { runId, agentId };
    const required: RuntimeContextCoordinate = {
      snapshot_revision: snapshotRevision,
      context_revision: contextRevision,
      recipe_digest: recipeDigest,
      authority,
      fidelity,
    };
    const key = targetKey(target);
    fence.current.activate(key);
    const generation = ++requestGeneration.current;
    const controller = new AbortController();
    queueMicrotask(() => {
      if (requestGeneration.current !== generation) return;
      setState((current) => {
        const previous = committedProjection(current, key);
        return previous
          ? { status: "refreshing", targetKey: key, projection: previous }
          : { status: "loading", targetKey: key };
      });
    });

    void fetchAgentRunRuntimeContextProjection(target, required, controller.signal)
      .then((projection) => {
        if (controller.signal.aborted || requestGeneration.current !== generation) return;
        fence.current.commit(key, projection, required);
        setState({ status: "ready", targetKey: key, projection });
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted || requestGeneration.current !== generation) return;
        const normalized = error instanceof Error
          ? error
          : new Error("模型上下文读取失败");
        setState((current) => ({
          status: "error",
          targetKey: key,
          previous: committedProjection(current, key),
          error: normalized,
        }));
      });

    return () => controller.abort();
  }, [
    runId,
    agentId,
    snapshotRevision,
    contextRevision,
    recipeDigest,
    authority,
    fidelity,
    refreshGeneration,
  ]);

  const refresh = useCallback(() => {
    setRefreshGeneration((generation) => generation + 1);
  }, []);

  return { state, refresh };
}
