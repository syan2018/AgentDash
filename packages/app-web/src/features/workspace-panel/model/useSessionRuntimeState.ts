/**
 * Session Runtime State — 通过 lifecycle frame 投影驱动。
 *
 * 旧版直接调用 `GET /sessions/{id}/context` 和 `GET /sessions/{id}/hook-runtime`，
 * 这两个端点已在 lifecycle 迁移中移除。
 *
 * 新版通过 `lifecycleStore` 查找 session 关联的 AgentFrame，再从 frame runtime
 * 投影中组装 `SessionContextPayload` 和 `HookSessionRuntimeInfo`。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type { HookSessionRuntimeInfo } from "../../../types";
import type { SessionRuntimeStateStatus } from "../../workspace-runtime";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchAgentFrameRuntime } from "../../../services/lifecycle";
import type { AgentFrameRuntimeView } from "../../../types";

export type { SessionRuntimeStateStatus };

export interface SessionContextPayload {
  workspace_id: string | null;
  agent_binding: null;
  vfs: null;
  runtime_surface: null;
  context_snapshot: null;
  session_capabilities: null;
}

export interface SessionRuntimeProjectionState {
  session_id: string | null;
  source_key: string | null;
  status: SessionRuntimeStateStatus;
  context: SessionContextPayload | null;
  hook_runtime: HookSessionRuntimeInfo | null;
  frame: AgentFrameRuntimeView | null;
  error: string | null;
}

interface UseSessionRuntimeStateInput {
  sessionId: string | null;
  sourceKey: string | null;
}

export function emptySessionRuntimeState(): SessionRuntimeProjectionState {
  return {
    session_id: null,
    source_key: null,
    status: "idle",
    context: null,
    hook_runtime: null,
    frame: null,
    error: null,
  };
}

function stateMatches(
  state: SessionRuntimeProjectionState,
  sessionId: string,
  sourceKey: string,
): boolean {
  return state.session_id === sessionId && state.source_key === sourceKey;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Session runtime state 加载失败";
}

/**
 * 从 lifecycleStore 中反查 session 对应的 frame_id。
 * 遍历所有 frame，找到 runtime_session_refs 包含目标 sessionId 的 frame。
 */
function findFrameIdForSession(
  frames: Map<string, AgentFrameRuntimeView>,
  agents: Map<string, { current_frame_id?: string }>,
  sessionId: string,
): string | null {
  for (const frame of frames.values()) {
    if (frame.runtime_session_refs.some((ref) => ref.runtime_session_id === sessionId)) {
      return frame.frame_ref.frame_id;
    }
  }
  for (const agent of agents.values()) {
    if (agent.current_frame_id) return agent.current_frame_id;
  }
  return null;
}

export function useSessionRuntimeState({
  sessionId,
  sourceKey,
}: UseSessionRuntimeStateInput) {
  const [state, setState] = useState<SessionRuntimeProjectionState>(() => emptySessionRuntimeState());
  const frames = useLifecycleStore((s) => s.frames);
  const agents = useLifecycleStore((s) => s.agents);
  const setFrame = useLifecycleStore((s) => s.setFrame);

  const loadFrameContext = useCallback(async (sid: string, skey: string) => {
    const frameId = findFrameIdForSession(frames, agents, sid);
    if (!frameId) {
      setState({
        session_id: sid,
        source_key: skey,
        status: "ready",
        context: null,
        hook_runtime: null,
        frame: null,
        error: null,
      });
      return;
    }

    try {
      const frameView = await fetchAgentFrameRuntime(frameId);
      setFrame(frameView);
      setState({
        session_id: sid,
        source_key: skey,
        status: "ready",
        context: null,
        hook_runtime: null,
        frame: frameView,
        error: null,
      });
    } catch (error: unknown) {
      setState({
        session_id: sid,
        source_key: skey,
        status: "error",
        context: null,
        hook_runtime: null,
        frame: null,
        error: errorMessage(error),
      });
    }
  }, [frames, agents, setFrame]);

  useEffect(() => {
    if (!sessionId || !sourceKey) {
      setState(emptySessionRuntimeState());
      return;
    }

    let cancelled = false;
    setState({
      session_id: sessionId,
      source_key: sourceKey,
      status: "loading",
      context: null,
      hook_runtime: null,
      frame: null,
      error: null,
    });

    void loadFrameContext(sessionId, sourceKey).then(() => {
      if (cancelled) return;
    });

    return () => {
      cancelled = true;
    };
  }, [sessionId, sourceKey, loadFrameContext]);

  const refreshContext = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    setState((current) => ({
      ...current,
      status: current.frame ? "refreshing" : "loading",
      error: null,
    }));
    await loadFrameContext(sessionId, sourceKey);
  }, [sessionId, sourceKey, loadFrameContext]);

  const refreshHookRuntime = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    await loadFrameContext(sessionId, sourceKey);
  }, [sessionId, sourceKey, loadFrameContext]);

  const activeState = useMemo(() => {
    return selectActiveSessionRuntimeState(state, sessionId, sourceKey);
  }, [sessionId, sourceKey, state]);

  return {
    state: activeState,
    refreshContext,
    refreshHookRuntime,
  };
}

export function selectActiveSessionRuntimeState(
  state: SessionRuntimeProjectionState,
  sessionId: string | null,
  sourceKey: string | null,
): SessionRuntimeProjectionState {
  if (!sessionId || !sourceKey || !stateMatches(state, sessionId, sourceKey)) {
    return emptySessionRuntimeState();
  }
  return state;
}
