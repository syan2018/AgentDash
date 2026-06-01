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
  fetchAgentFrameRuntime,
} from "../services/lifecycle";

// ─── State Shape ─────────────────────────────────────────

interface LifecycleState {
  runs: Map<string, LifecycleRunView>;
  graphInstances: Map<string, WorkflowGraphInstanceView>;
  agents: Map<string, LifecycleAgentView>;
  frames: Map<string, AgentFrameRuntimeView>;
  subjectExecutions: Map<string, SubjectExecutionView>;
  runtimeTraces: Map<string, RuntimeSessionTraceView>;

  isLoading: boolean;
  error: string | null;

  // ── write actions ──
  setRun: (run: LifecycleRunView) => void;
  setGraphInstance: (instance: WorkflowGraphInstanceView) => void;
  setAgent: (agent: LifecycleAgentView) => void;
  setFrame: (frame: AgentFrameRuntimeView) => void;
  setSubjectExecution: (view: SubjectExecutionView) => void;
  setRuntimeTrace: (trace: RuntimeSessionTraceView) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // ── bulk import（从 LifecycleRunView 展开子实体） ──
  ingestRun: (run: LifecycleRunView) => void;

  // ── API actions ──
  fetchAndIngestRun: (runId: string) => Promise<LifecycleRunView | null>;
  fetchSubjectExecution: (subjectKind: string, subjectId: string) => Promise<SubjectExecutionView | null>;
  fetchFrame: (frameId: string) => Promise<AgentFrameRuntimeView | null>;

  // ── derived views ──
  /** 按 subject association 聚合：返回指定 subject 关联的所有 run */
  runsBySubject: (subjectKind: string, subjectId: string) => LifecycleRunView[];
  /** 返回指定 run 下的所有 agent */
  agentsByRun: (runId: string) => LifecycleAgentView[];
}

// ─── Store ───────────────────────────────────────────────

export const useLifecycleStore = create<LifecycleState>((set, get) => ({
  runs: new Map(),
  graphInstances: new Map(),
  agents: new Map(),
  frames: new Map(),
  subjectExecutions: new Map(),
  runtimeTraces: new Map(),
  isLoading: false,
  error: null,

  setRun: (run) =>
    set((s) => {
      const next = new Map(s.runs);
      next.set(run.id, run);
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
      next.set(agent.id, agent);
      return { agents: next };
    }),

  setFrame: (frame) =>
    set((s) => {
      const next = new Map(s.frames);
      next.set(frame.id, frame);
      return { frames: next };
    }),

  setSubjectExecution: (view) =>
    set((s) => {
      const key = subjectExecutionKey(view.subject_kind, view.subject_id);
      const next = new Map(s.subjectExecutions);
      next.set(key, view);
      return { subjectExecutions: next };
    }),

  setRuntimeTrace: (trace) =>
    set((s) => {
      const next = new Map(s.runtimeTraces);
      next.set(trace.id, trace);
      return { runtimeTraces: next };
    }),

  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),

  ingestRun: (run) =>
    set((s) => {
      const nextRuns = new Map(s.runs);
      nextRuns.set(run.id, run);

      const nextGraphInstances = new Map(s.graphInstances);
      for (const gi of run.workflow_graph_instances) {
        nextGraphInstances.set(gi.id, gi);
      }

      const nextAgents = new Map(s.agents);
      for (const agent of run.agents) {
        nextAgents.set(agent.id, agent);
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

  fetchSubjectExecution: async (subjectKind, subjectId) => {
    try {
      const view = await fetchSubjectExecution(subjectKind, subjectId);
      get().setSubjectExecution(view);
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

  // ── derived views ──

  runsBySubject: (subjectKind, subjectId) => {
    const result: LifecycleRunView[] = [];
    for (const run of get().runs.values()) {
      const hasSubject = run.subject_associations.some(
        (sa) => sa.subject_kind === subjectKind && sa.subject_id === subjectId,
      );
      if (hasSubject) result.push(run);
    }
    return result;
  },

  agentsByRun: (runId) => {
    const result: LifecycleAgentView[] = [];
    for (const agent of get().agents.values()) {
      if (agent.run_id === runId) result.push(agent);
    }
    return result;
  },
}));
