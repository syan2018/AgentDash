/**
 * Lifecycle 归一化 Store
 *
 * 以 run / graph_instance / subject / agent / frame 为索引主键，
 * 完整替代原有 runsBySessionId 的 session-first 索引模式。
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
  runs: Map<string, LifecycleRunView>;
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
  setRun: (run: LifecycleRunView) => void;
  setGraphInstance: (instance: WorkflowGraphInstanceView) => void;
  setAgent: (agent: LifecycleAgentView) => void;
  setFrame: (frame: AgentFrameRuntimeView) => void;
  setSubjectExecution: (view: SubjectExecutionView) => void;
  setRuntimeTrace: (trace: RuntimeSessionTraceView) => void;
  setSessionMeta: (meta: SessionMeta) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // ── bulk import（从 LifecycleRunView 展开子实体） ──
  ingestRun: (run: LifecycleRunView) => void;

  // ── API actions ──
  fetchAndIngestRun: (runId: string) => Promise<LifecycleRunView | null>;
  fetchProjectActiveAgents: (projectId: string) => Promise<void>;
  fetchSubjectExecution: (subjectKind: string, subjectId: string) => Promise<SubjectExecutionView | null>;
  fetchFrame: (frameId: string) => Promise<AgentFrameRuntimeView | null>;
  fetchRuntimeTrace: (runtimeSessionId: string) => Promise<RuntimeSessionTraceView | null>;
  /** 批量拉取 session meta 并缓存到 sessionMetas */
  hydrateSessionMetas: (sessionIds: string[]) => Promise<void>;

  // ── derived views ──
  /** 按 subject association 聚合：返回指定 subject 关联的所有 run */
  runsBySubject: (subjectKind: string, subjectId: string) => LifecycleRunView[];
  /** 返回指定 run 下的所有 agent */
  agentsByRun: (runId: string) => LifecycleAgentView[];
  /** 返回指定 run 的主 session id（第一个 runtime_trace_ref） */
  primarySessionId: (runId: string) => string | null;
}

// ─── Store ───────────────────────────────────────────────

export const useLifecycleStore = create<LifecycleState>((set, get) => ({
  runs: new Map(),
  graphInstances: new Map(),
  agents: new Map(),
  frames: new Map(),
  subjectExecutions: new Map(),
  runtimeTraces: new Map(),
  sessionMetas: new Map(),
  isLoading: false,
  error: null,

  setRun: (run) =>
    set((s) => {
      const next = new Map(s.runs);
      next.set(run.run_ref.run_id, run);
      return { runs: next };
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

  ingestRun: (run) =>
    set((s) => {
      const nextRuns = new Map(s.runs);
      nextRuns.set(run.run_ref.run_id, run);

      const nextGraphInstances = new Map(s.graphInstances);
      for (const gi of run.workflow_graph_instances) {
        nextGraphInstances.set(gi.id, gi);
      }

      const nextAgents = new Map(s.agents);
      for (const agent of run.agents) {
        nextAgents.set(agent.agent_ref.agent_id, agent);
      }

      return {
        runs: nextRuns,
        graphInstances: nextGraphInstances,
        agents: nextAgents,
      };
    }),

  // ── API actions ──

  fetchAndIngestRun: async (runId) => {
    set({ isLoading: true, error: null });
    try {
      const run = await fetchLifecycleRun(runId);
      get().ingestRun(run);
      set({ isLoading: false });
      return run;
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
      return null;
    }
  },

  fetchProjectActiveAgents: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const view = await fetchProjectActiveAgents(projectId);
      for (const run of view.runs) {
        get().ingestRun(run);
      }
      set({ isLoading: false });

      const sessionIds = view.runs.flatMap(
        (r) => r.runtime_trace_refs.map((ref) => ref.runtime_session_id),
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
      for (const run of view.runs) {
        get().ingestRun(run);
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
    const existing = get().sessionMetas;
    const missing = sessionIds.filter((id) => !existing.has(id));
    if (missing.length === 0) return;

    const results = await Promise.allSettled(
      missing.map((id) => fetchSessionMeta(id)),
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

  runsBySubject: (subjectKind, subjectId) => {
    const result: LifecycleRunView[] = [];
    for (const run of get().runs.values()) {
      const hasSubject = run.subject_associations.some(
        (sa) => sa.subject_ref.kind === subjectKind && sa.subject_ref.id === subjectId,
      );
      if (hasSubject) result.push(run);
    }
    return result;
  },

  agentsByRun: (runId) => {
    const result: LifecycleAgentView[] = [];
    for (const agent of get().agents.values()) {
      if (agent.agent_ref.run_id === runId) result.push(agent);
    }
    return result;
  },

  primarySessionId: (runId) => {
    const run = get().runs.get(runId);
    return run?.runtime_trace_refs[0]?.runtime_session_id ?? null;
  },
}));
