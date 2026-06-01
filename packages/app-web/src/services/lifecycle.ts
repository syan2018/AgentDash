/**
 * Lifecycle target view API。
 *
 * 这些返回值已经由 generated contracts 定义，service 层只负责 endpoint 调用。
 */

import { api } from "../api/client";
import type {
  AgentFrameRuntimeView,
  LifecycleRunView,
  RuntimeSessionTraceView,
  SubjectExecutionView,
} from "../types";

export async function fetchLifecycleRun(runId: string): Promise<LifecycleRunView> {
  return api.get<LifecycleRunView>(`/lifecycle-runs/${encodeURIComponent(runId)}/view`);
}

export async function fetchSubjectExecution(
  subjectKind: string,
  subjectId: string,
): Promise<SubjectExecutionView> {
  return api.get<SubjectExecutionView>(
    `/subjects/${encodeURIComponent(subjectKind)}/${encodeURIComponent(subjectId)}/execution`,
  );
}

export async function fetchAgentFrameRuntime(frameId: string): Promise<AgentFrameRuntimeView> {
  return api.get<AgentFrameRuntimeView>(`/agent-frames/${encodeURIComponent(frameId)}/runtime`);
}

export async function fetchRuntimeTrace(runtimeSessionId: string): Promise<RuntimeSessionTraceView> {
  return api.get<RuntimeSessionTraceView>(
    `/sessions/${encodeURIComponent(runtimeSessionId)}/trace`,
  );
}
