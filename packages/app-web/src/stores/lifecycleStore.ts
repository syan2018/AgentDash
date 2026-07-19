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
  AgentRunView,
  AgentFrameRuntimeView,
  SubjectExecutionView,
  OrchestrationInstanceView,
  RuntimeNodeView,
} from "../types";
import { subjectExecutionKey } from "../types";
import {
  fetchLifecycleRun,
  fetchSubjectExecution,
  fetchProjectActiveAgents,
  fetchAgentFrameRuntime,
} from "../services/lifecycle";

// ─── State Shape ─────────────────────────────────────────

interface LifecycleState {
  lifecycleRuns: Map<string, LifecycleRunView>;
  orchestrations: Map<string, OrchestrationInstanceView>;
  runtimeNodes: Map<string, RuntimeNodeView>;
  agents: Map<string, AgentRunView>;
  frames: Map<string, AgentFrameRuntimeView>;
  subjectExecutions: Map<string, SubjectExecutionView>;

  isLoading: boolean;
  error: string | null;

  // ── write actions ──
  setLifecycleRun: (lifecycleRun: LifecycleRunView) => void;
  setOrchestration: (orchestration: OrchestrationInstanceView) => void;
  setRuntimeNode: (orchestrationId: string, node: RuntimeNodeView) => void;
  setAgent: (agent: AgentRunView) => void;
  setFrame: (frame: AgentFrameRuntimeView) => void;
  setSubjectExecution: (view: SubjectExecutionView) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // ── bulk import（从 LifecycleRunView 展开子实体） ──
  ingestLifecycleRun: (lifecycleRun: LifecycleRunView) => void;

  // ── API actions ──
  fetchAndIngestLifecycleRun: (lifecycleRunId: string) => Promise<LifecycleRunView | null>;
  fetchProjectActiveAgents: (projectId: string) => Promise<void>;
  fetchSubjectExecution: (subjectKind: string, subjectId: string) => Promise<SubjectExecutionView | null>;
  fetchFrame: (frameId: string) => Promise<AgentFrameRuntimeView | null>;

  // ── derived views ──
  /** 按 subject association 聚合：返回指定 subject 关联的所有 LifecycleRun */
  lifecycleRunsBySubject: (subjectKind: string, subjectId: string) => LifecycleRunView[];
  /** 返回指定 LifecycleRun 下的所有 agent */
  agentsByLifecycleRun: (lifecycleRunId: string) => AgentRunView[];
}

// ─── Store ───────────────────────────────────────────────

export function runtimeNodeKey(orchestrationId: string, nodePath: string, attempt: number): string {
  return `${orchestrationId}:${nodePath}:${attempt}`;
}

function indexRuntimeNodes(
  map: Map<string, RuntimeNodeView>,
  orchestrationId: string,
  nodes: RuntimeNodeView[],
) {
  for (const node of nodes) {
    map.set(runtimeNodeKey(orchestrationId, node.node_path, node.attempt), node);
    indexRuntimeNodes(map, orchestrationId, node.children);
  }
}

export const useLifecycleStore = create<LifecycleState>((set, get) => ({
  lifecycleRuns: new Map(),
  orchestrations: new Map(),
  runtimeNodes: new Map(),
  agents: new Map(),
  frames: new Map(),
  subjectExecutions: new Map(),
  isLoading: false,
  error: null,

  setLifecycleRun: (lifecycleRun) =>
    set((s) => {
      const next = new Map(s.lifecycleRuns);
      next.set(lifecycleRun.run_ref.run_id, lifecycleRun);
      return { lifecycleRuns: next };
    }),

  setOrchestration: (orchestration) =>
    set((s) => {
      const next = new Map(s.orchestrations);
      next.set(orchestration.orchestration_id, orchestration);
      return { orchestrations: next };
    }),

  setRuntimeNode: (orchestrationId, node) =>
    set((s) => {
      const next = new Map(s.runtimeNodes);
      next.set(runtimeNodeKey(orchestrationId, node.node_path, node.attempt), node);
      return { runtimeNodes: next };
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

  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),

  ingestLifecycleRun: (lifecycleRun) =>
    set((s) => {
      const nextLifecycleRuns = new Map(s.lifecycleRuns);
      nextLifecycleRuns.set(lifecycleRun.run_ref.run_id, lifecycleRun);

      const nextOrchestrations = new Map(s.orchestrations);
      const nextRuntimeNodes = new Map(s.runtimeNodes);
      for (const orchestration of lifecycleRun.orchestrations) {
        nextOrchestrations.set(orchestration.orchestration_id, orchestration);
        indexRuntimeNodes(nextRuntimeNodes, orchestration.orchestration_id, orchestration.nodes);
      }

      const nextAgents = new Map(s.agents);
      for (const { agent } of lifecycleRun.agents) {
        nextAgents.set(agent.agent_ref.agent_id, agent);
      }

      return {
        lifecycleRuns: nextLifecycleRuns,
        orchestrations: nextOrchestrations,
        runtimeNodes: nextRuntimeNodes,
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
    const result: AgentRunView[] = [];
    for (const agent of get().agents.values()) {
      if (agent.agent_ref.run_id === lifecycleRunId) result.push(agent);
    }
    return result;
  },

}));
