import type {
  BackendConfig,
  ProjectBackendAccess,
  Workspace,
} from "../../../types";

/**
 * 机器（运行环境）呈现层模型。
 *
 * 平台心智围绕「在哪台机器上运行」，必须一眼区分：
 * - 本机（这台设备）：desktop local runtime，registration_source = desktop_access_token
 * - 服务器 runner：standalone runner，registration_source = runner_registration_token
 *
 * 事实源是 backend.registration_source（后端收束后稳定写入），UI 直接据此打标，
 * 不再让用户从在线/scope 等状态去推断。此模块为纯函数，便于测试且不触碰
 * workspaceRouting 的解析行为。
 */

export type MachineKind = "local_device" | "server_runner" | "other";

export const LOCAL_DEVICE_REGISTRATION_SOURCE = "desktop_access_token";
export const SERVER_RUNNER_REGISTRATION_SOURCE = "runner_registration_token";

export function classifyMachine(backend: Pick<BackendConfig, "registration_source">): MachineKind {
  switch (backend.registration_source) {
    case LOCAL_DEVICE_REGISTRATION_SOURCE:
      return "local_device";
    case SERVER_RUNNER_REGISTRATION_SOURCE:
      return "server_runner";
    default:
      return "other";
  }
}

const MACHINE_KIND_LABELS: Record<MachineKind, string> = {
  local_device: "本机（这台设备）",
  server_runner: "服务器 runner",
  other: "机器",
};

export function machineKindLabel(kind: MachineKind): string {
  return MACHINE_KIND_LABELS[kind];
}

export interface MachineAvailabilityEntry {
  backendId: string;
  name: string;
  kind: MachineKind;
  online: boolean;
  /** 该 workspace 是否已在这台机器上定位到 ready 目录绑定。 */
  located: boolean;
}

function activeAccessIds(accesses: ProjectBackendAccess[]): Set<string> {
  return new Set(
    accesses
      .filter((access) => access.status === "active")
      .map((access) => access.backend_id),
  );
}

/**
 * 在已授权机器维度上汇总某个 workspace 的「可用机器」内联呈现数据：
 * 「本机 ✓ / 服务器 A 未定位」。located 表示该机器上存在 ready 的目录绑定。
 *
 * 纯派生：只读现有 backends / accesses / workspace.bindings，不改变路由解析行为。
 */
export function workspaceMachineAvailability(
  workspace: Workspace,
  backends: BackendConfig[],
  accesses: ProjectBackendAccess[],
): MachineAvailabilityEntry[] {
  const authorized = activeAccessIds(accesses);
  const readyBackendIds = new Set(
    workspace.bindings
      .filter((binding) => binding.status === "ready")
      .map((binding) => binding.backend_id),
  );

  return backends
    .filter((backend) => authorized.has(backend.id))
    .map((backend) => ({
      backendId: backend.id,
      name: backend.name,
      kind: classifyMachine(backend),
      online: backend.online === true,
      located: readyBackendIds.has(backend.id),
    }))
    .sort((left, right) => {
      // 本机优先展示，其次服务器 runner，再其他；同类按已定位优先。
      const kindOrder = (kind: MachineKind) =>
        kind === "local_device" ? 0 : kind === "server_runner" ? 1 : 2;
      const byKind = kindOrder(left.kind) - kindOrder(right.kind);
      if (byKind !== 0) return byKind;
      if (left.located !== right.located) return left.located ? -1 : 1;
      return left.name.localeCompare(right.name);
    });
}
