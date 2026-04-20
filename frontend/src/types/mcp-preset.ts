// ─── MCP Preset ──────────────────────────────────────
//
// 对齐后端 DTO：crates/agentdash-api/src/dto/mcp_preset.rs
// Domain 复用 frontend/src/types/index.ts 中的 McpServerDecl（http / sse / stdio 三种 transport）。
//
// 字段命名严格 snake_case，与后端 JSON 保持一致；无需做 camelCase 转换。

import type { McpServerDecl } from "./index";

/**
 * MCP Preset 来源标签。
 *
 * - `builtin`：内置模板，只读，仅允许"复制为 user"
 * - `user`：用户创建 / 克隆出来的可编辑副本
 */
export type McpPresetSource = "builtin" | "user";

/**
 * MCP Preset 响应体（GET / POST / PUT 的返回值）。
 *
 * 对齐后端 `McpPresetResponse`（crates/agentdash-api/src/dto/mcp_preset.rs）。
 */
export interface McpPresetDto {
  id: string;
  project_id: string;
  name: string;
  /** 可为 null（清空），也可能字段缺失（后端 serde skip_serializing_if） */
  description?: string | null;
  server_decl: McpServerDecl;
  source: McpPresetSource;
  /** 仅 `source === "builtin"` 时非空 */
  builtin_key?: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * 创建 user MCP Preset 的请求体。
 *
 * 对齐后端 `CreateMcpPresetRequest`。
 */
export interface CreateMcpPresetRequest {
  name: string;
  description?: string | null;
  server_decl: McpServerDecl;
}

/**
 * 更新 MCP Preset 的请求体（PATCH，字段均可选）。
 *
 * `description` 支持三态语义：
 * - 字段缺失（`undefined`）→ 保持原值不变
 * - 显式 `null` → 清空 description（后端 `Some(None)`）
 * - 字符串 → 更新为新值
 *
 * 对齐后端 `UpdateMcpPresetRequest`。
 */
export interface UpdateMcpPresetRequest {
  name?: string;
  description?: string | null;
  server_decl?: McpServerDecl;
}

/**
 * 复制 Preset 为 user 副本的请求体。
 *
 * `name` 为空时后端会回退到 `"<原 name> (copy)"`。
 */
export interface CloneMcpPresetRequest {
  name?: string;
}

/**
 * 装载 builtin Preset 的请求体。
 *
 * - `builtin_key` 缺省 → 装载全部内置模板（幂等）
 * - `builtin_key` 指定 → 仅装载对应模板
 */
export interface BootstrapMcpPresetRequest {
  builtin_key?: string;
}

/**
 * 列表查询参数。
 *
 * - `source` 缺省 / 空字符串 → 不过滤
 * - `source === "user" | "builtin"` → 按来源筛选
 */
export interface ListMcpPresetQuery {
  source?: McpPresetSource;
}
