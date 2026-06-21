import type {
  RoutineDispatchStrategyDto,
  RoutineTriggerConfigResponse,
} from "../generated/routine-contracts";

export type {
  RegenerateTokenResponse,
  RoutineCreationResponse,
  RoutineDispatchStrategyDto as RoutineDispatchStrategy,
  RoutineExecutionResponse as RoutineExecution,
  RoutineExecutionStatusDto as RoutineExecutionStatus,
  RoutineResponse as Routine,
  RoutineTriggerConfigResponse as RoutineTriggerConfig,
} from "../generated/routine-contracts";

export type RoutineTriggerType = RoutineTriggerConfigResponse["type"];
export type RoutineDispatchMode = RoutineDispatchStrategyDto["mode"];
