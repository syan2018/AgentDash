/**
 * MCP 共享工具函数。
 *
 * 独立于组件文件，避免 React Fast Refresh 对混合导出告警。
 */

import type {
  CreateMcpPresetRequest,
  McpPresetDto,
  McpRoutePolicy,
  McpTransportConfig,
  UpdateMcpPresetRequest,
} from "../../types";

export interface McpPresetFormState {
  key: string;
  display_name: string;
  /** 直接映射到 textarea；空串在 update 时表示清空描述。 */
  description: string;
  transport: McpTransportConfig;
  route_policy: McpRoutePolicy;
}

export const MCP_ROUTE_POLICY_OPTIONS: ReadonlyArray<{
  value: McpRoutePolicy;
  label: string;
}> = [
  { value: "auto", label: "auto（stdio 走 relay，http/sse 直连）" },
  { value: "relay", label: "relay（强制经本机）" },
  { value: "direct", label: "direct（强制直连）" },
];

/**
 * 构造一个空白的 MCP transport（默认 http）。
 * 供 Preset 新建表单初始化使用，保持 discriminated union narrow 成立。
 */
export function createDefaultMcpTransportConfig(): McpTransportConfig {
  return { type: "http", url: "", headers: [] };
}

export function buildMcpPresetFormState(preset?: McpPresetDto | null): McpPresetFormState {
  if (!preset) {
    return {
      key: "",
      display_name: "",
      description: "",
      transport: createDefaultMcpTransportConfig(),
      route_policy: "auto",
    };
  }

  return {
    key: preset.key,
    display_name: preset.display_name,
    description: preset.description ?? "",
    transport: preset.transport,
    route_policy: preset.route_policy,
  };
}

export function validateMcpPresetForm(form: McpPresetFormState): string | null {
  const trimmedKey = form.key.trim();
  const trimmedDisplayName = form.display_name.trim();
  if (!trimmedKey) return "工具标识不能为空";
  if (!trimmedDisplayName) return "显示名称不能为空";
  if (trimmedKey.startsWith("agentdash-")) return "工具标识不能使用保留前缀 agentdash-";
  if (trimmedKey.includes("::")) return "工具标识不能包含 ::";
  if (/[\\/:\\s]/.test(trimmedKey)) return "工具标识不能包含空白、冒号或路径分隔符";

  if (form.transport.type === "http" || form.transport.type === "sse") {
    if (!form.transport.url.trim()) return "URL 不能为空";
    try {
      new URL(form.transport.url.trim());
    } catch {
      return "URL 格式非法";
    }
  }

  if (form.transport.type === "stdio" && !form.transport.command.trim()) {
    return "Command 不能为空";
  }

  return null;
}

export function buildCreateMcpPresetRequest(
  form: McpPresetFormState,
): CreateMcpPresetRequest {
  const input: CreateMcpPresetRequest = {
    key: form.key.trim(),
    display_name: form.display_name.trim(),
    transport: form.transport,
    route_policy: form.route_policy,
  };
  const trimmedDesc = form.description.trim();
  if (trimmedDesc) {
    input.description = trimmedDesc;
  }
  return input;
}

export function buildUpdateMcpPresetPatch(
  current: McpPresetFormState,
  original: McpPresetDto,
): UpdateMcpPresetRequest {
  const patch: UpdateMcpPresetRequest = {};
  const trimmedKey = current.key.trim();
  if (trimmedKey !== original.key) {
    patch.key = trimmedKey;
  }

  const trimmedDisplayName = current.display_name.trim();
  if (trimmedDisplayName !== original.display_name) {
    patch.display_name = trimmedDisplayName;
  }

  const currentDesc = current.description.trim();
  const originalDesc = (original.description ?? "").trim();
  if (currentDesc !== originalDesc) {
    patch.description = currentDesc ? currentDesc : null;
  }

  if (JSON.stringify(current.transport) !== JSON.stringify(original.transport)) {
    patch.transport = current.transport;
  }

  if (current.route_policy !== original.route_policy) {
    patch.route_policy = current.route_policy;
  }

  return patch;
}

export function readMcpRoutePolicy(value: string): McpRoutePolicy {
  const option = MCP_ROUTE_POLICY_OPTIONS.find((item) => item.value === value);
  if (!option) {
    throw new Error(`未知 MCP route policy: ${value}`);
  }
  return option.value;
}
