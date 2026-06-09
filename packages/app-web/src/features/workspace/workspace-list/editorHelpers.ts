import type {
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate,
} from "../../../types";
import type { WorkspaceBindingInput } from "../../../stores/workspaceStore";

export type CreateMode = "from_directory" | "logical";

export type Feedback = { tone: "success" | "info" | "error"; text: string };

export const CREATE_MODE_LABELS: Record<CreateMode, string> = {
  from_directory: "从可选目录创建",
  logical: "先建空壳，之后补运行位置",
};

export function bindingDraftKey(
  binding: Pick<WorkspaceBindingInput, "backend_id" | "root_ref">,
): string {
  const backendId = binding.backend_id.trim();
  const rootRef = binding.root_ref.trim().replaceAll("\\", "/").replace(/\/+$/, "");
  return `${backendId}:${rootRef}`;
}

export function candidateDraftKey(candidate: WorkspaceInventoryCandidate): string {
  return bindingDraftKey({
    backend_id: candidate.backend_id,
    root_ref: candidate.root_ref,
  });
}

export function candidateKey(candidate: WorkspaceInventoryCandidate): string {
  return `${candidate.backend_id}:${candidate.root_ref}`;
}

export function dedupeBindings(bindings: WorkspaceBindingInput[]): WorkspaceBindingInput[] {
  const seen = new Set<string>();
  return bindings.filter((binding) => {
    const key = bindingDraftKey(binding);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function normalizePayloadText(payload: Record<string, unknown>): string {
  return JSON.stringify(payload, null, 2);
}

export function emptyPayload(kind: WorkspaceIdentityKind): Record<string, unknown> {
  if (kind === "git_repo") return { repo_key: "", branch: "" };
  if (kind === "p4_workspace") return { server_address: "", stream: "", client_name: "", path_key: "" };
  return { path_key: "" };
}

export function updatePayloadField(
  payload: Record<string, unknown>,
  key: string,
  value: string,
): Record<string, unknown> {
  return { ...payload, [key]: value };
}

function stringField(payload: Record<string, unknown> | unknown, key: string): string {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return "";
  const record = payload as Record<string, unknown>;
  const value = record[key];
  return typeof value === "string" ? value.trim() : "";
}

export function detectionPrimaryText(result: WorkspaceDetectionResult): string {
  if (result.identity_kind === "git_repo") {
    return stringField(result.binding.detected_facts, "remote_url")
      || stringField(result.identity_payload, "remote_url")
      || stringField(result.identity_payload, "repo_url")
      || stringField(result.identity_payload, "repo_key")
      || result.binding.root_ref;
  }

  if (result.identity_kind === "p4_workspace") {
    const server = stringField(result.identity_payload, "server_address");
    const stream = stringField(result.identity_payload, "stream");
    const client = stringField(result.identity_payload, "client_name");
    return [server, stream || client].filter(Boolean).join(" · ") || result.binding.root_ref;
  }

  return stringField(result.identity_payload, "path_key") || result.binding.root_ref;
}
