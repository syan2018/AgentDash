/**
 * Workspace service 层。
 *
 * 收口 workspace 相关的 api.client 调用。后端响应已是结构化 Workspace，无需
 * 额外 mapper；service 仅负责请求路径与 payload 组装，workspaceStore 只消费此层。
 */

import { api } from "../api/client";
import type {
  ContextContainerCapability,
  Workspace,
  WorkspaceBindingStatus,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from "../types";

export interface WorkspaceBindingInput {
  id?: string;
  backend_id: string;
  root_ref: string;
  status?: WorkspaceBindingStatus;
  detected_facts?: Record<string, unknown>;
  priority?: number;
}

export interface CreateWorkspaceOpts {
  identity_kind?: WorkspaceIdentityKind;
  identity_payload?: Record<string, unknown>;
  resolution_policy?: WorkspaceResolutionPolicy;
  bindings?: WorkspaceBindingInput[];
  shortcut_binding?: WorkspaceBindingInput;
  mount_capabilities?: ContextContainerCapability[];
}

export interface UpdateWorkspacePayload {
  name?: string;
  identity_kind?: WorkspaceIdentityKind;
  identity_payload?: Record<string, unknown>;
  resolution_policy?: WorkspaceResolutionPolicy;
  default_binding_id?: string | null;
  bindings?: WorkspaceBindingInput[];
  mount_capabilities?: ContextContainerCapability[];
}

export async function fetchWorkspaces(projectId: string): Promise<Workspace[]> {
  return api.get<Workspace[]>(`/projects/${projectId}/workspaces`);
}

export async function detectWorkspace(
  projectId: string,
  backendId: string,
  rootRef: string,
): Promise<WorkspaceDetectionResult> {
  return api.post<WorkspaceDetectionResult>(`/projects/${projectId}/workspaces/detect`, {
    backend_id: backendId,
    root_ref: rootRef,
  });
}

export async function createWorkspace(
  projectId: string,
  name: string,
  opts?: CreateWorkspaceOpts,
): Promise<Workspace> {
  return api.post<Workspace>(`/projects/${projectId}/workspaces`, {
    name,
    identity_kind: opts?.identity_kind,
    identity_payload: opts?.identity_payload,
    resolution_policy: opts?.resolution_policy ?? "prefer_online",
    bindings: opts?.bindings,
    shortcut_binding: opts?.shortcut_binding,
    mount_capabilities: opts?.mount_capabilities,
  });
}

export async function updateWorkspace(
  id: string,
  payload: UpdateWorkspacePayload,
): Promise<Workspace> {
  return api.put<Workspace>(`/workspaces/${id}`, payload);
}

export async function updateWorkspaceStatus(id: string, status: WorkspaceStatus): Promise<void> {
  await api.patch(`/workspaces/${id}/status`, { status });
}

export async function deleteWorkspace(id: string): Promise<void> {
  await api.delete(`/workspaces/${id}`);
}
