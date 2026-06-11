/**
 * MCP 共享工具函数。
 *
 * 独立于组件文件，避免 React Fast Refresh 对混合导出告警。
 */

import type {
  CreateMcpPresetRequest,
  McpPresetDto,
  McpRoutePolicy,
  McpRuntimeBindingConfig,
  McpRuntimeBindingRule,
  McpRuntimeBindingSource,
  McpRuntimeBindingTarget,
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
  runtime_binding: McpRuntimeBindingConfig | null;
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

export function createDefaultMcpRuntimeBindingRule(
  transportType: McpTransportConfig["type"] = "http",
): McpRuntimeBindingRule {
  return {
    source: { kind: "workspace_detected_fact", path: ["p4", "client_name"] },
    target:
      transportType === "stdio"
        ? { kind: "stdio_env", name: "P4CLIENT" }
        : { kind: "http_query", name: "p4_client" },
    required: true,
  };
}

export function buildMcpPresetFormState(preset?: McpPresetDto | null): McpPresetFormState {
  if (!preset) {
    return {
      key: "",
      display_name: "",
      description: "",
      transport: createDefaultMcpTransportConfig(),
      route_policy: "auto",
      runtime_binding: null,
    };
  }

  return {
    key: preset.key,
    display_name: preset.display_name,
    description: preset.description ?? "",
    transport: preset.transport,
    route_policy: preset.route_policy,
    runtime_binding: normalizeMcpRuntimeBindingForForm(preset.runtime_binding),
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

  const runtimeBindingError = validateRuntimeBinding(form);
  if (runtimeBindingError) return runtimeBindingError;

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
  const runtimeBinding = normalizeMcpRuntimeBindingForRequest(form.runtime_binding);
  if (runtimeBinding) {
    input.runtime_binding = runtimeBinding;
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

  const currentRuntimeBinding = normalizeMcpRuntimeBindingForRequest(current.runtime_binding);
  const originalRuntimeBinding = normalizeMcpRuntimeBindingForRequest(original.runtime_binding);
  if (JSON.stringify(currentRuntimeBinding ?? null) !== JSON.stringify(originalRuntimeBinding ?? null)) {
    patch.runtime_binding = currentRuntimeBinding ?? null;
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

export function hasMcpRuntimeBinding(
  runtimeBinding: McpRuntimeBindingConfig | null | undefined,
): boolean {
  return mcpRuntimeBindingRuleCount(runtimeBinding) > 0;
}

export function mcpRuntimeBindingRuleCount(
  runtimeBinding: McpRuntimeBindingConfig | null | undefined,
): number {
  return runtimeBinding?.bindings?.length ?? 0;
}

export function normalizeMcpRuntimeBindingForForm(
  runtimeBinding: McpRuntimeBindingConfig | null | undefined,
): McpRuntimeBindingConfig | null {
  const normalized = normalizeMcpRuntimeBindingForRequest(runtimeBinding);
  return normalized ?? null;
}

export function normalizeMcpRuntimeBindingForRequest(
  runtimeBinding: McpRuntimeBindingConfig | null | undefined,
): McpRuntimeBindingConfig | undefined {
  const rawRules = runtimeBinding?.bindings ?? [];
  const bindings = rawRules.map(normalizeRuntimeBindingRule);
  if (bindings.length === 0) {
    return undefined;
  }

  const mount_id = runtimeBinding?.mount_id?.trim();
  return mount_id ? { mount_id, bindings } : { bindings };
}

function normalizeRuntimeBindingRule(rule: McpRuntimeBindingRule): McpRuntimeBindingRule {
  return {
    source: normalizeRuntimeBindingSource(rule.source),
    target: normalizeRuntimeBindingTarget(rule.target),
    required: rule.required,
  };
}

function normalizeRuntimeBindingSource(
  source: McpRuntimeBindingSource,
): McpRuntimeBindingSource {
  if (source.kind === "workspace_identity" || source.kind === "workspace_detected_fact") {
    return {
      ...source,
      path: source.path.map((segment) => segment.trim()).filter(Boolean),
    };
  }
  return source;
}

function normalizeRuntimeBindingTarget(
  target: McpRuntimeBindingTarget,
): McpRuntimeBindingTarget {
  if (target.kind === "http_query" || target.kind === "http_header" || target.kind === "stdio_env") {
    return {
      ...target,
      name: target.name.trim(),
    };
  }
  return target;
}

function validateRuntimeBinding(form: McpPresetFormState): string | null {
  const rules = form.runtime_binding?.bindings ?? [];
  for (let index = 0; index < rules.length; index += 1) {
    const rule = normalizeRuntimeBindingRule(rules[index]);
    const row = `运行时绑定第 ${index + 1} 条`;
    if (
      (rule.source.kind === "workspace_identity" ||
        rule.source.kind === "workspace_detected_fact") &&
      rule.source.path.length === 0
    ) {
      return `${row} 的 source path 不能为空`;
    }

    if (
      (rule.target.kind === "http_query" ||
        rule.target.kind === "http_header" ||
        rule.target.kind === "stdio_env") &&
      !rule.target.name
    ) {
      return `${row} 的 target 名称不能为空`;
    }

    if (form.transport.type === "stdio") {
      if (rule.target.kind === "http_query" || rule.target.kind === "http_header") {
        return `${row} 的 HTTP target 不能用于 stdio transport`;
      }
    } else if (rule.target.kind === "stdio_env" || rule.target.kind === "stdio_cwd") {
      return `${row} 的 stdio target 不能用于 ${form.transport.type} transport`;
    }
  }
  return null;
}
