import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  createRoutine,
  deleteRoutine,
  fetchProjectRoutines,
  fetchRoutineExecutions,
  regenerateRoutineToken,
  setRoutineEnabled,
  updateRoutine,
  type CreateRoutinePayload,
  type UpdateRoutinePayload,
} from "../../../services/routine";

export const projectRoutinesKey = (projectId: string | null) =>
  ["project-routines", projectId] as const;

export const routineExecutionsKey = (routineId: string) =>
  ["routine-executions", routineId] as const;

const EXECUTION_PAGE_SIZE = 20;

export function useProjectRoutinesQuery(projectId: string | null) {
  return useQuery({
    queryKey: projectRoutinesKey(projectId),
    queryFn: () => fetchProjectRoutines(projectId as string),
    enabled: Boolean(projectId),
    placeholderData: undefined,
  });
}

export function useCreateRoutineMutation(projectId: string | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateRoutinePayload) => {
      if (!projectId) {
        throw new Error("创建 Routine 需要先选择 Project");
      }
      return createRoutine(projectId, payload);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: projectRoutinesKey(projectId) });
    },
  });
}

export function useUpdateRoutineMutation(projectId: string | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateRoutinePayload }) =>
      updateRoutine(id, payload),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: projectRoutinesKey(projectId) });
    },
  });
}

export function useDeleteRoutineMutation(projectId: string | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteRoutine(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: projectRoutinesKey(projectId) });
    },
  });
}

export function useSetRoutineEnabledMutation(projectId: string | null) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      setRoutineEnabled(id, enabled),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: projectRoutinesKey(projectId) });
    },
  });
}

export function useRegenerateRoutineTokenMutation() {
  return useMutation({
    mutationFn: (id: string) => regenerateRoutineToken(id),
  });
}

export function useRoutineExecutionsQuery(routineId: string) {
  return useInfiniteQuery({
    queryKey: routineExecutionsKey(routineId),
    queryFn: ({ pageParam }) =>
      fetchRoutineExecutions(routineId, EXECUTION_PAGE_SIZE, pageParam),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) =>
      lastPage.length < EXECUTION_PAGE_SIZE
        ? undefined
        : allPages.reduce((count, page) => count + page.length, 0),
  });
}
