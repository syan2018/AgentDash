// ─── MCP Preset ──────────────────────────────────────
//
// 对齐后端 DTO：crates/agentdash-api/src/dto/mcp_preset.rs
// Domain 复用 frontend/src/types/index.ts 中的 McpTransportConfig / McpRoutePolicy。
//
// 字段命名严格 snake_case，与后端 JSON 保持一致；无需做 camelCase 转换。

import type { McpRoutePolicy, McpTransportConfig } from "./index";

/**
 * MCP Preset 来源标签。
 *
 * - `builtin`：内置模板，只读，仅允许"复制为 user"
 * - `user`：用户创建 / 克隆出来的可编辑副本
 */
export type McpPresetSource = "builtin" | "user";

/**
 * MCP Preset 响应体（GET / POST / PUT 的返回值）。
 */
export interface McpPresetDto {
  id: string;
  project_id: string;
  /** 项目内唯一，也是 agent-facing server name */
  key: string;
  /** 纯展示名称 */
  display_name: string;
  /** 可为 null（清空），也可能字段缺失（后端 serde skip_serializing_if） */
  description?: string | null;
  /** 纯 transport 配置，不包含展示名或 route 语义 */
  transport: McpTransportConfig;
  /** 应用层路由策略 */
  route_policy: McpRoutePolicy;
  source: McpPresetSource;
  /** 仅 `source === "builtin"` 时非空 */
  builtin_key?: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * 创建 user MCP Preset 的请求体。
 */
export interface CreateMcpPresetRequest {
  key: string;
  display_name: string;
  description?: string | null;
  transport: McpTransportConfig;
  route_policy?: McpRoutePolicy;
}

/**
 * 更新 MCP Preset 的请求体（PATCH，字段均可选）。
 *
 * `description` 支持三态语义：
 * - 字段缺失（`undefined`）→ 保持原值不变
 * - 显式 `null` → 清空 description
 * - 字符串 → 更新为新值
 */
export interface UpdateMcpPresetRequest {
  key?: string;
  display_name?: string;
  description?: string | null;
  transport?: McpTransportConfig;
  route_policy?: McpRoutePolicy;
}

/**
 * 复制 Preset 为 user 副本的请求体。
 *
 * `key` 为空时后端会回退到 `"<原 key>-copy"`。
 * `display_name` 为空时后端会回退到 `"<原 display_name> (copy)"`。
 */
export interface CloneMcpPresetRequest {
  key?: string;
  display_name?: string;
}

/**
 * 装载 builtin Preset 的请求体。
 */
export interface BootstrapMcpPresetRequest {
  builtin_key?: string;
}

/**
 * 列表查询参数。
 */
export interface ListMcpPresetQuery {
  source?: McpPresetSource;
}
