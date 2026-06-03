/**
 * Session Runtime State — 通过后端 `/sessions/{id}/frame-runtime` 直接查询。
 *
 * 不再遍历 lifecycleStore 的 frame cache 做本地反查，
 * 而是让后端通过 `find_by_runtime_session` 精确锚定 frame 并返回 runtime view。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type { AgentFrameHookRuntimeInfo } from "../../../types";
import type { SessionRuntimeStateStatus } from "../../workspace-runtime";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchSessionFrameRuntime } from "../../../services/lifecycle";
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
  hook_runtime: AgentFrameHookRuntimeInfo | null;
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

export function useSessionRuntimeState({
  sessionId,
  sourceKey,
}: UseSessionRuntimeStateInput) {
  const [state, setState] = useState<SessionRuntimeProjectionState>(() => emptySessionRuntimeState());
  const setFrame = useLifecycleStore((s) => s.setFrame);

  const loadFrameContext = useCallback(async (
    sid: string,
    skey: string,
    canCommit: () => boolean = () => true,
  ) => {
    await Promise.resolve();
    if (!canCommit()) return;
    setState({
      session_id: sid,
      source_key: skey,
      status: "loading",
      context: null,
      hook_runtime: null,
      frame: null,
      error: null,
    });

    try {
      const frameView = await fetchSessionFrameRuntime(sid);
      if (!canCommit()) return;
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
      if (!canCommit()) return;
      // 404 表示 session 没有关联 AgentFrame（freeform session），视为正常空状态
      const is404 = error instanceof Error && error.message.includes("404");
      setState({
        session_id: sid,
        source_key: skey,
        status: is404 ? "ready" : "error",
        context: null,
        hook_runtime: null,
        frame: null,
        error: is404 ? null : errorMessage(error),
      });
    }
  }, [setFrame]);

  useEffect(() => {
    if (!sessionId || !sourceKey) {
      return;
    }

    let cancelled = false;
    const timeoutId = window.setTimeout(() => {
      void loadFrameContext(sessionId, sourceKey, () => !cancelled);
    }, 0);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
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
