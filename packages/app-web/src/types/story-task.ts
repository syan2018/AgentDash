import type { StoryResponse } from "../generated/story-contracts";
import type { TaskResponse } from "../generated/task-contracts";

export type Story = StoryResponse;
export type StoryContext = StoryResponse["context"];
export type Task = TaskResponse;

export type {
  StoryPriority,
  StoryStatus,
  StoryType,
} from "../generated/story-contracts";

export type {
  TaskStatus,
  TaskPlanStatus,
  TaskPriority,
  CreateRunTaskRequest,
  RunTaskCommandResponse,
  RunTaskPlanResponse,
  UpdateRunTaskRequest,
  UpdateRunTaskStatusRequest,
} from "../generated/task-contracts";

export type {
  StoryTaskProjectionItem,
  StoryTaskProjectionResponse,
  StoryTaskProjectionSource,
  StoryTaskProjectionSourceKind,
} from "../generated/story-contracts";
