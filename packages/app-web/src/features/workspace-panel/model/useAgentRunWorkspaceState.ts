import { useCallback, useEffect, useMemo, useState } from "react";

import type { AgentFrameRuntimeView, AgentRunWorkspaceView } from "../../../types";
import type { AgentFrameHookRuntimeInfo } from "../../../types";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchAgentRunWorkspace } from "../../../services/lifecycle";
import type { SessionRuntimeStateStatus } from "../../workspace-runtime";

export interface AgentRunWorkspaceProjectionState {
  run_id: string | null;
  agent_id: string | null;
  source_key: string | null;
  status: SessionRuntimeStateStatus;
  workspace: AgentRunWorkspaceView | null;
  delivery_trace_session_id: string | null;
  runtime_surface: ResolvedVfsSurface | null;
  hook_runtime: AgentFrameHookRuntimeInfo | null;
  frame: AgentFrameRuntimeView | null;
  runtime_surface_error: string | null;
  error: string | null;
}

interface UseAgentRunWorkspaceStateInput {
  runId: string | null;
  agentId: string | null;
  sourceKey: string | null;
}

type AgentRunWorkspaceLoadMode = "replace" | "refresh";

export function emptyAgentRunWorkspaceState(): AgentRunWorkspaceProjectionState {
  return {
    run_id: null,
    agent_id: null,
    source_key: null,
    status: "idle",
    workspace: null,
    delivery_trace_session_id: null,
    runtime_surface: null,
    hook_runtime: null,
    frame: null,
    runtime_surface_error: null,
    error: null,
  };
}

function stateMatches(
  state: AgentRunWorkspaceProjectionState,
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
  workspace: AgentRunWorkspaceView,
): ResolvedVfsSurface | null {
  return workspace.conversation?.resource_surface ?? workspace.resource_surface ?? null;
}

export function beginAgentRunWorkspaceStateLoad(
  current: AgentRunWorkspaceProjectionState,
  runId: string,
  agentId: string,
  sourceKey: string,
  mode: AgentRunWorkspaceLoadMode,
): AgentRunWorkspaceProjectionState {
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

export function failAgentRunWorkspaceStateLoad(
  current: AgentRunWorkspaceProjectionState,
  runId: string,
  agentId: string,
  sourceKey: string,
  mode: AgentRunWorkspaceLoadMode,
  message: string,
): AgentRunWorkspaceProjectionState {
  if (mode === "refresh" && stateMatches(current, runId, agentId, sourceKey)) {
    return {
      ...current,
      status: "error",
      error: message,
    };
  }

  return {
    ...emptyAgentRunWorkspaceState(),
    run_id: runId,
    agent_id: agentId,
    source_key: sourceKey,
    status: "error",
    error: message,
  };
}

export function useAgentRunWorkspaceState({
  runId,
  agentId,
  sourceKey,
}: UseAgentRunWorkspaceStateInput) {
  const [state, setState] = useState<AgentRunWorkspaceProjectionState>(() => emptyAgentRunWorkspaceState());
  const setAgent = useLifecycleStore((s) => s.setAgent);
  const setFrame = useLifecycleStore((s) => s.setFrame);

  const loadWorkspaceState = useCallback(async (
    rid: string,
    aid: string,
    skey: string,
    canCommit: () => boolean = () => true,
    mode: AgentRunWorkspaceLoadMode = "replace",
  ): Promise<AgentRunWorkspaceView | null> => {
    await Promise.resolve();
    if (!canCommit()) return null;
    setState((current) => beginAgentRunWorkspaceStateLoad(current, rid, aid, skey, mode));

    try {
      const workspace = await fetchAgentRunWorkspace(rid, aid);
      const deliveryTraceSessionId = workspace.delivery_trace_meta?.runtime_session_ref.runtime_session_id ?? null;
      const runtimeSurface = agentRunWorkspaceResourceSurface(workspace);

      if (!canCommit()) return workspace;
      if (workspace.agent) {
        setAgent(workspace.agent);
      }
      if (workspace.frame_runtime) {
        setFrame(workspace.frame_runtime);
      }
      setState({
        run_id: rid,
        agent_id: aid,
        source_key: skey,
        status: "ready",
        workspace,
        delivery_trace_session_id: deliveryTraceSessionId,
        runtime_surface: runtimeSurface,
        hook_runtime: null,
        frame: workspace.frame_runtime ?? null,
        runtime_surface_error: null,
        error: null,
      });
      return workspace;
    } catch (error: unknown) {
      if (!canCommit()) return null;
      const message = errorMessage(error);
      setState((current) => failAgentRunWorkspaceStateLoad(current, rid, aid, skey, mode, message));
      return null;
    }
  }, [setAgent, setFrame]);

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
