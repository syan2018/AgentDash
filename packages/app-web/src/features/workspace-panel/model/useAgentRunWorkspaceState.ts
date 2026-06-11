import { useCallback, useEffect, useMemo, useState } from "react";

import type { AgentFrameRuntimeView, AgentRunWorkspaceView } from "../../../types";
import type { AgentFrameHookRuntimeInfo } from "../../../types";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchAgentRunWorkspace } from "../../../services/lifecycle";
import { resolveVfsSurface } from "../../../services/vfs";
import type { SessionRuntimeStateStatus } from "../../workspace-runtime";

export interface AgentRunWorkspaceProjectionState {
  run_id: string | null;
  agent_id: string | null;
  source_key: string | null;
  status: SessionRuntimeStateStatus;
  workspace: AgentRunWorkspaceView | null;
  runtime_session_id: string | null;
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

export function emptyAgentRunWorkspaceState(): AgentRunWorkspaceProjectionState {
  return {
    run_id: null,
    agent_id: null,
    source_key: null,
    status: "idle",
    workspace: null,
    runtime_session_id: null,
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
  ) => {
    await Promise.resolve();
    if (!canCommit()) return;
    setState({
      ...emptyAgentRunWorkspaceState(),
      run_id: rid,
      agent_id: aid,
      source_key: skey,
      status: "loading",
    });

    try {
      const workspace = await fetchAgentRunWorkspace(rid, aid);
      const runtimeSessionId = workspace.delivery_runtime_ref?.runtime_session_id ?? null;
      const runtimeSurfaceResult = runtimeSessionId
        ? await Promise.allSettled([
            resolveVfsSurface({ source_type: "session_runtime", session_id: runtimeSessionId }),
          ])
        : [];
      const runtimeSurface = runtimeSurfaceResult[0]?.status === "fulfilled"
        ? runtimeSurfaceResult[0].value
        : null;
      const runtimeSurfaceError = runtimeSurfaceResult[0]?.status === "rejected"
        ? errorMessage(runtimeSurfaceResult[0].reason)
        : null;

      if (!canCommit()) return;
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
        runtime_session_id: runtimeSessionId,
        runtime_surface: runtimeSurface,
        hook_runtime: null,
        frame: workspace.frame_runtime ?? null,
        runtime_surface_error: runtimeSurfaceError,
        error: null,
      });
    } catch (error: unknown) {
      if (!canCommit()) return;
      setState({
        ...emptyAgentRunWorkspaceState(),
        run_id: rid,
        agent_id: aid,
        source_key: skey,
        status: "error",
        error: errorMessage(error),
      });
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
    if (!runId || !agentId || !sourceKey) return;
    setState((current) => ({
      ...current,
      status: current.frame ? "refreshing" : "loading",
      error: null,
      runtime_surface_error: null,
    }));
    await loadWorkspaceState(runId, agentId, sourceKey);
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

