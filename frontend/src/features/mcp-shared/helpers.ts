/**
 * MCP 共享工具函数。
 *
 * 独立于组件文件，避免 React Fast Refresh 对混合导出告警。
 */

import type { McpTransportConfig } from "../../types";

/**
 * 构造一个空白的 MCP transport（默认 http）。
 * 供 Preset 新建表单初始化使用，保持 discriminated union narrow 成立。
 */
export function createDefaultMcpTransportConfig(): McpTransportConfig {
  return { type: "http", url: "", headers: [] };
}
