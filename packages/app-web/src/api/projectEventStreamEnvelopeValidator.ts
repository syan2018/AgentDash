import type {
  ProjectEventStreamEnvelope,
} from "../generated/project-contracts";
import {
  parseGeneratedProjectEventStreamEnvelope,
  type GeneratedNdjsonEnvelopeParseResult,
  type GeneratedNdjsonEnvelopeValidationFailure,
  type GeneratedProjectEventStreamBackendRuntimeChangedEnvelope,
  type GeneratedProjectEventStreamConnectedEnvelope,
  type GeneratedProjectEventStreamHeartbeatEnvelope,
  type GeneratedProjectEventStreamStateChangedEnvelope,
} from "../generated/ndjson-stream-validators";

export type ProjectEventStreamConnectedEnvelope =
  GeneratedProjectEventStreamConnectedEnvelope;
export type ProjectEventStreamStateChangedEnvelope =
  GeneratedProjectEventStreamStateChangedEnvelope;
export type ProjectEventStreamBackendRuntimeChangedEnvelope =
  GeneratedProjectEventStreamBackendRuntimeChangedEnvelope;
export type ProjectEventStreamHeartbeatEnvelope =
  GeneratedProjectEventStreamHeartbeatEnvelope;

export type ProjectEventStreamEnvelopeParseResult =
  | Extract<GeneratedNdjsonEnvelopeParseResult<ProjectEventStreamEnvelope>, { ok: true }>
  | { ok: false; error: Error };

function projectEventStreamFailureToError(
  failure: GeneratedNdjsonEnvelopeValidationFailure,
): Error {
  switch (failure.reason) {
    case "not_object":
      return new Error("Project event stream envelope 必须是对象");
    case "unknown_type":
      return new Error(`未知 Project event stream 类型: ${failure.actual_type}`);
    case "invalid_branch":
      return new Error(
        `Project event stream ${failure.branch} ${failure.field_path} 必须是 ${failure.expected}`,
      );
  }
}

export function parseProjectEventStreamEnvelopeResult(
  value: unknown,
): ProjectEventStreamEnvelopeParseResult {
  const result = parseGeneratedProjectEventStreamEnvelope(value);
  if (result.ok) {
    return result;
  }
  return { ok: false, error: projectEventStreamFailureToError(result.failure) };
}

export function parseProjectEventStreamEnvelope(value: unknown): ProjectEventStreamEnvelope | null {
  const result = parseProjectEventStreamEnvelopeResult(value);
  return result.ok ? result.envelope : null;
}
