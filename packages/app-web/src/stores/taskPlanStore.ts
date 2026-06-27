import { create } from "zustand";

import type {
  CreateRunTaskRequest,
  RunTaskPlanResponse,
  Task,
  TaskPlanStatus,
  UpdateRunTaskRequest,
} from "../types";
import * as taskPlanService from "../services/taskPlan";
import type { RunTaskPlanQuery } from "../services/taskPlan";

interface TaskPlanState {
  taskPlansByRunId: Record<string, RunTaskPlanResponse>;
  isLoading: boolean;
  error: string | null;

  fetchRunTasks: (runId: string, query?: RunTaskPlanQuery) => Promise<RunTaskPlanResponse | null>;
  fetchAgentRunTasks: (
    runId: string,
    agentId: string,
    query?: RunTaskPlanQuery,
  ) => Promise<RunTaskPlanResponse | null>;
  createAgentRunTask: (
    runId: string,
    agentId: string,
    request: CreateRunTaskRequest,
  ) => Promise<Task | null>;
  updateTask: (runId: string, taskId: string, request: UpdateRunTaskRequest) => Promise<Task | null>;
  updateTaskStatus: (runId: string, taskId: string, status: TaskPlanStatus) => Promise<Task | null>;
  archiveTask: (runId: string, taskId: string) => Promise<Task | null>;
  upsertTask: (runId: string, task: Task) => void;
}

function upsertTaskInPlan(plan: RunTaskPlanResponse, task: Task): RunTaskPlanResponse {
  const exists = plan.tasks.some((item) => item.id === task.id);
  const tasks = exists
    ? plan.tasks.map((item) => (item.id === task.id ? task : item))
    : [task, ...plan.tasks];
  return { ...plan, tasks };
}

export const useTaskPlanStore = create<TaskPlanState>((set, get) => ({
  taskPlansByRunId: {},
  isLoading: false,
  error: null,

  fetchRunTasks: async (runId, query) => {
    set({ isLoading: true, error: null });
    try {
      const plan = await taskPlanService.fetchRunTasks(runId, query);
      set((state) => ({
        taskPlansByRunId: { ...state.taskPlansByRunId, [runId]: plan },
        isLoading: false,
      }));
      return plan;
    } catch (error) {
      set({ isLoading: false, error: (error as Error).message });
      return null;
    }
  },

  fetchAgentRunTasks: async (runId, agentId, query) => {
    set({ isLoading: true, error: null });
    try {
      const plan = await taskPlanService.fetchAgentRunTasks(runId, agentId, query);
      set((state) => ({
        taskPlansByRunId: { ...state.taskPlansByRunId, [runId]: plan },
        isLoading: false,
      }));
      return plan;
    } catch (error) {
      set({ isLoading: false, error: (error as Error).message });
      return null;
    }
  },

  createAgentRunTask: async (runId, agentId, request) => {
    try {
      const response = await taskPlanService.createAgentRunTask(runId, agentId, request);
      get().upsertTask(runId, response.task);
      return response.task;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  updateTask: async (runId, taskId, request) => {
    try {
      const response = await taskPlanService.updateRunTask(runId, taskId, request);
      get().upsertTask(runId, response.task);
      return response.task;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  updateTaskStatus: async (runId, taskId, status) => {
    try {
      const response = await taskPlanService.updateRunTaskStatus(runId, taskId, status);
      get().upsertTask(runId, response.task);
      return response.task;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  archiveTask: async (runId, taskId) => {
    try {
      const response = await taskPlanService.archiveRunTask(runId, taskId);
      get().upsertTask(runId, response.task);
      return response.task;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  upsertTask: (runId, task) => set((state) => {
    const current = state.taskPlansByRunId[runId] ?? {
      project_id: task.project_id,
      run_id: runId,
      tasks: [],
    };
    return {
      taskPlansByRunId: {
        ...state.taskPlansByRunId,
        [runId]: upsertTaskInPlan(current, task),
      },
    };
  }),
}));
