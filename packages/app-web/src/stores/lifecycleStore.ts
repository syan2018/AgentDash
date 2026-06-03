/**
 * Lifecycle 归一化 Store
 *
 * 以 lifecycle_run / graph_instance / subject / agent / frame 为索引主键，
 * 完整替代原有 session-first 索引模式。
 *
 * SubjectExecution 使用 `subject_kind:subject_id` 复合 key 索引。
 */

import { create } from "zustand";

import type {
  LifecycleRunView,
  WorkflowGraphInstanceView,
  LifecycleAgentView,
  AgentFrameRuntimeView,
  SubjectExecutionView,
  RuntimeSessionTraceView,
} from "../types";
import { subjectExecutionKey } from "../types";
import {
  fetchLifecycleRun,
  fetchSubjectExecution,
  fetchProjectActiveAgents,
  fetchAgentFrameRuntime,
  fetchRuntimeTrace,
} from "../services/lifecycle";
import { fetchSessionMeta, type SessionMeta } from "../services/session";

// ─── State Shape ─────────────────────────────────────────

interface LifecycleState {
  lifecycleRuns: Map<string, LifecycleRunView>;
  graphInstances: Map<string, WorkflowGraphInstanceView>;
  agents: Map<string, LifecycleAgentView>;
  frames: Map<string, AgentFrameRuntimeView>;
  subjectExecutions: Map<string, SubjectExecutionView>;
  runtimeTraces: Map<string, RuntimeSessionTraceView>;
  /** runtime_session_id → SessionMeta 缓存，用于列表显示 session title */
  sessionMetas: Map<string, SessionMeta>;

  isLoading: boolean;
  error: string | null;

  // ── write actions ──
  setLifecycleRun: (lifecycleRun: LifecycleRunView) => void;
  setGraphInstance: (instance: WorkflowGraphInstanceView) => void;
  setAgent: (agent: LifecycleAgentView) => void;
  setFrame: (frame: AgentFrameRuntimeView) => void;
  setSubjectExecution: (view: SubjectExecutionView) => void;
  setRuntimeTrace: (trace: RuntimeSessionTraceView) => void;
  setSessionMeta: (meta: SessionMeta) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // ── bulk import（从 LifecycleRunView 展开子实体） ──
  ingestLifecycleRun: (lifecycleRun: LifecycleRunView) => void;

  // ── API actions ──
  fetchAndIngestLifecycleRun: (lifecycleRunId: string) => Promise<LifecycleRunView | null>;
  fetchProjectActiveAgents: (projectId: string) => Promise<void>;
  fetchSubjectExecution: (subjectKind: string, subjectId: string) => Promise<SubjectExecutionView | null>;
  fetchFrame: (frameId: string) => Promise<AgentFrameRuntimeView | null>;
  fetchRuntimeTrace: (runtimeSessionId: string) => Promise<RuntimeSessionTraceView | null>;
  /** 批量拉取 session meta 并缓存到 sessionMetas */
  hydrateSessionMetas: (sessionIds: string[]) => Promise<void>;

  // ── derived views ──
  /** 按 subject association 聚合：返回指定 subject 关联的所有 LifecycleRun */
  lifecycleRunsBySubject: (subjectKind: string, subjectId: string) => LifecycleRunView[];
  /** 返回指定 LifecycleRun 下的所有 agent */
  agentsByLifecycleRun: (lifecycleRunId: string) => LifecycleAgentView[];
  /** 返回指定 LifecycleRun 当前 agent/frame delivery runtime session id */
  deliveryRuntimeSessionIdForLifecycleRun: (lifecycleRunId: string) => string | null;
}

// ─── Store ───────────────────────────────────────────────

export const useLifecycleStore = create<LifecycleState>((set, get) => ({
  lifecycleRuns: new Map(),
  graphInstances: new Map(),
  agents: new Map(),
  frames: new Map(),
  subjectExecutions: new Map(),
  runtimeTraces: new Map(),
  sessionMetas: new Map(),
  isLoading: false,
  error: null,

  setLifecycleRun: (lifecycleRun) =>
    set((s) => {
      const next = new Map(s.lifecycleRuns);
      next.set(lifecycleRun.run_ref.run_id, lifecycleRun);
      return { lifecycleRuns: next };
    }),

  setGraphInstance: (instance) =>
    set((s) => {
      const next = new Map(s.graphInstances);
      next.set(instance.id, instance);
      return { graphInstances: next };
    }),

  setAgent: (agent) =>
    set((s) => {
      const next = new Map(s.agents);
      next.set(agent.agent_ref.agent_id, agent);
      return { agents: next };
    }),

  setFrame: (frame) =>
    set((s) => {
      const next = new Map(s.frames);
      next.set(frame.frame_ref.frame_id, frame);
      return { frames: next };
    }),

  setSubjectExecution: (view) =>
    set((s) => {
      const key = subjectExecutionKey(view.subject_ref.kind, view.subject_ref.id);
      const next = new Map(s.subjectExecutions);
      next.set(key, view);
      return { subjectExecutions: next };
    }),

  setRuntimeTrace: (trace) =>
    set((s) => {
      const next = new Map(s.runtimeTraces);
      next.set(trace.runtime_session_ref.runtime_session_id, trace);
      return { runtimeTraces: next };
    }),

  setSessionMeta: (meta) =>
    set((s) => {
      const next = new Map(s.sessionMetas);
      next.set(meta.id, meta);
      return { sessionMetas: next };
    }),

  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),

  ingestLifecycleRun: (lifecycleRun) =>
    set((s) => {
      const nextLifecycleRuns = new Map(s.lifecycleRuns);
      nextLifecycleRuns.set(lifecycleRun.run_ref.run_id, lifecycleRun);

      const nextGraphInstances = new Map(s.graphInstances);
      for (const gi of lifecycleRun.workflow_graph_instances) {
        nextGraphInstances.set(gi.id, gi);
      }

      const nextAgents = new Map(s.agents);
      for (const agent of lifecycleRun.agents) {
        nextAgents.set(agent.agent_ref.agent_id, agent);
      }

      return {
        lifecycleRuns: nextLifecycleRuns,
        graphInstances: nextGraphInstances,
        agents: nextAgents,
      };
    }),

  // ── API actions ──

  fetchAndIngestLifecycleRun: async (lifecycleRunId) => {
    set({ isLoading: true, error: null });
    try {
      const lifecycleRun = await fetchLifecycleRun(lifecycleRunId);
      get().ingestLifecycleRun(lifecycleRun);
      set({ isLoading: false });
      return lifecycleRun;
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
      return null;
    }
  },

  fetchProjectActiveAgents: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const view = await fetchProjectActiveAgents(projectId);
      for (const lifecycleRun of view.runs) {
        get().ingestLifecycleRun(lifecycleRun);
      }
      set({ isLoading: false });

      const sessionIds = view.runs.flatMap((r) =>
        r.agents.flatMap((agent) =>
          agent.delivery_runtime_ref ? [agent.delivery_runtime_ref.runtime_session_id] : [],
        ),
      );
      if (sessionIds.length > 0) {
        void get().hydrateSessionMetas(sessionIds);
      }
    } catch (e) {
      set({ isLoading: false, error: (e as Error).message });
    }
  },

  fetchSubjectExecution: async (subjectKind, subjectId) => {
    try {
      const view = await fetchSubjectExecution(subjectKind, subjectId);
      get().setSubjectExecution(view);
      for (const lifecycleRun of view.runs) {
        get().ingestLifecycleRun(lifecycleRun);
      }
      return view;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchFrame: async (frameId) => {
    try {
      const frame = await fetchAgentFrameRuntime(frameId);
      get().setFrame(frame);
      return frame;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchRuntimeTrace: async (runtimeSessionId) => {
    try {
      const trace = await fetchRuntimeTrace(runtimeSessionId);
      get().setRuntimeTrace(trace);
      return trace;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  hydrateSessionMetas: async (sessionIds) => {
    const uniqueIds = Array.from(new Set(sessionIds));
    if (uniqueIds.length === 0) return;

    const results = await Promise.allSettled(
      uniqueIds.map((id) => fetchSessionMeta(id)),
    );
    const nextMetas = new Map(get().sessionMetas);
    for (const result of results) {
      if (result.status === "fulfilled") {
        nextMetas.set(result.value.id, result.value);
      }
    }
    set({ sessionMetas: nextMetas });
  },

  // ── derived views ──

  lifecycleRunsBySubject: (subjectKind, subjectId) => {
    const result: LifecycleRunView[] = [];
    for (const lifecycleRun of get().lifecycleRuns.values()) {
      const hasSubject = lifecycleRun.subject_associations.some(
        (sa) => sa.subject_ref.kind === subjectKind && sa.subject_ref.id === subjectId,
      );
      if (hasSubject) result.push(lifecycleRun);
    }
    return result;
  },

  agentsByLifecycleRun: (lifecycleRunId) => {
    const result: LifecycleAgentView[] = [];
    for (const agent of get().agents.values()) {
      if (agent.agent_ref.run_id === lifecycleRunId) result.push(agent);
    }
    return result;
  },

  deliveryRuntimeSessionIdForLifecycleRun: (lifecycleRunId) => {
    for (const agent of get().agents.values()) {
      if (agent.agent_ref.run_id === lifecycleRunId && agent.delivery_runtime_ref) {
        return agent.delivery_runtime_ref.runtime_session_id;
      }
    }
    return null;
  },
}));
