import { useCallback, useEffect, useMemo, useState } from "react";

import type { AgentRunCurrentFrameView, AgentRunProductView } from "../../../generated/workflow-contracts";
import type { AgentFrameHookRuntimeInfo } from "../../../types";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchAgentRunWorkspace } from "../../../services/lifecycle";
import {
  fetchAgentRunRuntimeInspect,
  type AgentRunRuntimeInspectResponse,
} from "../../../services/agentRunRuntime";
import type { WorkspaceRuntimeStateStatus } from "../../workspace-runtime";

export interface AgentRunWorkspaceState {
  run_id: string | null;
  agent_id: string | null;
  source_key: string | null;
  status: WorkspaceRuntimeStateStatus;
  workspace: AgentRunProductView | null;
  runtime_inspect: AgentRunRuntimeInspectResponse | null;
  runtime_surface: ResolvedVfsSurface | null;
  hook_runtime: AgentFrameHookRuntimeInfo | null;
  frame: AgentRunCurrentFrameView | null;
  runtime_surface_error: string | null;
  workspace_error: string | null;
  runtime_inspect_error: string | null;
  error: string | null;
}

interface UseAgentRunWorkspaceStateInput {
  runId: string | null;
  agentId: string | null;
  sourceKey: string | null;
}

type AgentRunWorkspaceLoadMode = "replace" | "refresh";

export function emptyAgentRunWorkspaceState(): AgentRunWorkspaceState {
  return {
    run_id: null,
    agent_id: null,
    source_key: null,
    status: "idle",
    workspace: null,
    runtime_inspect: null,
    runtime_surface: null,
    hook_runtime: null,
    frame: null,
    runtime_surface_error: null,
    workspace_error: null,
    runtime_inspect_error: null,
    error: null,
  };
}

function stateMatches(
  state: AgentRunWorkspaceState,
  runId: string,
  agentId: string,
  sourceKey: string,
): boolean {
  return state.run_id === runId && state.agent_id === agentId && state.source_key === sourceKey;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "AgentRun workspace state 加载失败";
}

export function agentRunWorkspaceResourceSurface(
  workspace: AgentRunProductView,
): ResolvedVfsSurface | null {
  return workspace.resource_surface ?? null;
}

export function beginAgentRunWorkspaceStateLoad(
  current: AgentRunWorkspaceState,
  runId: string,
  agentId: string,
  sourceKey: string,
  mode: AgentRunWorkspaceLoadMode,
): AgentRunWorkspaceState {
  if (mode === "refresh" && stateMatches(current, runId, agentId, sourceKey)) {
    return {
      ...current,
      status: current.frame ? "refreshing" : "loading",
      error: null,
      runtime_surface_error: null,
    };
  }

  return {
    ...emptyAgentRunWorkspaceState(),
    run_id: runId,
    agent_id: agentId,
    source_key: sourceKey,
    status: "loading",
  };
}

export function settleAgentRunWorkspaceStateLoad(
  current: AgentRunWorkspaceState,
  runId: string,
  agentId: string,
  sourceKey: string,
  mode: AgentRunWorkspaceLoadMode,
  workspaceResult: PromiseSettledResult<AgentRunProductView>,
  runtimeInspectResult: PromiseSettledResult<AgentRunRuntimeInspectResponse>,
): AgentRunWorkspaceState {
  const preserveCurrent = mode === "refresh" && stateMatches(current, runId, agentId, sourceKey);
  const workspace = workspaceResult.status === "fulfilled"
    ? workspaceResult.value
    : preserveCurrent ? current.workspace : null;
  const runtimeInspect = runtimeInspectResult.status === "fulfilled"
    ? runtimeInspectResult.value
    : preserveCurrent ? current.runtime_inspect : null;
  const workspaceError = workspaceResult.status === "rejected"
    ? errorMessage(workspaceResult.reason)
    : null;
  const runtimeInspectError = runtimeInspectResult.status === "rejected"
    ? errorMessage(runtimeInspectResult.reason)
    : null;
  return {
    run_id: runId,
    agent_id: agentId,
    source_key: sourceKey,
    status: workspace ? "ready" : "error",
    workspace,
    runtime_inspect: runtimeInspect,
    runtime_surface: workspaceResult.status === "fulfilled"
      ? agentRunWorkspaceResourceSurface(workspaceResult.value)
      : preserveCurrent ? current.runtime_surface : null,
    hook_runtime: preserveCurrent ? current.hook_runtime : null,
    frame: workspaceResult.status === "fulfilled"
      ? workspaceResult.value.current_frame ?? null
      : preserveCurrent ? current.frame : null,
    runtime_surface_error: preserveCurrent ? current.runtime_surface_error : null,
    workspace_error: workspaceError,
    runtime_inspect_error: runtimeInspectError,
    error: workspaceError ?? runtimeInspectError,
  };
}

export function useAgentRunWorkspaceState({
  runId,
  agentId,
  sourceKey,
}: UseAgentRunWorkspaceStateInput) {
  const [state, setState] = useState<AgentRunWorkspaceState>(() => emptyAgentRunWorkspaceState());
  const setAgent = useLifecycleStore((s) => s.setAgent);

  const loadWorkspaceState = useCallback(async (
    rid: string,
    aid: string,
    skey: string,
    canCommit: () => boolean = () => true,
    mode: AgentRunWorkspaceLoadMode = "replace",
  ): Promise<AgentRunProductView | null> => {
    await Promise.resolve();
    if (!canCommit()) return null;
    setState((current) => beginAgentRunWorkspaceStateLoad(current, rid, aid, skey, mode));

    const [workspaceResult, runtimeInspectResult] = await Promise.allSettled([
        fetchAgentRunWorkspace(rid, aid),
        fetchAgentRunRuntimeInspect({ runId: rid, agentId: aid }),
    ]);
    if (!canCommit()) {
      return workspaceResult.status === "fulfilled" ? workspaceResult.value : null;
    }
    const workspace = workspaceResult.status === "fulfilled" ? workspaceResult.value : null;

    if (workspace) {
      setAgent(workspace.agent);
    }
    setState((current) => settleAgentRunWorkspaceStateLoad(
      current,
      rid,
      aid,
      skey,
      mode,
      workspaceResult,
      runtimeInspectResult,
    ));
    return workspace;
  }, [setAgent]);

  useEffect(() => {
    if (!runId || !agentId || !sourceKey) return;
    let cancelled = false;
    const timeoutId = window.setTimeout(() => {
      void loadWorkspaceState(runId, agentId, sourceKey, () => !cancelled);
    }, 0);
    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [agentId, loadWorkspaceState, runId, sourceKey]);

  const refreshWorkspaceState = useCallback(async () => {
    if (!runId || !agentId || !sourceKey) return null;
    return loadWorkspaceState(runId, agentId, sourceKey, () => true, "refresh");
  }, [agentId, loadWorkspaceState, runId, sourceKey]);

  const activeState = useMemo(() => {
    if (!runId || !agentId || !sourceKey || !stateMatches(state, runId, agentId, sourceKey)) {
      return emptyAgentRunWorkspaceState();
    }
    return state;
  }, [agentId, runId, sourceKey, state]);

  return {
    state: activeState,
    refreshWorkspaceState,
    refreshHookRuntime: refreshWorkspaceState,
  };
}
