/**
 * Session Runtime State — 通过后端 `/sessions/{id}/runtime-control` 直接查询。
 *
 * 不再遍历 lifecycleStore 的 frame cache 做本地反查，
 * 而是让后端通过 RuntimeSessionExecutionAnchor 返回 Session control view。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  AgentFrameHookRuntimeInfo,
  AgentFrameRuntimeView,
  SessionRuntimeControlView,
} from "../../../types";
import type { SessionRuntimeStateStatus } from "../../workspace-runtime";
import { useLifecycleStore } from "../../../stores/lifecycleStore";
import { fetchSessionRuntimeControl } from "../../../services/lifecycle";
import { resolveVfsSurface } from "../../../services/vfs";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";

export type { SessionRuntimeStateStatus };

export interface SessionRuntimeProjectionState {
  session_id: string | null;
  source_key: string | null;
  status: SessionRuntimeStateStatus;
  runtime_surface: ResolvedVfsSurface | null;
  hook_runtime: AgentFrameHookRuntimeInfo | null;
  frame: AgentFrameRuntimeView | null;
  control: SessionRuntimeControlView | null;
  runtime_surface_error: string | null;
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
    runtime_surface: null,
    hook_runtime: null,
    frame: null,
    control: null,
    runtime_surface_error: null,
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
  const ingestLifecycleRun = useLifecycleStore((s) => s.ingestLifecycleRun);
  const setAgent = useLifecycleStore((s) => s.setAgent);
  const setFrame = useLifecycleStore((s) => s.setFrame);

  const loadRuntimeState = useCallback(async (
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
      runtime_surface: null,
      hook_runtime: null,
      frame: null,
      control: null,
      runtime_surface_error: null,
      error: null,
    });

    try {
      const [controlResult, runtimeSurfaceResult] = await Promise.allSettled([
        fetchSessionRuntimeControl(sid),
        resolveVfsSurface({ source_type: "session_runtime", session_id: sid }),
      ]);
      if (controlResult.status === "rejected") {
        throw controlResult.reason;
      }
      const control = controlResult.value;
      const runtimeSurface = runtimeSurfaceResult.status === "fulfilled"
        ? runtimeSurfaceResult.value
        : null;
      const runtimeSurfaceError = runtimeSurfaceResult.status === "rejected"
        ? errorMessage(runtimeSurfaceResult.reason)
        : null;
      if (!canCommit()) return;
      if (control.run) {
        ingestLifecycleRun(control.run);
      }
      if (control.agent) {
        setAgent(control.agent);
      }
      if (control.frame_runtime) {
        setFrame(control.frame_runtime);
      }
      setState({
        session_id: sid,
        source_key: skey,
        status: "ready",
        runtime_surface: runtimeSurface,
        hook_runtime: null,
        frame: control.frame_runtime ?? null,
        control,
        runtime_surface_error: runtimeSurfaceError,
        error: null,
      });
    } catch (error: unknown) {
      if (!canCommit()) return;
      // 404 表示 session 没有关联 AgentFrame，视为正常空状态
      const is404 = error instanceof Error && error.message.includes("404");
      setState({
        session_id: sid,
        source_key: skey,
        status: is404 ? "ready" : "error",
        runtime_surface: null,
        hook_runtime: null,
        frame: null,
        control: null,
        runtime_surface_error: null,
        error: is404 ? null : errorMessage(error),
      });
    }
  }, [ingestLifecycleRun, setAgent, setFrame]);

  useEffect(() => {
    if (!sessionId || !sourceKey) {
      return;
    }

    let cancelled = false;
    const timeoutId = window.setTimeout(() => {
      void loadRuntimeState(sessionId, sourceKey, () => !cancelled);
    }, 0);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [sessionId, sourceKey, loadRuntimeState]);

  const refreshRuntimeState = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    setState((current) => ({
      ...current,
      status: current.frame ? "refreshing" : "loading",
      error: null,
      runtime_surface_error: null,
    }));
    await loadRuntimeState(sessionId, sourceKey);
  }, [sessionId, sourceKey, loadRuntimeState]);

  const refreshHookRuntime = useCallback(async () => {
    if (!sessionId || !sourceKey) return;
    await loadRuntimeState(sessionId, sourceKey);
  }, [sessionId, sourceKey, loadRuntimeState]);

  const activeState = useMemo(() => {
    return selectActiveSessionRuntimeState(state, sessionId, sourceKey);
  }, [sessionId, sourceKey, state]);

  return {
    state: activeState,
    refreshRuntimeState,
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
