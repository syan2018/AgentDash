import type {
  LayerState,
  RuntimeDiagnosticsBackendFact,
  RuntimeDiagnosticsCloudApiInput,
  RuntimeDiagnosticsRuntimeSummaryFact,
} from "@agentdash/core/local-runtime";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";
import type { EventConnectionState } from "../../../stores/eventStore";

interface CreateCloudApiDiagnosticsInput {
  apiError: string | null;
  isChecking: boolean;
  target: string | null;
  eventConnectionState: EventConnectionState;
}

export function createCloudApiDiagnosticsInput({
  apiError,
  isChecking,
  target,
  eventConnectionState,
}: CreateCloudApiDiagnosticsInput): RuntimeDiagnosticsCloudApiInput {
  return {
    state: cloudLayerState(apiError, isChecking, eventConnectionState),
    target,
    message: apiError,
    event_stream_state: eventConnectionState,
  };
}

export function backendDiagnosticsFacts(backends: BackendConfig[]): RuntimeDiagnosticsBackendFact[] {
  return backends.map((backend) => ({
    id: backend.id,
    name: backend.name,
    online: backend.online,
    backend_type: backend.backend_type,
    profile_id: backend.profile_id,
    machine_id: backend.machine_id,
    machine_label: backend.machine_label,
    share_scope_kind: backend.share_scope_kind,
    share_scope_id: backend.share_scope_id,
    capability_slot: backend.capability_slot,
    last_claimed_at: backend.last_claimed_at,
    registration_source: backend.registration_source,
    runtime_health: backend.runtime_health,
  }));
}

export function runtimeSummaryDiagnosticsFacts(
  summaries: BackendRuntimeSummary[],
): RuntimeDiagnosticsRuntimeSummaryFact[] {
  return summaries.map((summary) => ({
    backend_id: summary.backend_id,
    online: summary.online,
    allocatable: summary.allocatable,
    active_session_count: summary.active_session_count,
  }));
}

function cloudLayerState(
  apiError: string | null,
  isChecking: boolean,
  eventConnectionState: EventConnectionState,
): LayerState {
  if (apiError) return "unavailable";
  if (isChecking || eventConnectionState === "connecting" || eventConnectionState === "reconnecting") {
    return "checking";
  }
  if (eventConnectionState === "connected") return "healthy";
  return "unknown";
}
