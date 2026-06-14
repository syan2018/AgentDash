/**
 * MCP 共享 UI 组件（跨 feature 复用）。
 *
 * 组件实现已迁至 @agentdash/views/mcp-shared，此处仅做 re-export
 * 以保持 app-web 内部引用路径不变。
 */

export {
  McpTransportConfigEditor,
  KeyValueList,
  StringList,
} from '@agentdash/views/mcp-shared'
export type {
  McpTransportConfigEditorEntry,
  McpTransportConfigEditorProps,
  McpTransportConfigEditorValue,
} from '@agentdash/views/mcp-shared'
export { createDefaultMcpTransportConfig } from './helpers'
