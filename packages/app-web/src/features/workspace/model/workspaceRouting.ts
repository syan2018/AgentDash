import type {
  BackendConfig,
  BackendWorkspaceInventoryStatus,
  ProjectBackendAccess,
  Workspace,
  WorkspaceBinding,
  WorkspaceBindingStatus,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate,
} from "../../../types";
import type { WorkspaceBindingInput } from "../../../stores/workspaceStore";
import { BINDING_STATUS_LABELS } from "./workspaceTerms";

function bindingStatusText(status: WorkspaceBindingStatus): string {
  return BINDING_STATUS_LABELS[status] ?? status;
}

export interface WorkspaceResolutionSummary {
  state: "resolved" | "warning" | "blocked";
  label: string;
  description: string;
  binding: WorkspaceBinding | null;
  warnings: string[];
}

export interface WorkspaceAvailabilitySummary {
  total: number;
  ready: number;
  online: number;
  authorized: number;
}

export interface WorkspaceDraft {
  name: string;
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  binding: WorkspaceBindingInput;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(record: Record<string, unknown>, key: string): string | null {
  const value = record[key];
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function readNestedString(record: Record<string, unknown>, parent: string, key: string): string | null {
  const value = record[parent];
  return isRecord(value) ? readString(value, key) : null;
}

function backendIsOnline(backend: BackendConfig | undefined): boolean {
  return backend?.online === true;
}

function activeAccessIds(accesses: ProjectBackendAccess[]): Set<string> {
  return new Set(
    accesses
      .filter((access) => access.status === "active")
      .map((access) => access.backend_id),
  );
}

function findBackend(backends: BackendConfig[], backendId: string): BackendConfig | undefined {
  return backends.find((backend) => backend.id === backendId);
}

export function backendDisplayName(backends: BackendConfig[], backendId: string): string {
  return findBackend(backends, backendId)?.name ?? backendId;
}

export function localAuthorizedBackends(
  backends: BackendConfig[],
  accesses: ProjectBackendAccess[],
): BackendConfig[] {
  const authorized = activeAccessIds(accesses);
  return backends.filter((backend) => (
    backend.backend_type === "local"
    && backend.online === true
    && authorized.has(backend.id)
  ));
}

export function authorizedBackends(
  backends: BackendConfig[],
  accesses: ProjectBackendAccess[],
): BackendConfig[] {
  const authorized = activeAccessIds(accesses);
  return backends.filter((backend) => authorized.has(backend.id));
}

export function identitySummary(
  kind: WorkspaceIdentityKind,
  payload: Record<string, unknown> | unknown,
): string {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return "(empty)";
  const p = payload as Record<string, unknown>;
  if (kind === "git_repo") {
    const repo = readString(p, "repo_key")
      ?? readString(p, "remote_url")
      ?? readString(p, "repo_root")
      ?? readString(p, "root_hint")
      ?? readNestedString(p, "hints", "root_hint");
    const branch = readString(p, "branch")
      ?? readString(p, "current_branch")
      ?? readNestedString(p, "hints", "current_branch");
    return [repo ?? "未填写仓库地址", branch].filter(Boolean).join(" · ");
  }

  if (kind === "p4_workspace") {
    const stream = readString(p, "stream");
    const client = readString(p, "client_name");
    const server = readString(p, "server_address");
    const pathKey = readString(p, "path_key")
      ?? readString(p, "workspace_root")
      ?? readString(p, "root_hint")
      ?? readNestedString(p, "hints", "root_hint");
    return [server, stream ?? client ?? pathKey ?? "未填写 P4 工作空间"].filter(Boolean).join(" · ");
  }

  return readString(p, "path_key")
    ?? readString(p, "root_hint")
    ?? readNestedString(p, "hints", "root_hint")
    ?? "未填写本地目录";
}

export function detectedFactsSummary(binding: WorkspaceBinding | null): string | null {
  if (!binding || !isRecord(binding.detected_facts)) return null;
  const git = binding.detected_facts.git;
  if (isRecord(git) && git.is_repo === true) {
    const branch = readString(git, "branch") ?? "HEAD";
    const repo = readString(git, "source_repo") ?? binding.root_ref;
    return `Git ${branch} · ${repo}`;
  }
  const p4 = binding.detected_facts.p4;
  if (isRecord(p4)) {
    return readString(p4, "stream")
      ?? readString(p4, "client_name")
      ?? readString(p4, "workspace_root");
  }
  return null;
}

export function buildDefaultWorkspaceName(
  identityKind: WorkspaceIdentityKind,
  rootRef: string,
): string {
  const segments = rootRef.replaceAll("\\", "/").split("/").filter(Boolean);
  const tail = segments.at(-1) ?? "workspace";
  if (identityKind === "git_repo") return tail;
  if (identityKind === "p4_workspace") return `${tail}-p4`;
  return tail;
}

export function bindingToInput(binding: WorkspaceBinding): WorkspaceBindingInput {
  return {
    id: binding.id,
    backend_id: binding.backend_id,
    root_ref: binding.root_ref,
    status: binding.status,
    detected_facts: binding.detected_facts as Record<string, unknown> | undefined,
    priority: binding.priority,
  };
}

export function candidateToBindingInput(candidate: WorkspaceInventoryCandidate): WorkspaceBindingInput {
  return {
    id: crypto.randomUUID(),
    backend_id: candidate.backend_id,
    root_ref: candidate.root_ref,
    status: inventoryStatusToBindingStatus(candidate.status),
    detected_facts: candidate.detected_facts,
    priority: 0,
  };
}

function inventoryStatusToBindingStatus(status: BackendWorkspaceInventoryStatus): WorkspaceBindingStatus {
  if (status === "available") return "ready";
  if (status === "offline") return "offline";
  return "error";
}

export function candidateToDraft(candidate: WorkspaceInventoryCandidate): WorkspaceDraft {
  return {
    name: buildDefaultWorkspaceName(candidate.identity_kind, candidate.root_ref),
    identity_kind: candidate.identity_kind,
    identity_payload: candidate.identity_payload,
    binding: candidateToBindingInput(candidate),
  };
}

export function summarizeAvailability(
  workspace: Workspace,
  backends: BackendConfig[],
  accesses: ProjectBackendAccess[],
): WorkspaceAvailabilitySummary {
  const authorized = activeAccessIds(accesses);
  return workspace.bindings.reduce<WorkspaceAvailabilitySummary>(
    (summary, binding) => {
      const backend = findBackend(backends, binding.backend_id);
      const isAuthorized = authorized.has(binding.backend_id);
      const isReady = binding.status === "ready";
      return {
        total: summary.total + 1,
        ready: summary.ready + (isReady ? 1 : 0),
        online: summary.online + (isReady && isAuthorized && backendIsOnline(backend) ? 1 : 0),
        authorized: summary.authorized + (isAuthorized ? 1 : 0),
      };
    },
    { total: 0, ready: 0, online: 0, authorized: 0 },
  );
}

export function summarizeResolution(
  workspace: Workspace,
  backends: BackendConfig[],
  accesses: ProjectBackendAccess[],
): WorkspaceResolutionSummary {
  if (workspace.bindings.length === 0) {
    return {
      state: "blocked",
      label: "等待运行位置",
      description: "目前只有代码来源，还没有匹配到可运行的 Backend / 目录。",
      binding: null,
      warnings: ["还没有任何运行位置"],
    };
  }

  const authorized = activeAccessIds(accesses);
  const warnings: string[] = [];
  const candidates = workspace.bindings
    .map((binding) => {
      const backend = findBackend(backends, binding.backend_id);
      const isAuthorized = authorized.has(binding.backend_id);
      const isOnline = backendIsOnline(backend);
      const backendLabel = backendDisplayName(backends, binding.backend_id);
      if (!isAuthorized) {
        warnings.push(`Backend「${backendLabel}」尚未授权给当前 Project`);
      } else if (!isOnline) {
        warnings.push(`Backend「${backendLabel}」当前不在线`);
      }
      if (binding.status !== "ready") {
        warnings.push(`目录「${binding.root_ref}」状态为${bindingStatusText(binding.status)}`);
      }
      return { binding, isAuthorized, isOnline, isReady: binding.status === "ready" };
    })
    .filter((candidate) => candidate.isAuthorized && candidate.isReady);

  const defaultCandidate = workspace.default_binding_id
    ? candidates.find((candidate) => candidate.binding.id === workspace.default_binding_id)
    : undefined;
  const onlineCandidates = candidates.filter((candidate) => candidate.isOnline);
  const firstOnline = onlineCandidates
    .slice()
    .sort((left, right) => right.binding.priority - left.binding.priority)[0];
  const firstCandidate = candidates[0];

  const selected = workspace.resolution_policy === "prefer_default_binding"
    ? defaultCandidate ?? firstOnline ?? firstCandidate
    : firstOnline ?? defaultCandidate ?? firstCandidate;

  if (!selected) {
    return {
      state: "blocked",
      label: "无法运行",
      description: warnings[0] ?? "当前没有可用的运行位置。",
      binding: null,
      warnings,
    };
  }

  const backendName = backendDisplayName(backends, selected.binding.backend_id);
  const fallback = workspace.default_binding_id
    && selected.binding.id !== workspace.default_binding_id;
  return {
    state: fallback || warnings.length > 0 ? "warning" : "resolved",
    label: fallback ? "已切换到备用运行位置" : "已就绪",
    description: `${backendName} @ ${selected.binding.root_ref}`,
    binding: selected.binding,
    warnings,
  };
}
